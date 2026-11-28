use super::*;
use crate::demo::format_menu::{icon_button, toolbar, toolbar_button};
use gpdoc::{EditorConfig, EditorSurface, Node};
use gpui::{App, Div, KeyDownEvent, SharedString, Stateful, rgb};
use serde_json::Map;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Action {
    id: String,
    command: String,
    label: String,
    mark: Option<String>,
    #[serde(default)]
    args: Map<String, Value>,
    fields: Vec<Field>,
    fields_from: Option<String>,
}

#[derive(Deserialize)]
struct Field {
    name: String,
    label: String,
    #[serde(flatten)]
    kind: FieldKind,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum FieldKind {
    Text {
        value: String,
        #[serde(default)]
        optional: bool,
    },
    Textarea {
        value: String,
    },
    Color {
        value: String,
    },
    Number {
        value: i64,
        min: i64,
        max: i64,
    },
    Select {
        choices: Vec<(String, String)>,
        #[serde(default)]
        boolean: bool,
    },
}

enum Input {
    Text(Entity<EditorView>),
    Choice(usize),
}

impl Field {
    fn open(&self, config: &EditorConfig, cx: &mut App) -> Input {
        let value = match &self.kind {
            FieldKind::Select { .. } => return Input::Choice(0),
            FieldKind::Number { value, .. } => value.to_string(),
            FieldKind::Text { value, .. }
            | FieldKind::Textarea { value }
            | FieldKind::Color { value } => value.clone(),
        };
        let lines = value
            .split('\n')
            .map(|line| Node::text("paragraph", line))
            .collect();
        Input::Text(
            config
                .open(DocumentSource::Blocks(lines), cx)
                .expect("open toolbar input"),
        )
    }

    fn value(&self, input: &Input, cx: &App) -> Result<Value> {
        let value = match input {
            Input::Choice(index) => {
                let FieldKind::Select { choices, boolean } = &self.kind else {
                    unreachable!()
                };
                let value = &choices[*index].0;
                return Ok(if *boolean {
                    Value::Bool(value.parse()?)
                } else {
                    value.clone().into()
                });
            }
            Input::Text(editor) => editor
                .read(cx)
                .editor()
                .document()
                .blocks()
                .iter()
                .map(Node::text_content)
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let limit = match (&self.kind, self.name.as_str()) {
            (FieldKind::Textarea { .. }, _) => 4000,
            (_, "href" | "src") => 500,
            _ => 120,
        };
        if value.encode_utf16().count() > limit {
            return Err(format!("{} accepts up to {limit} characters.", self.label).into());
        }
        if value.is_empty() && !matches!(self.kind, FieldKind::Text { optional: true, .. }) {
            return Err(format!("Enter {}.", self.label).into());
        }
        if !matches!(self.kind, FieldKind::Textarea { .. }) && value.contains('\n') {
            return Err(format!("Enter {} on one line.", self.label).into());
        }
        match self.kind {
            FieldKind::Number { min, max, .. } => value
                .parse::<i64>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .map(Value::from)
                .ok_or_else(|| {
                    format!("{} must be an integer from {min} to {max}.", self.label).into()
                }),
            FieldKind::Color { .. }
                if !(value.len() == 7
                    && value.starts_with('#')
                    && value[1..].bytes().all(|b| b.is_ascii_hexdigit())) =>
            {
                Err("Enter a color as #RRGGBB.".into())
            }
            _ => Ok(value.into()),
        }
    }
}

pub(super) struct Toolbar {
    pub(super) actions: Vec<Action>,
    inputs: Vec<Vec<Input>>,
}

impl Toolbar {
    pub(super) fn new(actions: Vec<Action>, cx: &mut App) -> Self {
        let config = EditorConfig::builder()
            .plugin(gpdoc_plugin_paragraph::paragraph())
            .build()
            .expect("configure toolbar inputs");
        let inputs = actions
            .iter()
            .map(|action| {
                action
                    .fields
                    .iter()
                    .map(|field| field.open(&config, cx))
                    .collect()
            })
            .collect();
        Self { actions, inputs }
    }

    fn command(&self, index: usize, cx: &App) -> Result<actions::DemoAction> {
        let action = &self.actions[index];
        let group = match &action.fields_from {
            Some(id) => self
                .actions
                .iter()
                .position(|a| a.id == *id)
                .ok_or("Unknown field group")?,
            None => index,
        };
        let mut args = action.args.clone();
        for (field, input) in self.actions[group].fields.iter().zip(&self.inputs[group]) {
            args.insert(field.name.clone(), field.value(input, cx)?);
        }
        args.insert("action".into(), action.command.clone().into());
        Ok(serde_json::from_value(Value::Object(args))?)
    }
}

fn button(id: impl Into<gpui::ElementId>, label: &str, selected: bool) -> Stateful<Div> {
    toolbar_button(id, selected)
        .focusable()
        .tab_stop(true)
        .role(gpui::Role::Button)
        .aria_label(label.to_owned())
        .focus_visible(|style| style.border_1().border_color(rgb(0x236eae)))
        .px(px(8.))
        .h(px(28.))
        .child(label.to_owned())
}

impl PluginSurface {
    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let outcome = self.toolbar.command(index, cx).and_then(|command| {
            self.view.update(cx, |view, cx| {
                ActionContext {
                    view,
                    window,
                    cx,
                    facts: &mut self.facts,
                }
                .perform(command)?;
                view.focus_in(window, cx);
                Ok(())
            })
        });
        self.error = outcome.err().map(|error| error.to_string());
        cx.notify();
    }

    fn render_input(&self, group: usize, index: usize, cx: &mut Context<Self>) -> Div {
        let field = &self.toolbar.actions[group].fields[index];
        let mut control = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(field.label.clone());
        match &self.toolbar.inputs[group][index] {
            Input::Choice(selected) => {
                let FieldKind::Select { choices, .. } = &field.kind else {
                    unreachable!()
                };
                control.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .children(choices.iter().enumerate().map(|(n, (_, label))| {
                            button(
                                SharedString::from(format!(
                                    "{}-{}-{n}",
                                    self.toolbar.actions[group].id, field.name
                                )),
                                label,
                                *selected == n,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.toolbar.inputs[group][index] = Input::Choice(n);
                                    cx.notify();
                                },
                            ))
                        })),
                )
            }
            Input::Text(editor) => {
                let multiline = matches!(field.kind, FieldKind::Textarea { .. });
                control = if multiline {
                    control.w_full()
                } else {
                    control.w(px(160.)).max_w_full()
                };
                let input_surface = EditorSurface::new(editor);
                let editor = editor.clone();
                control.child(
                    div()
                        .w_full()
                        .h(px(if multiline { 82. } else { 30. }))
                        .border_1()
                        .rounded_sm()
                        .border_color(rgb(0xc7c9cc))
                        .bg(gpui::white())
                        .when(!multiline, |el| {
                            el.capture_key_down(cx.listener(
                                move |this, event: &KeyDownEvent, window, cx| {
                                    if event.keystroke.key == "enter"
                                        && !editor.read(cx).is_composing()
                                    {
                                        window.prevent_default();
                                        cx.stop_propagation();
                                        this.activate(group, window, cx);
                                    }
                                },
                            ))
                        })
                        .child(
                            input_surface
                                .text_size(px(14.))
                                .line_height(px(21.))
                                .layout(|doc| div().px(px(6.)).py(px(3.)).child(doc)),
                        ),
                )
            }
        }
    }

    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut fields = Vec::new();
        let mut buttons = Vec::new();
        for (i, action) in self.toolbar.actions.iter().enumerate() {
            for j in 0..action.fields.len() {
                fields.push(self.render_input(i, j, cx));
            }
            let active = action.mark.as_ref().is_some_and(|mark| {
                self.view
                    .read(cx)
                    .editor()
                    .mark_active(mark)
                    .unwrap_or(false)
            });
            buttons.push(
                icon_button(
                    SharedString::from(action.id.clone()),
                    action.label.clone().into(),
                    gpui::svg()
                        .data(icons::icon(&action.id).svg_bytes())
                        .size(px(17.)),
                    active,
                )
                .focusable()
                .tab_stop(true)
                .role(gpui::Role::Button)
                .aria_label(action.label.clone())
                .when(action.mark.is_some(), |button| {
                    button.aria_toggled(active.into())
                })
                .focus_visible(|style| style.border_1().border_color(rgb(0x236eae)))
                .on_click(cx.listener(move |this, _, window, cx| this.activate(i, window, cx))),
            );
        }
        div()
            .id("toolbar")
            .role(gpui::Role::Toolbar)
            .aria_label("Editor toolbar")
            .max_h(px(188.))
            .overflow_y_scroll()
            .flex_none()
            .p(px(6.))
            .bg(rgb(0xf8f8f7))
            .border_b_1()
            .border_color(rgb(0xdededa))
            .when(!fields.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .pb(px(5.))
                        .children(fields),
                )
            })
            .child(toolbar().flex_wrap().gap(px(2.)).children(buttons))
    }
}
