//! What the relay does with each frame of an authenticated device: forward
//! an envelope, or act on a control frame.

use std::collections::VecDeque;
use std::time::Instant;

use axum::body::Bytes;
use tokio::sync::mpsc::error::TrySendError;
use weft_protocol::{
    ControlError, ControlFrame, DEVICE_ID_LEN, FrameType, Kind, MAX_DEVICE_NAME_LEN,
    MAX_WELCOME_DEVICES, PEER_KEYS_LEN, PairRequest, RouteHeader, classify, device_id_from_sign_pk,
};

use crate::{
    DeviceId, MIN_ENVELOPE_LEN, PAIR_OPEN_PER_MINUTE, PAIR_REQUEST_PER_MINUTE, RATE_WINDOW, Relay,
    Shared, now_ms, short_id,
};

/// Pairing quotas of one connection.
pub(crate) struct RateLimits {
    pair_open: RateLimit,
    pair_request: RateLimit,
}

impl RateLimits {
    pub(crate) fn new() -> RateLimits {
        RateLimits {
            pair_open: RateLimit::new(PAIR_OPEN_PER_MINUTE),
            pair_request: RateLimit::new(PAIR_REQUEST_PER_MINUTE),
        }
    }
}

/// Allows `max` events per sliding [`RATE_WINDOW`].
struct RateLimit {
    max: usize,
    allowed: VecDeque<Instant>,
}

impl RateLimit {
    fn new(max: usize) -> RateLimit {
        RateLimit {
            max,
            allowed: VecDeque::with_capacity(max),
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        while self
            .allowed
            .front()
            .is_some_and(|at| now.duration_since(*at) >= RATE_WINDOW)
        {
            self.allowed.pop_front();
        }
        if self.allowed.len() >= self.max {
            return false;
        }
        self.allowed.push_back(now);
        true
    }
}

impl Relay {
    /// Handles one frame from an authenticated device. Returns the error to
    /// answer with, if any. Frames that are neither an envelope nor a valid
    /// control frame are dropped.
    pub(crate) fn dispatch(
        &self,
        sender: &DeviceId,
        frame: Bytes,
        limits: &mut RateLimits,
    ) -> Option<ControlError> {
        let from = short_id(sender);
        match classify(&frame) {
            FrameType::Envelope => {
                self.forward(sender, frame);
                None
            }
            FrameType::Control => match ControlFrame::decode(&frame) {
                Ok(control) => self.control(sender, control, frame, limits),
                Err(err) => {
                    tracing::debug!(%from, %err, "dropped frame: invalid control frame");
                    None
                }
            },
            FrameType::Unknown => {
                tracing::debug!(%from, len = frame.len(), "dropped frame: unknown type");
                None
            }
        }
    }

    /// Routes one envelope on its version, length, sender and recipient;
    /// when the recipient is connected, the kind is not checked, so new kinds
    /// need no relay update. When the recipient is offline, only group-change
    /// and device-name envelopes (`0x03`–`0x06`) with valid lengths are queued.
    fn forward(&self, sender: &DeviceId, frame: Bytes) {
        let from = short_id(sender);
        let header = match RouteHeader::decode(&frame) {
            Ok(header) => header,
            Err(err) => {
                tracing::debug!(%from, %err, "dropped frame: invalid header");
                return;
            }
        };
        if header.sender != *sender {
            tracing::debug!(%from, "dropped frame: sender is not the authenticated device");
            return;
        }
        let to = short_id(&header.recipient);
        let outbox = {
            let mut state = self.state.lock().unwrap();
            if !state.groups.same_group(sender, &header.recipient) {
                tracing::debug!(%from, %to, "dropped frame: not in the same group");
                return;
            }
            match state.peers.get(&header.recipient) {
                Some(peer) => peer.outbox.clone(),
                None => {
                    self.queue_offline(&mut state, &header, &frame);
                    return;
                }
            }
        };
        match outbox.try_send(frame) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::debug!(%from, %to, "dropped frame: recipient queue full");
            }
            Err(TrySendError::Closed(frame)) => {
                let mut state = self.state.lock().unwrap();
                if state.groups.same_group(sender, &header.recipient)
                    && !state.peers.contains_key(&header.recipient)
                {
                    self.queue_offline(&mut state, &header, &frame);
                }
            }
        }
    }

    fn queue_offline(&self, state: &mut Shared, header: &RouteHeader, frame: &[u8]) {
        let from = short_id(&header.sender);
        let to = short_id(&header.recipient);
        if !is_queueable_offline(header.kind, frame.len()) {
            tracing::debug!(%from, %to, kind = header.kind, "dropped frame: recipient not connected");
            return;
        }
        if let Err(err) = state.groups.queue_offline(
            header,
            frame,
            now_ms(),
            self.offline_ttl,
            self.max_offline_per_device,
        ) {
            tracing::error!(%from, %to, %err, "offline envelope not saved, dropped");
        } else {
            tracing::debug!(%from, %to, kind = header.kind, "queued frame for offline recipient");
        }
    }

    /// Acts on a control frame; `frame` is its encoded form, as received.
    fn control(
        &self,
        sender: &DeviceId,
        control: ControlFrame,
        frame: Bytes,
        limits: &mut RateLimits,
    ) -> Option<ControlError> {
        let now = Instant::now();
        let result = match control {
            ControlFrame::PairOpen { pairing_id } => {
                if !limits.pair_open.allow(now) {
                    return Some(ControlError::RateLimited);
                }
                self.pair_open(sender, pairing_id, now)
            }
            // An invitation is handled exactly like a join request: the
            // relay cannot tell them apart (the MAC binds the intent) and
            // does not try to.
            ControlFrame::PairRequest(request) | ControlFrame::PairInvite(request) => {
                if !limits.pair_request.allow(now) {
                    return Some(ControlError::RateLimited);
                }
                self.pair_request(sender, &request, frame, now)
            }
            ControlFrame::GroupAdd { device_id } => self.group_add(sender, &device_id, now),
            ControlFrame::GroupJoin { device_id } => self.group_join(sender, &device_id, now),
            ControlFrame::GroupRemove { device_id } => self.group_remove(sender, &device_id),
            ControlFrame::Error(_) => {
                tracing::debug!(from = %short_id(sender), "dropped frame: error frame from a device");
                Ok(())
            }
        };
        result.err()
    }

    fn pair_open(
        &self,
        owner: &DeviceId,
        pairing_id: [u8; 16],
        now: Instant,
    ) -> Result<(), ControlError> {
        let mut state = self.state.lock().unwrap();
        if !state.pairing.open(owner, pairing_id, now) {
            // The id is the open slot of another device: leave that slot alone.
            return Err(ControlError::NotAllowed);
        }
        tracing::debug!(device = %short_id(owner), "pairing slot opened");
        Ok(())
    }

    /// Forwards the request, unchanged, to the owner of the slot it names.
    /// The relay cannot check the MAC: the slot owner does.
    fn pair_request(
        &self,
        joiner: &DeviceId,
        request: &PairRequest,
        frame: Bytes,
        now: Instant,
    ) -> Result<(), ControlError> {
        if device_id_from_sign_pk(&request.sign_pk) != *joiner {
            return Err(ControlError::NotAllowed);
        }
        let mut state = self.state.lock().unwrap();
        let owner = state
            .pairing
            .slot_owner(&request.pairing_id, now)
            .ok_or(ControlError::UnknownPairing)?;
        if owner == *joiner {
            return Err(ControlError::NotAllowed);
        }
        // If the owner cannot be reached the slot stays open, so the request
        // can be sent again once the owner is back.
        let delivered = state
            .peers
            .get(&owner)
            .is_some_and(|peer| peer.outbox.try_send(frame).is_ok());
        if !delivered {
            return Err(ControlError::UnknownPairing);
        }
        state.pairing.close(&request.pairing_id, joiner, now);
        tracing::debug!(owner = %short_id(&owner), joiner = %short_id(joiner), "pair request forwarded");
        Ok(())
    }

    fn group_add(
        &self,
        owner: &DeviceId,
        joiner: &DeviceId,
        now: Instant,
    ) -> Result<(), ControlError> {
        let mut state = self.state.lock().unwrap();
        if !state.pairing.has_pending_join(owner, joiner, now) {
            return Err(ControlError::NotAllowed);
        }
        match state.groups.add(owner, joiner, now_ms()) {
            Ok(true) => state.pairing.consume_pending_join(owner, joiner),
            // The pending join is kept: the owner may make room and retry.
            Ok(false) => return Err(ControlError::GroupFull),
            Err(err) => tracing::error!(%err, "group change not saved, ignored"),
        }
        Ok(())
    }

    /// The slot owner accepts the invitation of `inviter`: it moves into the
    /// group of `inviter` (created if `inviter` has none), leaving its own.
    fn group_join(
        &self,
        owner: &DeviceId,
        inviter: &DeviceId,
        now: Instant,
    ) -> Result<(), ControlError> {
        let mut state = self.state.lock().unwrap();
        if !state.pairing.has_pending_join(owner, inviter, now) {
            return Err(ControlError::NotAllowed);
        }
        match state.groups.add(inviter, owner, now_ms()) {
            Ok(true) => state.pairing.consume_pending_join(owner, inviter),
            // The pending join is kept, as for a group add.
            Ok(false) => return Err(ControlError::GroupFull),
            Err(err) => tracing::error!(%err, "group change not saved, ignored"),
        }
        Ok(())
    }

    fn group_remove(&self, sender: &DeviceId, target: &DeviceId) -> Result<(), ControlError> {
        let mut state = self.state.lock().unwrap();
        if target != sender && !state.groups.same_group(sender, target) {
            return Err(ControlError::NotAllowed);
        }
        if let Err(err) = state.groups.remove(target, now_ms()) {
            tracing::error!(%err, "group change not saved, ignored");
        }
        Ok(())
    }
}

fn is_queueable_offline(kind: u8, frame_len: usize) -> bool {
    const WELCOME: u8 = Kind::Welcome as u8;
    const DEVICE_ADDED: u8 = Kind::DeviceAdded as u8;
    const DEVICE_REMOVED: u8 = Kind::DeviceRemoved as u8;
    const DEVICE_NAME: u8 = Kind::DeviceName as u8;

    match kind {
        WELCOME => {
            frame_len > MIN_ENVELOPE_LEN
                && (frame_len - MIN_ENVELOPE_LEN) % PEER_KEYS_LEN == 1
                && frame_len <= MIN_ENVELOPE_LEN + 1 + MAX_WELCOME_DEVICES * PEER_KEYS_LEN
        }
        DEVICE_ADDED => frame_len == MIN_ENVELOPE_LEN + PEER_KEYS_LEN,
        DEVICE_REMOVED => frame_len == MIN_ENVELOPE_LEN + DEVICE_ID_LEN,
        DEVICE_NAME => {
            (MIN_ENVELOPE_LEN + 1..=MIN_ENVELOPE_LEN + MAX_DEVICE_NAME_LEN).contains(&frame_len)
        }
        _ => false,
    }
}
