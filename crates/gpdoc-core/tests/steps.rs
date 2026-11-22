//! Ported cases from test/test-step.ts and test/test-replace_step.ts at prosemirror-transform 1.12.6.
use gpdoc_core::model::{Node, Schema, Slice};
use gpdoc_core::transform::{Mapping, ReplaceAroundStep, Step, StepMap};
use serde_json::{Value, json};

fn schema() -> Schema {
    Schema::from_json(&json!({"nodes":{"doc":{"content":"block+"},"p":{"group":"block","content":"inline*"},"quote":{"group":"block","content":"block+"},"text":{"group":"inline"}},"marks":{"em":{}}})).unwrap()
}

fn doc(schema: &Schema, content: Value) -> Node {
    schema
        .node_from_json(&json!({"type":"doc","content":content}))
        .unwrap()
}
fn p(s: &str) -> Value {
    json!({"type":"p","content":[{"type":"text","text":s}]})
}
fn step(schema: &Schema, from: usize, to: usize, insert: &str) -> Step {
    let mut value = json!({"stepType":"replace","from":from,"to":to});
    if !insert.is_empty() {
        value["slice"] = json!({"content":[{"type":"text","text":insert}]});
    }
    Step::from_json(schema, &value).unwrap()
}

#[test]
fn adjacent_edits_merge_to_the_same_document() {
    let s = schema();
    let doc = doc(&s, json!([p("foobar")]));
    let cases = [
        ((2, 2, "a"), (3, 3, "b"), true),
        ((2, 2, "a"), (2, 2, "b"), true),
        ((2, 2, "a"), (4, 4, "b"), false),
        ((3, 3, "a"), (2, 2, "b"), false),
        ((3, 4, ""), (2, 3, ""), true),
        ((2, 3, ""), (2, 3, ""), true),
        ((1, 2, ""), (2, 3, ""), false),
        ((2, 3, ""), (2, 2, "x"), true),
        ((2, 2, "quux"), (6, 6, "baz"), true),
        ((2, 2, "quux"), (2, 2, "baz"), true),
        ((2, 5, ""), (2, 4, ""), true),
        ((4, 6, ""), (2, 4, ""), true),
        ((3, 4, "x"), (4, 5, "y"), true),
    ];
    for ((a, b, x), (c, d, y), can_merge) in cases {
        let one = step(&s, a, b, x);
        let two = step(&s, c, d, y);
        let merged = one.merge(&two);
        assert_eq!(merged.is_some(), can_merge);
        if let Some(merged) = merged {
            let after = merged.apply(&s, &doc).unwrap();
            assert_eq!(after, two.apply(&s, &one.apply(&s, &doc).unwrap()).unwrap());
            assert_eq!(
                merged.invert(&s, &doc).unwrap().apply(&s, &after).unwrap(),
                doc
            );
        }
    }
}

#[test]
fn gap_content_must_fit_closed_slice_but_can_join_open_slice() {
    let s = schema();
    let before = doc(&s, json!([p("a")]));
    let invalid = Step::from_json(&s,&json!({"stepType":"replaceAround","from":1,"to":2,"gapFrom":1,"gapTo":2,"slice":{"content":[{"type":"quote"}]},"insert":1,"structure":true})).unwrap();
    assert_eq!(
        invalid.apply(&s, &before).unwrap_err().to_string(),
        "Content does not fit in gap"
    );
    let before = doc(&s, json!([{"type":"quote","content":[p("x")]}]));
    let open=Step::from_json(&s,&json!({"stepType":"replaceAround","from":0,"to":1,"gapFrom":1,"gapTo":1,"slice":{"openEnd":1,"content":[{"type":"quote"}]},"insert":1,"structure":true})).unwrap();
    assert_eq!(open.apply(&s, &before).unwrap(), before);
}

#[test]
fn mapped_steps_keep_inserts_at_boundaries() {
    let s = schema();
    let before = doc(&s, json!([p("abcd")]));
    for pos in [0, 5] {
        let step = ReplaceAroundStep {
            from: pos,
            to: pos,
            gap_from: pos,
            gap_to: pos,
            slice: Slice::empty(),
            insert: 0,
            structure: true,
        };
        let next = step.apply(&s, &before).unwrap();
        assert_eq!(next, before);
        assert_eq!(
            step.invert(&before).unwrap().apply(&s, &next).unwrap(),
            before
        );
        assert!(
            step.map(&Mapping::new(vec![StepMap::new(vec![[pos, 0, 10]])]))
                .is_none()
        );
    }
    let add = step(&s, 1, 1, "x");
    let Step::Replace(add) = add else {
        unreachable!()
    };
    let insert = StepMap::new(vec![[1, 0, 2]]);
    assert_eq!(add.map(&insert).unwrap().from, 3);
    assert_eq!(add.map_with_bias(&insert, -1).unwrap().from, 1);
}

#[test]
fn mark_groups_cannot_hide_structural_edits_behind_an_identity_map() {
    let s = schema();
    assert!(Step::mark_set(vec![step(&s, 1, 2, "abc")]).is_err());
    let mark = Step::from_json(
        &s,
        &json!({"stepType":"addMark","from":1,"to":2,"mark":{"type":"em"}}),
    )
    .unwrap();
    let nested = Step::mark_set(vec![Step::mark_set(vec![mark.clone()]).unwrap()]).unwrap();
    let doc = doc(&s, json!([p("abc")]));
    assert_eq!(
        nested.apply(&s, &doc).unwrap(),
        mark.apply(&s, &doc).unwrap()
    );
    assert_eq!(nested.get_map().map(3, 1), 3);
}

#[test]
fn decoding_rejects_ranges_that_would_overflow_a_step_map() {
    let s = schema();
    for value in [
        json!({"stepType":"replace","from":3,"to":1}),
        json!({"stepType":"replace","from":usize::MAX,"to":usize::MAX,"slice":{"content":[{"type":"text","text":"A"}]}}),
        json!({"stepType":"replaceAround","from":1,"to":4,"gapFrom":0,"gapTo":4,"insert":0}),
        json!({"stepType":"replaceAround","from":1,"to":4,"gapFrom":2,"gapTo":4,"insert":1}),
        json!({"stepType":"replaceAround","from":1,"to":usize::MAX,"gapFrom":1,"gapTo":usize::MAX,"insert":0,"slice":{"content":[{"type":"text","text":"x"}]}}),
    ] {
        assert!(Step::from_json(&s, &value).is_err(), "accepted {value}");
    }
}
