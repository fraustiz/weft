//! Persistence of the groups in SQLite. Pairing slots and pending joins are
//! deliberately not stored: they live for a few minutes at most.

use std::path::Path;

use rusqlite::Connection;

use crate::DeviceId;
use crate::groups::{Change, GroupId};

// The primary key on `device_id` enforces "at most one group per device".
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS group_members (
        device_id BLOB PRIMARY KEY,
        group_id  INTEGER NOT NULL
    ) STRICT;
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

    /// Saves all the changes, or none of them.
    pub fn apply(&mut self, changes: &[Change]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                Change::Join(device_id, group_id) => tx.execute(
                    "INSERT OR REPLACE INTO group_members (device_id, group_id) VALUES (?1, ?2)",
                    (device_id, group_id),
                )?,
                Change::Leave(device_id) => tx.execute(
                    "DELETE FROM group_members WHERE device_id = ?1",
                    (device_id,),
                )?,
            };
        }
        tx.commit()
    }
}
