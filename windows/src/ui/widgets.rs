//! Small pieces the pages share. Everything is system chrome except the
//! status mark and the primary action, which carry the brand orange.

use windows_reactor::*;

/// The brand colours.
pub const ORANGE: Color = Color::rgb(0xF2, 0x6A, 0x21);
pub const INK: Color = Color::rgb(0x17, 0x13, 0x0F);
pub const CREAM: Color = Color::rgb(0xF6, 0xEF, 0xE6);

/// Secondary text: the primary text colour at the opacity Fluent uses for
/// its secondary text.
const SECONDARY: f64 = 0.7;

/// Keys views by position, for lists built in a fixed order.
pub fn keyed(views: Vec<View>) -> Vec<KeyedView> {
    views
        .into_iter()
        .enumerate()
        .map(|(index, view)| KeyedView::new(index, view))
        .collect()
}

pub fn body(text: impl Into<String>) -> TextBlock {
    TextBlock::new()
        .text(text)
        .text_wrapping(TextWrapping::Wrap)
}

pub fn secondary(text: impl Into<String>) -> TextBlock {
    body(text).opacity(SECONDARY)
}

/// A group label, in the "body strong" style of Fluent.
pub fn label(text: impl Into<String>) -> TextBlock {
    body(text)
        .font_weight(FontWeight::SEMI_BOLD)
        .automation_heading_level(AutomationHeadingLevel::Level2)
}

/// The title of a view that replaces a page, in the "subtitle" style.
pub fn title(text: impl Into<String>) -> TextBlock {
    body(text)
        .font_size(20.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .automation_heading_level(AutomationHeadingLevel::Level1)
}

/// The brand pixel, the rounded square of the mark (corner radius a
/// quarter of the side): orange while the sync is live, a neutral grey
/// otherwise. The words next to it say the same.
pub fn status_mark(live: bool) -> View {
    let mark = Border::new()
        .width(10.0)
        .height(10.0)
        .corner_radius(2.5)
        .vertical_alignment(VerticalAlignment::Center);
    if live {
        mark.background(ORANGE).into()
    } else {
        mark.background(ThemeBrush::PrimaryText).opacity(0.4).into()
    }
}

/// A raised surface for one item of a list.
pub fn card(content: impl Into<View>) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(8.0)
        .padding(Thickness::new(16.0, 12.0, 16.0, 12.0))
        .content(content)
}

/// The one primary action of a view: orange, with ink text since white on
/// this orange is too faint to read.
pub fn primary_button(label: &str, on_click: Callback<()>) -> View {
    let hover = Color::argb(0xE6, 0xF2, 0x6A, 0x21);
    let pressed = Color::argb(0xCC, 0xF2, 0x6A, 0x21);
    let colours = ResourceOverrides::new()
        .set("AccentButtonBackground", ORANGE)
        .set("AccentButtonBackgroundPointerOver", hover)
        .set("AccentButtonBackgroundPressed", pressed)
        .set("AccentButtonBorderBrush", ORANGE)
        .set("AccentButtonBorderBrushPointerOver", hover)
        .set("AccentButtonBorderBrushPressed", pressed)
        .set("AccentButtonForeground", INK)
        .set("AccentButtonForegroundPointerOver", INK)
        .set("AccentButtonForegroundPressed", INK);
    Button::new()
        .style(ButtonStyle::Accent)
        .resource_overrides(colours)
        .on_click(on_click)
        .content(label)
}

pub fn button(label: &str, on_click: Callback<()>) -> View {
    Button::new().on_click(on_click).content(label)
}

/// Views side by side, with the gap between buttons.
pub fn row(views: Vec<View>) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .keyed_children(keyed(views))
}

/// Views one above the other, `gap` apart.
pub fn column(gap: f64, views: Vec<View>) -> View {
    StackPanel::new().spacing(gap).keyed_children(keyed(views))
}

/// A spinner and what is being waited for.
pub fn waiting(text: &str) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .children((
            ProgressRing::new()
                .is_indeterminate(true)
                .width(20.0)
                .height(20.0),
            body(text).vertical_alignment(VerticalAlignment::Center),
        ))
}
