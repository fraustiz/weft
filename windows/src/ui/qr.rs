//! The pairing QR code, drawn with WinUI shapes: one ink rectangle per run
//! of dark modules in a row, on a canvas inside a cream card. Nothing of it
//! is ever written to disk, and it stays crisp at every display scale.
//!
//! The card is light in both themes, with the quiet zone the standard asks
//! for: cameras read dark modules on a light ground best.

use qrcode::{Color as Module, QrCode};
use windows_reactor::*;

use super::widgets::{CREAM, INK};
use crate::i18n::t;

/// Light modules around the code.
const QUIET_ZONE: usize = 4;
/// The side of a module, in effective pixels: a whole number of physical
/// pixels at the usual display scales.
const MODULE: f64 = 4.0;

/// A horizontal run of dark modules, in modules from the top left corner
/// of the code (quiet zone not included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    row: usize,
    column: usize,
    length: usize,
}

/// The side of the code in modules, and its dark runs row by row.
fn runs(code: &QrCode) -> (usize, Vec<Run>) {
    let width = code.width();
    let modules = code.to_colors();
    let mut runs = Vec::new();
    for (row, line) in modules.chunks(width).enumerate() {
        let mut column = 0;
        while column < width {
            if line[column] == Module::Light {
                column += 1;
                continue;
            }
            let length = line[column..]
                .iter()
                .take_while(|module| **module == Module::Dark)
                .count();
            runs.push(Run {
                row,
                column,
                length,
            });
            column += length;
        }
    }
    (width, runs)
}

/// The card showing `text` as a QR code, or `None` if it does not fit in
/// one (it always does for a pairing code).
pub fn view(text: &str) -> Option<View> {
    let code = QrCode::new(text.as_bytes()).ok()?;
    let (width, runs) = runs(&code);
    let side = width as f64 * MODULE;
    let modules = runs.into_iter().enumerate().map(|(index, run)| {
        KeyedView::new(
            index,
            Rectangle::new()
                .fill(INK)
                .width(run.length as f64 * MODULE)
                .height(MODULE)
                .canvas_left(run.column as f64 * MODULE)
                .canvas_top(run.row as f64 * MODULE),
        )
    });
    let canvas = Canvas::new()
        .width(side)
        .height(side)
        .keyed_children(modules);
    Some(
        Border::new()
            .background(CREAM)
            .corner_radius(8.0)
            .padding(Thickness::uniform(QUIET_ZONE as f64 * MODULE))
            .horizontal_alignment(HorizontalAlignment::Center)
            .automation_name(t("Pairing QR code", "QR code d'association"))
            .content(canvas),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_cover_exactly_the_dark_modules() {
        let code = QrCode::new(b"WEFT1:test").unwrap();
        let (width, runs) = runs(&code);
        assert_eq!(width, code.width());
        let mut drawn = vec![Module::Light; width * width];
        for run in &runs {
            assert!(run.length > 0 && run.column + run.length <= width);
            for column in run.column..run.column + run.length {
                assert_eq!(drawn[run.row * width + column], Module::Light, "overlap");
                drawn[run.row * width + column] = Module::Dark;
            }
        }
        assert!(drawn == code.to_colors());
        // Neighbouring dark modules share a rectangle.
        for pair in runs.windows(2) {
            assert!(pair[0].row != pair[1].row || pair[0].column + pair[0].length < pair[1].column);
        }
    }
}
