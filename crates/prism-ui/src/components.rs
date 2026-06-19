//! Shared UI components for the Prism suite.
//!
//! All components are pure functions returning `impl IntoElement`.
//! Colors use `.bg(rgba)` and `.text_color(rgba)` which both accept `Rgba`
//! via `From<Rgba> for Fill` and `From<Rgba> for Hsla` in gpui.

use gpui::{
    div, svg, px, IntoElement, ParentElement, Styled,
};

use crate::icons::Icon;
use crate::tokens::{colors, font_size, radius, spacing};

// ── icon / icon_colored ──────────────────────────────────────────────────────
// These use prism-ui's own Lucide SVG assets (not gpui-component's icon set),
// so they stay as gpui::svg() calls.

/// Render an SVG icon at the given size in pixels, tinted with the default
/// primary text color.
pub fn icon(icon: Icon, size_px: f32) -> impl IntoElement {
    svg()
        .path(icon.path())
        .w(px(size_px))
        .h(px(size_px))
        .text_color(colors::text_primary())
}

/// Icon with an explicit tint color.
pub fn icon_colored(icon: Icon, size_px: f32, color: gpui::Rgba) -> impl IntoElement {
    svg()
        .path(icon.path())
        .w(px(size_px))
        .h(px(size_px))
        .text_color(color)
}

// ── tool_button ──────────────────────────────────────────────────────────────
// Kept div-based: gpui-component Button has an uncertain API surface for our
// use case (custom icon child + active-state background from design tokens).
// Using the stable, tested div impl avoids build breakage across app crates.

/// A tool button — square, with a single icon centered.
/// `active` highlights the button with the accent background.
pub fn tool_button(icon: Icon, active: bool) -> impl IntoElement {
    let bg = if active {
        colors::tool_active()
    } else {
        colors::surface_raised()
    };
    let icon_color = if active {
        colors::text_primary()
    } else {
        colors::text_secondary()
    };
    div()
        .w(px(36.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MD))
        .bg(bg)
        .child(
            svg()
                .path(icon.path())
                .w(px(16.0))
                .h(px(16.0))
                .text_color(icon_color),
        )
}

// ── label ────────────────────────────────────────────────────────────────────
// Kept div-based: simple and stable; gpui-component Label adds no value here.

/// A small label — muted secondary text at MD size.
pub fn label(text: impl Into<String>) -> impl IntoElement {
    div()
        .text_size(px(font_size::MD))
        .text_color(colors::text_secondary())
        .child(text.into())
}

// ── section_header ───────────────────────────────────────────────────────────
// Kept div-based: no direct gpui-component equivalent.

/// A section header label — primary text at LG size.
pub fn section_header(text: impl Into<String>) -> impl IntoElement {
    div()
        .text_size(px(font_size::LG))
        .text_color(colors::text_primary())
        .px(px(spacing::MD))
        .py(px(spacing::SM))
        .child(text.into())
}

// ── divider ──────────────────────────────────────────────────────────────────
// Kept div-based: gpui-component Divider/Separator module path is uncertain
// (could be divider, separator, or hr). Hairline div is robust and correct.

/// A hairline divider in the border color.
pub fn divider() -> impl IntoElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(colors::surface_border())
}

// ── card ─────────────────────────────────────────────────────────────────────
// Kept div-based: no direct gpui-component equivalent for a generic card
// container with custom padding/rounding from design tokens.

/// A card / raised surface with rounded corners and padding.
pub fn card(children: impl IntoElement + 'static) -> impl IntoElement {
    div()
        .bg(colors::surface_raised())
        .rounded(px(radius::LG))
        .p(px(spacing::MD))
        .child(children)
}

// ── badge ────────────────────────────────────────────────────────────────────
// Kept div-based: gpui-component Badge API and availability is uncertain.
// The pill-badge with accent color and design-token sizing is simpler here.

/// An accent-colored pill badge with a short text label (e.g. "NEW", "BETA").
pub fn badge(text: impl Into<String>) -> impl IntoElement {
    div()
        .bg(colors::accent())
        .rounded(px(radius::PILL))
        .px(px(spacing::SM))
        .py(px(spacing::XS))
        .text_size(px(font_size::XS))
        .text_color(colors::text_primary())
        .child(text.into())
}
