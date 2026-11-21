//! The global group cap, end to end: once the relay holds its maximum number
//! of groups, forming a new one is refused with `GroupFull`, while existing
//! groups keep working.

mod common;

use common::{Device, memory_config, pair, request_join, start_relay_with};
use weft_protocol::{ControlError, ControlFrame};

#[tokio::test]
async fn a_new_group_is_refused_past_the_global_cap() {
    let mut config = memory_config();
    config.max_groups = 1;
    let url = start_relay_with(config).await;

    // The one allowed group forms.
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    // A second, independent pair reaches the confirmation step, but the relay
    // refuses to create their group.
    let mut c = Device::connect(&url).await;
    let mut d = Device::connect(&url).await;
    request_join(&mut c, &mut d).await;
    c.send_control(ControlFrame::GroupAdd { device_id: d.id() })
        .await;
    c.expect_error(ControlError::GroupFull).await;

    // The first group is untouched and still delivers.
    a.assert_delivers(&mut b, "still paired").await;
}
