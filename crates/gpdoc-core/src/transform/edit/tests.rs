use super::*;
use crate::model::declarations::{
    AllowedMarks, AttrSpecs, ContentSpec, MarkSpec, NodeSpec, Placement,
};
use crate::model::{Content, ContentRef, TextRun};

fn schema() -> Schema {
    let text = ContentSpec::text(AllowedMarks::All);
    let children = ContentSpec::expression("block+");
    let nodes = [("p", text.clone()), ("h", text), ("quote", children)]
        .into_iter()
        .map(|(name, content)| NodeSpec {
            placement: Placement::Block,
            name: name.into(),
            groups: vec!["block".into()],
            attrs: AttrSpecs::new(),
            content,
        })
        .collect();
    let marks = [("bold", true), ("link", false)]
        .into_iter()
        .map(|(name, inclusive_end)| MarkSpec {
            groups: vec![],
            excludes: None,
            name: name.into(),
            attrs: AttrSpecs::new(),
            inclusive_end,
        })
        .collect();
    Schema::from_declarations(nodes, marks, "block+").unwrap()
}

fn p(text: &str) -> Node {
    Node::text("p", text)
}
fn quote(nodes: Vec<Node>) -> Node {
    Node::container("quote", nodes)
}
fn range(start: usize, end: usize) -> Range<DocPos> {
    DocPos(start)..DocPos(end)
}
fn replace(start: usize, end: usize, text: &str) -> Edit {
    Edit::ReplaceText {
        range: range(start, end),
        text: text.into(),
    }
}
fn mark(kind: &str) -> Mark {
    Mark::new(kind, Attrs::new())
}
fn run(text: &str, marks: &[&str]) -> TextRun {
    TextRun {
        text: text.into(),
        marks: marks.iter().map(|kind| mark(kind)).collect(),
    }
}
fn text(runs: Vec<TextRun>) -> Node {
    Node::new(
        "p",
        Attrs::new(),
        Content::Inline(runs.into_iter().map(Node::from).collect()),
    )
}

#[test]
fn replace_across_ancestors_preserves_right_siblings_and_their_positions() {
    let schema = schema();
    let doc = schema
        .document(vec![Node::text("h", "abc"), quote(vec![p("def"), p("Z")])])
        .unwrap();
    let untouched = doc.blocks()[1].child(1);
    let (result, map) = apply_edit(&doc, &replace(2, 8, "X")).unwrap();
    assert_eq!(
        result.blocks(),
        [Node::text("h", "aXef"), quote(vec![untouched.clone()])]
    );
    let ContentRef::Blocks(right) = result.blocks()[1].content() else {
        panic!()
    };
    assert!(right[0].ptr_eq(untouched));
    assert_eq!(map.map(DocPos(9), Affinity::After).position, DocPos(4));
    let after = result
        .nodes()
        .into_iter()
        .find(|(_, node)| node.ptr_eq(untouched))
        .unwrap()
        .0;
    let before = doc
        .nodes()
        .into_iter()
        .find(|(_, node)| node.ptr_eq(untouched))
        .unwrap()
        .0;
    assert_eq!(map.map(before, Affinity::After).position, after);
    assert_eq!(map.inverse().map(after, Affinity::After).position, before);
}

#[test]
fn deep_left_and_empty_right_wrappers_close_at_the_join() {
    let schema = schema();
    let doc = schema
        .document(vec![quote(vec![quote(vec![p("ab")])]), p("cd"), p("ef")])
        .unwrap();
    let (result, _) = apply_edit(&doc, &replace(4, 10, "X")).unwrap();
    assert_eq!(
        result.blocks(),
        [quote(vec![quote(vec![p("aXd")])]), p("ef")]
    );
    let doc = schema
        .document(vec![p("ab"), quote(vec![quote(vec![p("cd")])]), p("ef")])
        .unwrap();
    let (result, _) = apply_edit(&doc, &replace(2, 8, "X")).unwrap();
    assert_eq!(result.blocks(), [p("aXd"), p("ef")]);
}

#[test]
fn wrapping_and_unwrapping_keep_children_and_map_surviving_offsets() {
    let schema = schema();
    let doc = schema.document(vec![p("a"), p("😀"), p("b")]).unwrap();
    let child = &doc.blocks()[1];
    let wrap = Edit::Wrap {
        range: range(3, 7),
        kind: "quote".into(),
        attrs: Attrs::new(),
    };
    let (wrapped, forward) = apply_edit(&doc, &wrap).unwrap();
    assert_eq!(
        wrapped.blocks(),
        [p("a"), quote(vec![child.clone()]), p("b")]
    );
    let child_before = forward.map(DocPos(3), Affinity::After);
    assert_eq!(
        child_before,
        MappedPosition {
            position: DocPos(4),
            deleted: false
        }
    );
    assert!(
        wrapped
            .node_at(child_before.position)
            .unwrap()
            .ptr_eq(child)
    );
    let (unwrapped, backward) = apply_edit(&wrapped, &Edit::Unwrap { before: DocPos(3) }).unwrap();
    assert_eq!(doc, unwrapped);
    assert_eq!(
        backward
            .map(child_before.position, Affinity::After)
            .position,
        DocPos(3)
    );
    assert!(backward.map(DocPos(3), Affinity::After).deleted);
}

#[test]
fn split_and_join_text_with_adjacent_marks_and_utf16_boundaries() {
    let schema = schema();
    let doc = schema
        .document(vec![text(vec![run("a😀b", &["bold"])])])
        .unwrap();
    assert!(split(&doc, DocPos(3)).is_err());
    let step = split(&doc, DocPos(4)).unwrap();
    let decoded = Step::from_json(&schema, &step.to_json()).unwrap();
    let (split, map) = decoded.preview(&doc).unwrap();
    assert_eq!(
        split.blocks(),
        [
            text(vec![run("a😀", &["bold"])]),
            text(vec![run("b", &["bold"])])
        ]
    );
    assert_eq!(map.map(DocPos(4), Affinity::After).position, DocPos(6));
    let (joined, _) = apply_edit(&split, &Edit::Join { at: DocPos(5) }).unwrap();
    assert_eq!(joined, doc);
}

#[test]
fn mark_and_unmark_across_blocks_without_moving_positions() {
    let schema = schema();
    let doc = schema.document(vec![p("abc"), p("def")]).unwrap();
    let step = Edit::SetMark {
        range: range(2, 8),
        mark: mark("bold"),
    };
    let (marked, map) = apply_edit(&doc, &step).unwrap();
    assert_eq!(
        marked.blocks(),
        [
            text(vec![run("a", &[]), run("bc", &["bold"])]),
            text(vec![run("de", &["bold"]), run("f", &[])]),
        ]
    );
    for i in 0..=10 {
        assert_eq!(
            map.map(DocPos(i), Affinity::After),
            MappedPosition {
                position: DocPos(i),
                deleted: false
            }
        );
    }
    let step = Edit::RemoveMark {
        range: range(2, 8),
        kind: "bold".into(),
    };
    assert_eq!(apply_edit(&marked, &step).unwrap().0, doc);
}

#[test]
fn typing_inherits_formatting_at_the_start_inside_and_end_of_a_span() {
    let schema = schema();
    let original = run("ab", &["bold", "link"]);
    let doc = schema.document(vec![text(vec![original.clone()])]).unwrap();
    for (pos, expected) in [
        (1, vec![run("X", &["bold"]), original.clone()]),
        (2, vec![run("aXb", &["bold", "link"])]),
        (3, vec![original, run("X", &["bold"])]),
    ] {
        let (result, _) = apply_edit(&doc, &replace(pos, pos, "X")).unwrap();
        assert_eq!(result.blocks(), [text(expected)]);
    }
}

#[test]
fn structural_edits_require_siblings_and_preserve_text() {
    let schema = schema();
    let doc = schema.document(vec![quote(vec![p("a")]), p("b")]).unwrap();
    let invalid_steps = [
        Edit::ReplaceNodes {
            range: range(1, 5),
            nodes: vec![p("x")],
        },
        Edit::Boundaries(vec![BoundaryEdit {
            range: range(2, 3),
            insert: vec![],
        }]),
        Edit::Boundaries(vec![BoundaryEdit {
            range: range(0, 0),
            insert: vec![Boundary::Close],
        }]),
        Edit::Join { at: DocPos(5) },
    ];
    for step in invalid_steps {
        assert!(apply_edit(&doc, &step).is_err());
    }
}

#[test]
fn inserting_an_oversize_shared_subtree_returns_an_error() {
    let schema = schema();
    let doc = schema.document(vec![p("a")]).unwrap();
    let original = doc.blocks()[0].clone();
    let mut huge = p("b");
    for _ in 0..usize::BITS {
        huge = quote(vec![huge.clone(), huge]);
    }
    let append = Edit::ReplaceNodes {
        range: range(3, 3),
        nodes: vec![huge.clone(), huge],
    };
    assert!(matches!(
        apply_edit(&doc, &append),
        Err(Error::InvalidDocument(_))
    ));
    assert!(doc.blocks()[0].ptr_eq(&original));
}

#[test]
fn empty_cursor_replacement_reuses_the_normalized_document_and_validates_the_position() {
    let schema = schema();
    let original = text(vec![run("a", &[]), run("😀", &[])]);
    let doc = schema.document(vec![original.clone()]).unwrap();
    let (same, map) = apply_edit(&doc, &replace(2, 2, "")).unwrap();
    assert!(same.blocks()[0].ptr_eq(&doc.blocks()[0]));
    assert!(map.is_identity());
    assert!(apply_edit(&doc, &replace(3, 3, "")).is_err());
}
