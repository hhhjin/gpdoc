mod actions;
mod behavior;
mod setup;

use gpdoc::{CommandOutcome, DocPos, Edit, Editor, Error, Selection, Step, Value};
use serde::Serialize;
use std::{collections::BTreeMap, ops::Range};
use wasm_bindgen::prelude::*;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default, Serialize)]
struct Preview {
    document: Value,
    selection: [usize; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    peer: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_selection: Option<[usize; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alice_caret: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bob_caret: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    facts: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<SearchPreview>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    presentation: BTreeMap<usize, Presentation>,
}

#[derive(Serialize)]
struct SearchPreview {
    ranges: Vec<[usize; 2]>,
    current: Option<usize>,
}

#[derive(Default, Serialize)]
struct Presentation {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tokens: Vec<Token>,
}

#[derive(Serialize)]
struct Token {
    text: String,
    color: [u8; 4],
}

impl Preview {
    fn read(editor: &Editor) -> Result<Self> {
        let mut preview = Self {
            document: json(editor)?,
            selection: match editor.selection() {
                Selection::Text { anchor, head } => [anchor.0, head.0],
                other => {
                    let range = other.bounds(editor.document());
                    [range.start.0, range.end.0]
                }
            },
            ..Self::default()
        };
        for (pos, node) in editor.document().iter_nodes() {
            match node.kind.as_str() {
                "codeBlock" => {
                    let text = node.text_content();
                    let tokens: Vec<_> = editor
                        .config()
                        .presentation()
                        .syntax_tokens(node, &text)
                        .into_iter()
                        .filter_map(|token| {
                            Some(Token {
                                text: text.get(token.bytes)?.into(),
                                color: token.color,
                            })
                        })
                        .collect();
                    if !tokens.is_empty() {
                        preview.presentation.insert(
                            pos.0,
                            Presentation {
                                tokens,
                                ..Presentation::default()
                            },
                        );
                    }
                }
                "mention" => {
                    preview.presentation.insert(
                        pos.0,
                        Presentation {
                            label: editor.config().atom_label(node),
                            ..Presentation::default()
                        },
                    );
                }
                _ => {}
            }
        }
        Ok(preview)
    }

    fn fact(&mut self, label: impl Into<String>, value: impl ToString) {
        self.facts.push((label.into(), value.to_string()));
    }
}

fn json(editor: &Editor) -> Result<Value> {
    Ok(serde_json::from_str(&editor.document().to_json()?)?)
}

fn select(editor: &mut Editor, anchor: usize, head: usize) -> std::result::Result<(), Error> {
    let selection = Selection::Text {
        anchor: DocPos(anchor),
        head: DocPos(head),
    };
    // Calling select on an unchanged caret clears pending typing marks.
    if selection != editor.selection() {
        editor.select(selection)?;
    }
    Ok(())
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum InputKind {
    Text = "text",
    Paste = "paste",
    ParagraphBreak = "paragraphBreak",
    LineBreak = "lineBreak",
}

impl InputKind {
    fn apply(self, editor: &mut Editor, text: &str) -> std::result::Result<(), Error> {
        match self {
            Self::Text => {
                let mut input = editor.begin_text_input();
                input.replace(editor.selection(), &text.split('\n').collect::<Vec<_>>())?;
                editor.commit_text_input(input)?;
            }
            Self::Paste => {
                editor.paste_text(text)?;
            }
            Self::ParagraphBreak => {
                editor.paragraph_break()?;
            }
            Self::LineBreak => {
                if editor
                    .config()
                    .installed_plugins()
                    .any(|p| p.name() == "hardBreak")
                    && matches!(
                        editor.run(gpdoc_plugin_hard_break::Set)?,
                        CommandOutcome::Applied(_)
                    )
                {
                    return Ok(());
                }
                editor.insert_text("\n")?;
            }
            _ => return Err(Error::InvalidEdit("Unknown input kind".into())),
        }
        Ok(())
    }
}

fn js(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}

#[wasm_bindgen]
pub struct PluginEditor {
    slug: String,
    editor: Editor,
    peers: Option<behavior::Peers>,
    facts: Vec<(String, String)>,
}

#[wasm_bindgen]
impl PluginEditor {
    #[wasm_bindgen(constructor)]
    pub fn new(
        slug: &str,
        source: &str,
        from: usize,
        to: usize,
    ) -> std::result::Result<PluginEditor, JsError> {
        let mut editor = setup::open(slug, source).map_err(js)?;
        if from > 0 && to > 0 {
            select(&mut editor, from, to).map_err(js)?;
        }
        let peers = if matches!(slug, "collaboration" | "collaboration-caret") {
            Some(behavior::Peers::new(&editor).map_err(js)?)
        } else {
            None
        };
        Ok(Self {
            slug: slug.into(),
            editor,
            peers,
            facts: vec![],
        })
    }

    pub fn select(&mut self, anchor: usize, head: usize) -> std::result::Result<(), JsError> {
        select(&mut self.editor, anchor, head).map_err(js)
    }

    pub fn input(&mut self, text: &str, kind: InputKind) -> std::result::Result<(), JsError> {
        let revision = self.editor.revision();
        self.edit(|editor| kind.apply(editor, text)).map_err(js)?;
        if self.slug == "character-count" && revision == self.editor.revision() && !text.is_empty()
        {
            return Err(js("The edit exceeds the 24-unit character limit."));
        }
        Ok(())
    }

    pub fn publish_caret(
        &mut self,
        from_bob: bool,
        anchor: usize,
        head: usize,
        focused: bool,
    ) -> std::result::Result<(), JsError> {
        if let Some(peers) = &mut self.peers {
            let selection = Selection::Text {
                anchor: DocPos(anchor),
                head: DocPos(head),
            };
            peers.publish(from_bob, selection, focused).map_err(js)?;
        }
        Ok(())
    }

    pub fn peer_history(&mut self, redo: bool) -> std::result::Result<(), JsError> {
        self.peer_history_inner(redo).map_err(js)
    }

    pub fn peer_input(
        &mut self,
        anchor: usize,
        head: usize,
        text: &str,
        kind: InputKind,
    ) -> std::result::Result<(), JsError> {
        self.peer_input_inner(anchor, head, text, kind).map_err(js)
    }

    pub fn shortcut(
        &mut self,
        key: &str,
        primary: bool,
        shift: bool,
        alt: bool,
    ) -> std::result::Result<bool, JsError> {
        let mut key = gpdoc::Shortcut::new(key);
        key.primary = primary;
        key.shift = shift;
        key.alt = alt;
        let config = self.editor.config().clone();
        for command in config.shortcuts(&key) {
            if matches!(
                self.edit(|editor| editor.run_stored(command)).map_err(js)?,
                CommandOutcome::Applied(_)
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn action(&mut self, input: &str) -> std::result::Result<(), JsError> {
        self.perform(serde_json::from_str(input).map_err(js)?)
            .map_err(js)
    }

    pub fn snapshot(&self) -> std::result::Result<String, JsError> {
        let mut preview = Preview::read(&self.editor).map_err(js)?;
        behavior::decorate(self, &mut preview).map_err(js)?;
        preview.facts.extend(self.facts.iter().cloned());
        serde_json::to_string(&preview).map_err(js)
    }

    pub fn history(&mut self, redo: bool) -> std::result::Result<(), JsError> {
        if let Some(peers) = &mut self.peers {
            peers.history(&mut self.editor, redo).map_err(js)
        } else {
            let result = if redo {
                self.editor.redo()
            } else {
                self.editor.undo()
            };
            match result {
                Ok(_) | Err(Error::NoHistory) => Ok(()),
                Err(error) => Err(js(error)),
            }
        }
    }
}

impl PluginEditor {
    fn edit<T>(
        &mut self,
        op: impl FnOnce(&mut Editor) -> std::result::Result<T, Error>,
    ) -> Result<T> {
        if let Some(peers) = &mut self.peers {
            let result = op(&mut self.editor)?;
            peers.record(&self.editor)?;
            Ok(result)
        } else {
            Ok(op(&mut self.editor)?)
        }
    }

    fn attribute(&mut self, pos: usize, attr: &str, value: Value) -> Result<()> {
        let mut tx = self.editor.transaction();
        tx.push(Step::set_node_attribute(DocPos(pos), attr, value));
        self.editor.apply(tx)?;
        Ok(())
    }

    fn duplicate(&mut self) -> Result<()> {
        let (range, node) = self.current_block()?;
        let mut tx = self.editor.transaction();
        tx.push(Edit::ReplaceNodes {
            range: range.end..range.end,
            nodes: vec![node.clone()],
        });
        self.editor.apply(tx)?;
        Ok(())
    }

    fn current_block(&self) -> Result<(Range<DocPos>, &gpdoc::Node)> {
        let start = self.editor.selection().bounds(self.editor.document()).start;
        let mut pos = 0;
        for node in self.editor.document().blocks() {
            let end = pos + node.size();
            if start.0 < end {
                return Ok((DocPos(pos)..DocPos(end), node));
            }
            pos = end;
        }
        Err("Place the caret in a block first.".into())
    }
}
