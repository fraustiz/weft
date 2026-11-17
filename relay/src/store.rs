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
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS group_members (
        device_id BLOB PRIMARY KEY,
        group_id  INTEGER NOT NULL
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
    /// gives a private database that disappears with the store.
    pub fn open(path: &Path) -> rusqlite::Result<Store> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Every device that is in a group, with its group.
    pub fn load(&self) -> rusqlite::Result<Vec<(DeviceId, GroupId)>> {
        let mut statement = self
            .conn
            .prepare("SELECT device_id, group_id FROM group_members")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    /// Saves all the changes, or none of them. Leaving a group also wipes
    /// any envelopes queued for that recipient.
    pub fn apply(&mut self, changes: &[Change]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                Change::Join(device_id, group_id) => {
                    tx.execute(
                        "INSERT OR REPLACE INTO group_members (device_id, group_id) VALUES (?1, ?2)",
                        (device_id, group_id),
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
