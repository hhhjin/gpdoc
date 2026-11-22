// Cases from prosemirror-transform 1.12.6, test/test-trans.ts, describe("replace").
use gpdoc_core::model::{Fragment, Node, Schema, Slice};
use gpdoc_core::transform::replace_step;
use serde_json::{Value, json};

fn paragraph(kind: &str, text: &str) -> Value {
    if text.is_empty() {
        json!({"type":kind})
    } else {
        json!({"type":kind,"content":[{"type":"text","text":text}]})
    }
}

fn fit(schema: &Schema, doc: &Node, from: usize, to: usize, slice: &Slice) -> Node {
    match replace_step(schema, doc, from, to, slice).unwrap() {
        Some(step) => step.apply(schema, doc).unwrap(),
        None => doc.clone(),
    }
}

#[test]
fn required_heading_and_body_split_an_open_slice_between_parents() {
    let schema = Schema::from_json(&json!({"nodes":{
        "doc":{"content":"heading body"},
        "paragraph":{"group":"block","content":"text*"},
        "heading":{"group":"block","content":"text*"},
        "body":{"content":"block+"},
        "text":{}
    }}))
    .unwrap();
    let document = |heading, body: Vec<Value>| {
        schema.node_from_json(&json!({
        "type":"doc","content":[paragraph("heading", heading), {"type":"body","content":body}]
    })).unwrap()
    };

    let original = document(
        "Head",
        vec![paragraph("paragraph", "One"), paragraph("paragraph", "Two")],
    );
    let slice = original.slice(8, 16, false).unwrap();
    let next = fit(&schema, &original, 0, original.content.size(), &slice);
    assert_eq!(next, document("One", vec![paragraph("paragraph", "Two")]));

    let slice = original.slice(6, original.content.size(), false).unwrap();
    let next = fit(&schema, &original, 0, original.content.size(), &slice);
    assert_eq!(
        next,
        document(
            "",
            vec![paragraph("paragraph", "One"), paragraph("paragraph", "Two")]
        )
    );
}

#[test]
fn isolating_nodes_can_join_only_when_the_slice_is_open_on_both_sides() {
    let schema = Schema::from_json(&json!({"nodes":{
        "doc":{"content":"block+"},
        "paragraph":{"group":"block","content":"text*"},
        "iso":{"group":"block","content":"block+","isolating":true},
        "text":{}
    }}))
    .unwrap();
    let iso = json!({"type":"iso","content":[paragraph("paragraph","two")]});
    let content = Fragment::from(schema.node_from_json(&iso).unwrap());
    let doc = |children: Vec<Value>| {
        schema
            .node_from_json(&json!({"type":"doc","content":children}))
            .unwrap()
    };
    let original = doc(vec![paragraph("paragraph", "one")]);
    let split = fit(&schema, &original, 2, 3, &Slice::new(content.clone(), 2, 0));
    assert_eq!(
        split,
        doc(vec![
            paragraph("paragraph", "o"),
            iso,
            paragraph("paragraph", "e")
        ])
    );
    let joined = fit(&schema, &original, 2, 3, &Slice::new(content, 2, 2));
    assert_eq!(joined, doc(vec![paragraph("paragraph", "otwoe")]));
}
