//! Plugins panel — lists registered plugins, provides a Run button per plugin,
//! and a JSON params display. Foundation for future WASM/dylib plugin loading.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header};

use crate::app_state::{Action, App};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let names: Vec<String> = app.plugin_registry.names().into_iter().map(|s| s.to_string()).collect();
    let plugin_params = app.plugin_params.clone();

    let rows: Vec<_> = names.into_iter().enumerate().map(|(i, name)| {
        let name_s = name.clone();
        let params_s = plugin_params.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(colors::surface_border())
            .child(
                div()
                    .flex_1()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_primary())
                    .child(name_s.clone())
            )
            .child(
                div()
                    .id(SharedString::from(format!("plugin-run-{i}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(colors::tool_active())
                    .border_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(font_size::XS))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        let params = serde_json::from_str(&params_s)
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        root.app.apply(Action::RunPlugin {
                            name: name_s.clone(),
                            params,
                        });
                        cx.notify();
                    }))
                    .child("Run")
            )
    }).collect();

    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        .child(section_header("Plugins"))
        .child(divider())
        .children(rows)
        .child(
            div()
                .px_2()
                .py_1()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(format!("Params JSON: {}", app.plugin_params))
        )
}
