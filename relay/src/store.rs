//! Persistence of groups and offline envelopes in SQLite. Pairing slots and
//! pending joins are deliberately not stored: they live for a few minutes at
//! most.

use std::path::Path;

use axum::body::Bytes;
use rusqlite::Connection;
use weft_protocol::{Kind, RouteHeader};

use crate::DeviceId;
use crate::groups::{Change, GroupId};

// The primary key on `device_id` enforces "at most one group per device".
// `last_active_ms` is the wall-clock time a group member was last seen; the
// relay prunes a group once all its members have been idle past the TTL.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS group_members (
        device_id      BLOB PRIMARY KEY,
        group_id       INTEGER NOT NULL,
        last_active_ms INTEGER NOT NULL DEFAULT 0
    ) STRICT;

    CREATE TABLE IF NOT EXISTS pending_envelopes (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        recipient BLOB NOT NULL,
        sender    BLOB NOT NULL,
        kind      INTEGER NOT NULL,
        stored_ms INTEGER NOT NULL,
        frame     BLOB NOT NULL
    ) STRICT;

    CREATE INDEX IF NOT EXISTS pending_by_recipient
        ON pending_envelopes (recipient, id);
";

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (and creates if needed) the database at `path`. `:memory:`
    /// gives a private database that disappears with the store. Existing
    /// databases are migrated in place, without losing any group.
    pub fn open(path: &Path) -> rusqlite::Result<Store> {
        Store::open_at(path, crate::now_ms())
    }

    /// Like [`Store::open`], with the clock injected for tests.
    pub fn open_at(path: &Path, now_ms: i64) -> rusqlite::Result<Store> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate_last_active(&conn, now_ms)?;
        Ok(Store { conn })
    }

    /// Every device that is in a group, with its group and last-seen time.
    pub fn load(&self) -> rusqlite::Result<Vec<(DeviceId, GroupId, i64)>> {
        let mut statement = self
            .conn
            .prepare("SELECT device_id, group_id, last_active_ms FROM group_members")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect()
    }

    /// Saves all the changes, or none of them. Leaving a group also wipes
    /// any envelopes queued for that recipient. A join records `now_ms` as
    /// the member's last-seen time.
    pub fn apply(&mut self, changes: &[Change], now_ms: i64) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                Change::Join(device_id, group_id) => {
                    tx.execute(
                        "INSERT OR REPLACE INTO group_members
                             (device_id, group_id, last_active_ms) VALUES (?1, ?2, ?3)",
                        (device_id, group_id, now_ms),
                    )?;
                }
                Change::Leave(device_id) => {
                    tx.execute(
                        "DELETE FROM group_members WHERE device_id = ?1",
                        (device_id,),
                    )?;
                    tx.execute(
                        "DELETE FROM pending_envelopes WHERE recipient = ?1",
                        (device_id,),
                    )?;
                }
            }
        }
        tx.commit()
    }

    /// Marks `device_id` as active now, so its group is not pruned.
    pub fn touch(&mut self, device_id: &DeviceId, now_ms: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE group_members SET last_active_ms = ?2 WHERE device_id = ?1",
            (device_id, now_ms),
        )?;
        Ok(())
    }

    /// Queues an encrypted envelope frame for an offline `recipient`, after
    /// pruning expired entries, replacing any queued `DeviceName` (`0x06`)
    /// from the same `sender`, and trimming the oldest queued entries for
    /// `recipient` to `max_per_device`.
    pub fn queue_envelope(
        &mut self,
        header: &RouteHeader,
        frame: &[u8],
        now_ms: i64,
        ttl_ms: i64,
        max_per_device: usize,
    ) -> rusqlite::Result<()> {
        let cutoff_ms = now_ms.saturating_sub(ttl_ms);
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM pending_envelopes WHERE stored_ms <= ?1",
            (cutoff_ms,),
        )?;
        if max_per_device == 0 {
            return tx.commit();
        }
        let kind = i64::from(header.kind);
        if header.kind == Kind::DeviceName as u8 {
            tx.execute(
                "DELETE FROM pending_envelopes WHERE recipient = ?1 AND sender = ?2 AND kind = ?3",
                (&header.recipient, &header.sender, kind),
            )?;
        }
        tx.execute(
            "INSERT INTO pending_envelopes (recipient, sender, kind, stored_ms, frame)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (&header.recipient, &header.sender, kind, now_ms, frame),
        )?;
        let keep = i64::try_from(max_per_device).unwrap_or(i64::MAX);
        tx.execute(
            "DELETE FROM pending_envelopes WHERE id IN (
                 SELECT id FROM pending_envelopes
                 WHERE recipient = ?1
                 ORDER BY id DESC
                 LIMIT -1 OFFSET ?2
             )",
            (&header.recipient, keep),
        )?;
        tx.commit()
    }

    /// Prunes expired envelopes for `recipient` and returns the remaining
    /// ones in FIFO (`id ASC`) order.
    pub fn load_pending(
        &mut self,
        recipient: &DeviceId,
        now_ms: i64,
        ttl_ms: i64,
    ) -> rusqlite::Result<Vec<(i64, Bytes)>> {
        let cutoff_ms = now_ms.saturating_sub(ttl_ms);
        self.conn.execute(
            "DELETE FROM pending_envelopes WHERE recipient = ?1 AND stored_ms <= ?2",
            (recipient, cutoff_ms),
        )?;
        let mut statement = self.conn.prepare(
            "SELECT id, frame FROM pending_envelopes WHERE recipient = ?1 ORDER BY id ASC",
        )?;
        let rows = statement.query_map((recipient,), |row| {
            let id: i64 = row.get(0)?;
            let frame: Vec<u8> = row.get(1)?;
            Ok((id, Bytes::from(frame)))
        })?;
        rows.collect()
    }

    /// Deletes one queued envelope once it has been sent on the socket.
    pub fn delete_pending(&mut self, id: i64) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM pending_envelopes WHERE id = ?1", (id,))?;
        Ok(())
    }
}

/// Adds the `last_active_ms` column to a database created before it existed,
/// backfilling existing rows with `now_ms` so no group is ever treated as
/// stale (and deleted) just because it predates this version.
fn migrate_last_active(conn: &Connection, now_ms: i64) -> rusqlite::Result<()> {
    let has_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('group_members') WHERE name = 'last_active_ms'")?
        .exists([])?;
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE group_members ADD COLUMN last_active_ms INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    // Any row still at 0 (fresh migration, or a DEFAULT 0 insert from an older
    // binary) is treated as "seen now", never as stale.
    conn.execute(
        "UPDATE group_members SET last_active_ms = ?1 WHERE last_active_ms = 0",
        (now_ms,),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::Change;

    fn temp_db() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("weft-store-{}-{nanos}.sqlite3", std::process::id()))
    }

    #[test]
    fn migration_backfills_zero_last_active_to_now() {
        let path = temp_db();
        {
            let mut store = Store::open_at(&path, 1_000).unwrap();
            // A row left at 0, as an older binary (DEFAULT 0 column) would.
            store.apply(&[Change::Join([9u8; 16], 1)], 0).unwrap();
            assert_eq!(store.load().unwrap(), vec![([9u8; 16], 1, 0)]);
        }
        // Reopening backfills 0 to the open time, so an existing group is not
        // seen as ancient (and pruned) merely because it predates the column.
        let store = Store::open_at(&path, 5_000).unwrap();
        assert_eq!(store.load().unwrap(), vec![([9u8; 16], 1, 5_000)]);
        let _ = std::fs::remove_file(&path);
    }
}
