//! Leaving the group, which the worker has no command for.
//!
//! It only uses public pieces of `weft-cli` and `weft-protocol`, while the
//! worker is stopped: a session of its own sends each member a Device
//! removed envelope naming this device (`docs/protocol.md`, section 5,
//! step 5), then `0xB3` for this device to the relay. Then the state
//! forgets every trusted device. Counters are kept, as a removal keeps
//! them.

use std::time::Duration;

use weft_cli::{Session, StateDir};
use weft_protocol::{
    ControlFrame, Header, Kind, VERSION, encode_device_removed, random_message_id, seal,
    session_keys,
};

use crate::CoreError;

/// How long telling the others may take before this device leaves anyway.
const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(10);

/// Tells the members and the relay that this device leaves.
async fn announce(dir: &StateDir) -> weft_cli::Result<()> {
    let mut session = Session::connect(dir.clone()).await?;
    let own = session.device_id();
    let members = session.state().devices.clone();
    let plaintext = encode_device_removed(&own);
    let mut frames = Vec::with_capacity(members.len());
    for member in &members {
        let member_id = member.device_id();
        let keys = session_keys(session.identity(), &member_id, &member.kx_pk)?;
        // Saved before the envelope leaves, as the session does.
        let (counter, _) = dir.update(|state| state.next_counter(&member_id))?;
        let header = Header {
            version: VERSION,
            kind: Kind::DeviceRemoved,
            sender: own,
            recipient: member_id,
            message_id: random_message_id(),
            counter,
        };
        frames.push(seal(&header, &plaintext, &keys.tx)?);
    }
    for frame in frames {
        session.send_frame(frame).await?;
    }
    session
        .send_frame(ControlFrame::GroupRemove { device_id: own }.encode())
        .await?;
    let settled = session.settle().await;
    session.close().await;
    settled
}

/// Leaves the group. Returns whether the members and the relay were told.
pub(crate) fn leave(dir: &StateDir) -> Result<bool, CoreError> {
    let state_error = |err: weft_cli::Error| CoreError::State {
        message: err.to_string(),
    };
    let members = dir.load().map_err(state_error)?.devices.len();
    let told = if members == 0 {
        true
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| CoreError::State {
                message: err.to_string(),
            })?;
        match runtime.block_on(tokio::time::timeout(ANNOUNCE_TIMEOUT, announce(dir))) {
            Ok(Ok(())) => true,
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "could not tell the group; leaving anyway");
                false
            }
            Err(_) => {
                tracing::warn!("telling the group timed out; leaving anyway");
                false
            }
        }
    };
    dir.update(|state| state.devices.clear())
        .map_err(state_error)?;
    tracing::info!(members, told, "left the group");
    Ok(told)
}
