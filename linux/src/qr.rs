//! Terminal QR code rendering using UTF-8 half-block characters (`▀`, `▄`, `█`).

use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

/// Renders `payload` as a compact UTF-8 half-block QR code (`▀`, `▄`, `█`, `' '`)
/// suitable for dark and light terminal emulators.
///
/// Returns `None` if `payload` is empty or exceeds QR capacity.
pub fn render_terminal_qr(payload: &str) -> Option<String> {
    if payload.is_empty() {
        return None;
    }
    let qr = QrCode::new(payload.as_bytes()).ok()?;
    Some(
        qr.render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use weft_protocol::PairingCode;

    use super::*;

    #[test]
    fn renders_pairing_code_in_utf8_half_blocks() {
        let code = PairingCode {
            relay_url: "wss://weft.frohub.eu/v1".into(),
            pairing_id: [0x11; 16],
            sign_pk: [0x22; 32],
            kx_pk: [0x33; 32],
            secret: [0x44; 32],
        };
        let encoded = code.encode().unwrap();
        let rendered = render_terminal_qr(&encoded).expect("valid QR");

        assert!(rendered.contains('█'));
        assert!(rendered.contains('▀'));
        assert!(rendered.contains('▄'));

        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines.len() >= 15, "unexpected QR height: {}", lines.len());
        let width = lines[0].chars().count();
        assert!(width >= 29, "unexpected QR width: {width}");
        for line in &lines {
            assert_eq!(line.chars().count(), width, "uneven QR row width");
            assert!(
                line.chars().all(|c| matches!(c, '█' | '▀' | '▄' | ' ')),
                "unexpected char in QR row"
            );
        }
    }

    #[test]
    fn rejects_empty_or_oversized_qr_payloads() {
        assert!(render_terminal_qr("").is_none());
        assert!(render_terminal_qr(&"x".repeat(8192)).is_none());
    }
}
