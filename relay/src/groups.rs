//! Groups (`docs/protocol.md`, section 5): which devices may exchange
//! envelopes. A device is in at most one group and a group always has at
//! least two devices. Every change is saved before it takes effect.

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Bytes;
use weft_protocol::{MAX_GROUP_SIZE, RouteHeader};

use crate::store::Store;
use crate::{DeviceId, duration_ms, short_id};

/// Local to the relay and never sent on the wire.
pub type GroupId = i64;

/// One step of a group change, as stored and as applied in memory.
pub enum Change {
    Join(DeviceId, GroupId),
    Leave(DeviceId),
}

pub struct Groups {
    store: Store,
    group_of: HashMap<DeviceId, GroupId>,
    members: HashMap<GroupId, Vec<DeviceId>>,
    next_group_id: GroupId,
}

impl Groups {
    /// Loads the groups saved in `store`.
    pub fn load(store: Store) -> rusqlite::Result<Groups> {
        let mut groups = Groups {
            store,
            group_of: HashMap::new(),
            members: HashMap::new(),
            next_group_id: 1,
        };
        for (device_id, group_id) in groups.store.load()? {
            groups.insert(device_id, group_id);
        }
        tracing::info!(groups = groups.members.len(), "groups loaded");
        Ok(groups)
    }

    pub fn same_group(&self, a: &DeviceId, b: &DeviceId) -> bool {
        match (self.group_of.get(a), self.group_of.get(b)) {
            (Some(group_a), Some(group_b)) => group_a == group_b,
            _ => false,
        }
    }

    /// Puts `joiner` in the group of `owner`, creating the group if `owner`
    /// had none. `joiner` leaves its previous group first. Returns
    /// `Ok(false)`, changing nothing, if the group of `owner` is full.
    pub fn add(&mut self, owner: &DeviceId, joiner: &DeviceId) -> rusqlite::Result<bool> {
        if self.same_group(owner, joiner) {
            return Ok(true);
        }
        let existing = self.group_of.get(owner).copied();
        if existing.is_some_and(|group| self.members[&group].len() >= MAX_GROUP_SIZE) {
            return Ok(false);
        }
        let mut changes = self.leave_changes(joiner);
        let group = existing.unwrap_or_else(|| {
            changes.push(Change::Join(*owner, self.next_group_id));
            self.next_group_id
        });
        changes.push(Change::Join(*joiner, group));
        self.commit(changes)?;
        Ok(true)
    }

    /// Takes `device` out of its group, if it has one.
    pub fn remove(&mut self, device: &DeviceId) -> rusqlite::Result<()> {
        let changes = self.leave_changes(device);
        self.commit(changes)
    }

    /// Queues an envelope frame for an offline group member.
    pub fn queue_offline(
        &mut self,
        header: &RouteHeader,
        frame: &[u8],
        now_ms: i64,
        ttl: Duration,
        max_per_device: usize,
    ) -> rusqlite::Result<()> {
        self.store
            .queue_envelope(header, frame, now_ms, duration_ms(ttl), max_per_device)
    }

    /// Loads non-expired queued envelopes for `recipient` in FIFO order.
    pub fn load_offline(
        &mut self,
        recipient: &DeviceId,
        now_ms: i64,
        ttl: Duration,
    ) -> rusqlite::Result<Vec<(i64, Bytes)>> {
        self.store.load_pending(recipient, now_ms, duration_ms(ttl))
    }

    /// Deletes one queued envelope after delivery.
    pub fn delete_offline(&mut self, id: i64) -> rusqlite::Result<()> {
        self.store.delete_pending(id)
    }

    /// What it takes for `device` to leave its group. A group that would be
    /// left with a single device is deleted: that device leaves too.
    fn leave_changes(&self, device: &DeviceId) -> Vec<Change> {
        let Some(group) = self.group_of.get(device) else {
            return Vec::new();
        };
        let members = &self.members[group];
        if members.len() <= 2 {
            members.iter().copied().map(Change::Leave).collect()
        } else {
            vec![Change::Leave(*device)]
        }
    }

    /// Saves the changes, then applies them in memory. If saving fails,
    /// nothing changes.
    fn commit(&mut self, changes: Vec<Change>) -> rusqlite::Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        self.store.apply(&changes)?;
        for change in changes {
            match change {
                Change::Join(device_id, group_id) => {
                    if !self.members.contains_key(&group_id) {
                        tracing::info!(group = group_id, "group created");
                    }
                    self.insert(device_id, group_id);
                    tracing::info!(device = %short_id(&device_id), group = group_id, "device joined group");
                }
                Change::Leave(device_id) => self.leave(&device_id),
            }
        }
        Ok(())
    }

    fn insert(&mut self, device_id: DeviceId, group_id: GroupId) {
        self.group_of.insert(device_id, group_id);
        self.members.entry(group_id).or_default().push(device_id);
        self.next_group_id = self.next_group_id.max(group_id + 1);
    }

    fn leave(&mut self, device_id: &DeviceId) {
        let Some(group_id) = self.group_of.remove(device_id) else {
            return;
        };
        tracing::info!(device = %short_id(device_id), group = group_id, "device left group");
        let members = self.members.get_mut(&group_id).expect("group has members");
        members.retain(|member| member != device_id);
        if members.is_empty() {
            self.members.remove(&group_id);
            tracing::info!(group = group_id, "group deleted");
        }
    }
}
