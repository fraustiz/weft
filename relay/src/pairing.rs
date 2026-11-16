//! Pairing slots and pending joins (`docs/protocol.md`, section 5). Both are
//! short-lived and kept in memory only.
//!
//! The relay never sees the pairing secret and does not check the request
//! MAC: only the slot owner can.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::DeviceId;

type PairingId = [u8; 16];

struct Slot {
    owner: DeviceId,
    expires_at: Instant,
}

pub struct Pairing {
    slot_ttl: Duration,
    pending_join_ttl: Duration,
    slots: HashMap<PairingId, Slot>,
    /// `(owner, joiner)` → expiry: `joiner` sent a pair request to a slot of
    /// `owner`, which may now add it to its group.
    pending_joins: HashMap<(DeviceId, DeviceId), Instant>,
}

impl Pairing {
    pub fn new(slot_ttl: Duration, pending_join_ttl: Duration) -> Pairing {
        Pairing {
            slot_ttl,
            pending_join_ttl,
            slots: HashMap::new(),
            pending_joins: HashMap::new(),
        }
    }

    /// Opens a slot for `owner`, replacing its previous one. Returns `false`
    /// if `pairing_id` is the open slot of another device.
    pub fn open(&mut self, owner: &DeviceId, pairing_id: PairingId, now: Instant) -> bool {
        // Opening is the only way these maps grow, so it is where expired
        // entries are swept. The owner's previous slot goes with them.
        self.slots
            .retain(|_, slot| slot.expires_at > now && slot.owner != *owner);
        self.pending_joins.retain(|_, expires_at| *expires_at > now);
        if self.slots.contains_key(&pairing_id) {
            return false;
        }
        let slot = Slot {
            owner: *owner,
            expires_at: now + self.slot_ttl,
        };
        self.slots.insert(pairing_id, slot);
        true
    }

    /// The owner of the open slot `pairing_id`, if there is one.
    pub fn slot_owner(&self, pairing_id: &PairingId, now: Instant) -> Option<DeviceId> {
        let slot = self.slots.get(pairing_id)?;
        (slot.expires_at > now).then_some(slot.owner)
    }

    /// Closes the slot `pairing_id` and lets its owner add `joiner` to its
    /// group for a while.
    pub fn close(&mut self, pairing_id: &PairingId, joiner: &DeviceId, now: Instant) {
        if let Some(slot) = self.slots.remove(pairing_id) {
            self.pending_joins
                .insert((slot.owner, *joiner), now + self.pending_join_ttl);
        }
    }

    pub fn has_pending_join(&self, owner: &DeviceId, joiner: &DeviceId, now: Instant) -> bool {
        self.pending_joins
            .get(&(*owner, *joiner))
            .is_some_and(|expires_at| *expires_at > now)
    }

    pub fn consume_pending_join(&mut self, owner: &DeviceId, joiner: &DeviceId) {
        self.pending_joins.remove(&(*owner, *joiner));
    }
}
