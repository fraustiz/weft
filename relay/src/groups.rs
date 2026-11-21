//! Groups (`docs/protocol.md`, section 5): which devices may exchange
//! envelopes. A device is in at most one group and a group always has at
//! least two devices. Every change is saved before it takes effect.
//!
//! Two caps keep an abusive client from growing the state without bound: a
//! global ceiling on the number of groups, and a time-to-live after which a
//! group whose members have all gone idle is pruned.

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
    /// Most recent activity of any member of a group, in wall-clock ms.
    last_active: HashMap<GroupId, i64>,
    next_group_id: GroupId,
    /// Largest number of groups the relay keeps at once.
    max_groups: usize,
}

impl Groups {
    /// Loads the groups saved in `store`, capping their number at `max_groups`.
    pub fn load(store: Store, max_groups: usize) -> rusqlite::Result<Groups> {
        let mut groups = Groups {
            store,
            group_of: HashMap::new(),
            members: HashMap::new(),
            last_active: HashMap::new(),
            next_group_id: 1,
            max_groups,
        };
        for (device_id, group_id, last_active) in groups.store.load()? {
            groups.insert(device_id, group_id, last_active);
        }
        tracing::info!(groups = groups.members.len(), max_groups, "groups loaded");
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
    /// `Ok(false)`, changing nothing, if the group of `owner` is full or if
    /// a new group would push the relay past its global cap.
    pub fn add(
        &mut self,
        owner: &DeviceId,
        joiner: &DeviceId,
        now_ms: i64,
    ) -> rusqlite::Result<bool> {
        if self.same_group(owner, joiner) {
            return Ok(true);
        }
        let existing = self.group_of.get(owner).copied();
        if existing.is_some_and(|group| self.members[&group].len() >= MAX_GROUP_SIZE) {
            return Ok(false);
        }
        // A fresh group is about to be created: refuse it at the global cap.
        if existing.is_none() && self.members.len() >= self.max_groups {
            tracing::warn!(
                max_groups = self.max_groups,
                "group creation refused: global cap reached"
            );
            return Ok(false);
        }
        let mut changes = self.leave_changes(joiner);
        let group = existing.unwrap_or_else(|| {
            changes.push(Change::Join(*owner, self.next_group_id));
            self.next_group_id
        });
        changes.push(Change::Join(*joiner, group));
        self.commit(changes, now_ms)?;
        Ok(true)
    }

    /// Takes `device` out of its group, if it has one.
    pub fn remove(&mut self, device: &DeviceId, now_ms: i64) -> rusqlite::Result<()> {
        let changes = self.leave_changes(device);
        self.commit(changes, now_ms)
    }

    /// Records that `device` was just active, so its group is not pruned.
    pub fn touch(&mut self, device: &DeviceId, now_ms: i64) {
        let Some(&group) = self.group_of.get(device) else {
            return;
        };
        if let Err(err) = self.store.touch(device, now_ms) {
            tracing::error!(%err, "could not record device activity");
            return;
        }
        let at = self.last_active.entry(group).or_insert(now_ms);
        *at = (*at).max(now_ms);
    }

    /// Group ids whose members have all been idle past `ttl` and are all
    /// currently disconnected, as judged by `is_connected`.
    pub fn inactive_unconnected_ids(
        &self,
        now_ms: i64,
        ttl: Duration,
        is_connected: impl Fn(&DeviceId) -> bool,
    ) -> Vec<GroupId> {
        let cutoff = now_ms.saturating_sub(duration_ms(ttl));
        self.last_active
            .iter()
            .filter(|&(_, &at)| at < cutoff)
            .map(|(&group, _)| group)
            .filter(|group| {
                self.members
                    .get(group)
                    .is_some_and(|members| members.iter().all(|device| !is_connected(device)))
            })
            .collect()
    }

    /// Removes a whole group and every envelope queued for its members.
    pub fn remove_group(&mut self, group: GroupId, now_ms: i64) -> rusqlite::Result<()> {
        let Some(members) = self.members.get(&group) else {
            return Ok(());
        };
        let changes = members.iter().copied().map(Change::Leave).collect();
        self.commit(changes, now_ms)
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
    fn commit(&mut self, changes: Vec<Change>, now_ms: i64) -> rusqlite::Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        self.store.apply(&changes, now_ms)?;
        for change in changes {
            match change {
                Change::Join(device_id, group_id) => {
                    if !self.members.contains_key(&group_id) {
                        tracing::info!(group = group_id, "group created");
                    }
                    self.insert(device_id, group_id, now_ms);
                    tracing::debug!(device = %short_id(&device_id), group = group_id, "device joined group");
                }
                Change::Leave(device_id) => self.leave(&device_id),
            }
        }
        Ok(())
    }

    fn insert(&mut self, device_id: DeviceId, group_id: GroupId, last_active: i64) {
        self.group_of.insert(device_id, group_id);
        self.members.entry(group_id).or_default().push(device_id);
        let at = self.last_active.entry(group_id).or_insert(last_active);
        *at = (*at).max(last_active);
        self.next_group_id = self.next_group_id.max(group_id + 1);
    }

    fn leave(&mut self, device_id: &DeviceId) {
        let Some(group_id) = self.group_of.remove(device_id) else {
            return;
        };
        tracing::debug!(device = %short_id(device_id), group = group_id, "device left group");
        let members = self.members.get_mut(&group_id).expect("group has members");
        members.retain(|member| member != device_id);
        if members.is_empty() {
            self.members.remove(&group_id);
            self.last_active.remove(&group_id);
            tracing::info!(group = group_id, "group deleted");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn groups(max_groups: usize) -> Groups {
        Groups::load(
            Store::open_at(Path::new(":memory:"), 0).unwrap(),
            max_groups,
        )
        .unwrap()
    }

    fn dev(n: u8) -> DeviceId {
        [n; 16]
    }

    #[test]
    fn refuses_a_new_group_past_the_global_cap() {
        let mut g = groups(1);
        assert!(g.add(&dev(1), &dev(2), 1_000).unwrap(), "first group forms");
        assert!(
            !g.add(&dev(3), &dev(4), 1_000).unwrap(),
            "a second group is refused at the cap"
        );
        assert_eq!(g.members.len(), 1);
        // Adding to the existing group creates no new group, so it is allowed.
        assert!(g.add(&dev(1), &dev(5), 1_000).unwrap());
        assert!(g.same_group(&dev(2), &dev(5)));
    }

    #[test]
    fn prunes_only_groups_idle_past_the_ttl_with_nobody_connected() {
        let mut g = groups(10);
        g.add(&dev(1), &dev(2), 1_000).unwrap();
        let ttl = Duration::from_millis(500);

        // Not yet stale: now - ttl is still before the last activity.
        assert!(g.inactive_unconnected_ids(1_200, ttl, |_| false).is_empty());

        // Stale and nobody connected: eligible.
        let stale = g.inactive_unconnected_ids(2_000, ttl, |_| false);
        assert_eq!(stale.len(), 1);

        // A connected member shields the group from pruning.
        assert!(
            g.inactive_unconnected_ids(2_000, ttl, |d| *d == dev(1))
                .is_empty()
        );

        // Touching a member refreshes the whole group.
        g.touch(&dev(2), 1_900);
        assert!(g.inactive_unconnected_ids(2_000, ttl, |_| false).is_empty());
    }

    #[test]
    fn remove_group_deletes_the_whole_group() {
        let mut g = groups(10);
        g.add(&dev(1), &dev(2), 1_000).unwrap();
        g.add(&dev(1), &dev(3), 1_000).unwrap();
        let group = g.inactive_unconnected_ids(10_000, Duration::from_millis(1), |_| false)[0];

        g.remove_group(group, 10_000).unwrap();

        assert_eq!(g.members.len(), 0);
        assert!(g.group_of.is_empty());
        assert!(g.last_active.is_empty());
    }
}
