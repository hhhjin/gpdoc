use super::*;
use gpdoc_plugin_collaboration::{Collaboration, StateVector};
use gpdoc_plugin_collaboration_caret::{CollaborationCaret, User};

pub(super) struct Peers {
    alice: Collaboration,
    bob: Collaboration,
    alice_caret: CollaborationCaret,
    bob_caret: CollaborationCaret,
}

impl Peers {
    pub fn new(editor: &Editor) -> Result<Self> {
        let alice = Collaboration::new(editor.fork(), "default")?;
        let bob = Collaboration::from_update(
            editor.config(),
            "default",
            &alice.update_since(&StateVector::default()),
        )?;
        let alice_caret = CollaborationCaret::new(&alice);
        let bob_caret = CollaborationCaret::new(&bob);
        Ok(Self {
            alice,
            bob,
            alice_caret,
            bob_caret,
        })
    }

    pub fn record(&mut self, editor: &Editor) -> Result<()> {
        if let Some(update) = self.alice.record(editor)? {
            self.bob.apply_update(&update)?;
        }
        Ok(())
    }

    pub fn history(&mut self, editor: &mut Editor, redo: bool) -> Result<()> {
        let update = if redo {
            self.alice.redo()
        } else {
            self.alice.undo()
        };
        match update {
            Ok(update) => {
                self.bob.apply_update(&update)?;
                *editor = self.alice.editor().fork();
            }
            Err(gpdoc_plugin_collaboration::Error::Editor(Error::NoHistory)) => {}
            Err(error) => return Err(error.into()),
        };
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        from_bob: bool,
        selection: Selection,
        focused: bool,
    ) -> Result<()> {
        let (session, sender, receiver, name) = if from_bob {
            (
                &mut self.bob,
                &mut self.bob_caret,
                &mut self.alice_caret,
                "Bob",
            )
        } else {
            (
                &mut self.alice,
                &mut self.alice_caret,
                &mut self.bob_caret,
                "Alice",
            )
        };
        if focused {
            session.select(selection)?;
        }
        let user = User::from_iter([("name".into(), name.into())]);
        receiver.receive(&sender.publish(session, user, focused)?)?;
        Ok(())
    }
}

impl PluginEditor {
    pub(super) fn peer_history_inner(&mut self, redo: bool) -> Result<()> {
        let peers = self.peers.as_mut().ok_or("No second editor")?;
        let peer = &mut peers.bob;
        let result = if redo { peer.redo() } else { peer.undo() };
        match result {
            Ok(update) => {
                peers.alice.apply_update(&update)?;
                self.editor = peers.alice.editor().fork();
                Ok(())
            }
            Err(gpdoc_plugin_collaboration::Error::Editor(Error::NoHistory)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn peer_input_inner(
        &mut self,
        anchor: usize,
        head: usize,
        text: &str,
        kind: InputKind,
    ) -> Result<()> {
        let peers = self.peers.as_mut().ok_or("No second editor")?;
        let peer = &mut peers.bob;
        peer.select(Selection::Text {
            anchor: DocPos(anchor),
            head: DocPos(head),
        })?;
        let (_, update) = peer.edit(|editor| kind.apply(editor, text))?;
        peers.alice.apply_update(&update)?;
        self.editor = peers.alice.editor().fork();
        Ok(())
    }
}

pub(super) fn decorate(demo: &PluginEditor, preview: &mut Preview) -> Result<()> {
    let document = demo.editor.document();
    match demo.slug.as_str() {
        "character-count" => {
            use gpdoc_plugin_character_count::{CountMode, characters, words};
            preview.fact(
                "UTF-16 units",
                format!(
                    "{} / {}",
                    characters(document, CountMode::TextSize),
                    setup::CHARACTER_LIMIT
                ),
            );
            preview.fact("Words", words(document));
        }
        "find-and-replace" => {
            let search = gpdoc_plugin_find_and_replace::results(&demo.editor);
            preview.fact("Matches", search.ranges.len());
            preview.search = Some(SearchPreview {
                ranges: search
                    .ranges
                    .into_iter()
                    .map(|r| [r.start.0, r.end.0])
                    .collect(),
                current: search.current,
            });
        }
        "table-of-contents" => {
            for heading in gpdoc_plugin_table_of_contents::headings(document) {
                preview.fact(
                    heading.text,
                    format!("H{} · #{}", heading.original_level, heading.id),
                );
            }
        }
        "unique-id" => {
            for (index, block) in document.blocks().iter().enumerate() {
                if let Some(id) = block.attrs.get("id").and_then(Value::as_str) {
                    preview.fact(format!("Paragraph {} ID", index + 1), id);
                }
            }
        }
        _ => {}
    }
    if let Some(peers) = &demo.peers {
        preview.peer = Some(json(peers.bob.editor())?);
        if let Selection::Text { anchor, head } = peers.bob.editor().selection() {
            preview.peer_selection = Some([anchor.0, head.0]);
        }
        if demo.slug == "collaboration-caret" {
            for (awareness, session, caret) in [
                (&peers.alice_caret, &peers.alice, &mut preview.bob_caret),
                (&peers.bob_caret, &peers.bob, &mut preview.alice_caret),
            ] {
                if let Some(peer) = awareness.peers(session).first()
                    && let Some(Selection::Text { head, .. }) = peer.selection
                {
                    *caret = Some(head.0);
                }
            }
        }
    }
    Ok(())
}
