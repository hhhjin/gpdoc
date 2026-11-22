// Adapted from prosemirror-model 1.25.13 test/{test-mark,test-node,test-diff,test-resolve}.ts.
use gpdoc_core::model::{Attrs, Fragment, Node, Schema};
use serde_json::{Value, json};

fn schema() -> Schema {
    Schema::from_json(&json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block"},
            "hard_break": {"inline": true, "group": "inline"},
            "text": {"group": "inline"}
        },
        "marks": {
            "remark": {"attrs": {"id": {}}, "excludes": "", "inclusive": false},
            "user": {"attrs": {"id": {}}, "excludes": "_"},
            "strong": {"excludes": "em-group"},
            "em": {"group": "em-group"}
        }
    }))
    .unwrap()
}

fn branch(schema: &Schema, kind: &str, content: Vec<Node>) -> Node {
    schema
        .node(kind, Attrs::new(), content.into(), Vec::new())
        .unwrap()
}

#[test]
fn marks_respect_exclusions_rank_and_distinct_attributes() {
    let schema = schema();
    let mark = |value: Value| schema.mark_from_json(&value).unwrap();
    let remark = |id| mark(json!({"type":"remark","attrs":{"id":id}}));
    let first = remark(1);
    let second = remark(2);
    let user = mark(json!({"type":"user","attrs":{"id":1}}));
    let strong = mark(json!({"type":"strong"}));
    let em = mark(json!({"type":"em"}));
    let set = second.add_to_set(&schema, std::slice::from_ref(&first));
    assert_eq!(set, vec![first.clone(), second.clone()]);
    assert_eq!(first.add_to_set(&schema, &set), set);
    assert_eq!(user.add_to_set(&schema, &set), vec![user.clone()]);
    assert_eq!(
        em.add_to_set(&schema, std::slice::from_ref(&user)),
        vec![user]
    );
    assert_eq!(
        strong.add_to_set(&schema, &[first.clone(), em.clone()]),
        vec![first.clone(), strong.clone()]
    );
    assert_eq!(
        em.add_to_set(&schema, &[first.clone(), strong.clone()]),
        vec![first.clone(), strong]
    );
    assert_eq!(second.remove_from_set(&set), vec![first]);
}

#[test]
fn noninclusive_mark_is_active_only_inside_or_across_equal_marks() {
    let schema = schema();
    let make_mark = |id| {
        schema
            .mark_from_json(&json!({"type":"remark","attrs":{"id":id}}))
            .unwrap()
    };
    let first = make_mark(1);
    let other = make_mark(2);
    let strong = schema.mark("strong", Attrs::new()).unwrap();
    let doc = branch(
        &schema,
        "doc",
        vec![branch(
            &schema,
            "paragraph",
            vec![
                schema.text("abc", vec![first.clone()]).unwrap(),
                schema
                    .text("de", vec![first.clone(), strong.clone()])
                    .unwrap(),
                schema.text("f", vec![other]).unwrap(),
            ],
        )],
    );
    let marks = |pos| doc.resolve(pos).unwrap().marks(&schema);
    assert!(marks(1).is_empty());
    assert_eq!(marks(2), vec![first.clone()]);
    assert_eq!(marks(4), vec![first]);
    assert_eq!(marks(6), vec![strong]);
    assert!(marks(7).is_empty());
}

#[test]
fn resolves_child_indices_at_every_ancestor() {
    let s = schema();
    let t = |text| s.text(text, vec![]).unwrap();
    let p = |children| branch(&s, "paragraph", children);
    let b = |children| branch(&s, "blockquote", children);
    let three = s
        .text("three", vec![s.mark("em", Attrs::new()).unwrap()])
        .unwrap();
    let doc = branch(
        &s,
        "doc",
        vec![b(vec![
            p(vec![t("one")]),
            b(vec![p(vec![t("two "), three]), p(vec![t("four")])]),
        ])],
    );
    let pos = doc.resolve(12).unwrap();
    for (d, boundary) in [
        (0, vec![0, 26]),
        (1, vec![1, 6, 25]),
        (2, vec![7, 18, 24]),
        (3, vec![8, 12, 17]),
    ] {
        for (i, expected) in boundary.into_iter().enumerate() {
            assert_eq!(pos.pos_at_index(i, d), expected);
        }
    }
}

#[test]
fn diff_and_text_cut_use_utf16_and_keep_surrogate_pairs_whole() {
    let s = schema();
    let paragraph = |text| branch(&s, "paragraph", vec![s.text(text, vec![]).unwrap()]);
    let a: Fragment = vec![paragraph("a𝛿b")].into();
    let b: Fragment = vec![paragraph("a𝛾b")].into();
    assert_eq!(a.find_diff_start(&b), Some(2));
    assert_eq!(a.find_diff_end(&b), Some((4, 4)));
    assert_eq!(a.cut(2, 4).unwrap().child(0).text_content(), "𝛿");
    assert!(a.cut(2, 3).is_err());
    assert!(a.cut(3, 4).is_err());
}

#[test]
fn nested_traversal_and_text_between_preserve_empty_block_separators() {
    let s = schema();
    let p = |children| branch(&s, "paragraph", children);
    let doc = branch(
        &s,
        "doc",
        vec![
            branch(
                &s,
                "blockquote",
                vec![p(vec![s.text("ab", vec![]).unwrap()]), p(vec![])],
            ),
            p(vec![
                s.text("c", vec![]).unwrap(),
                branch(&s, "hard_break", vec![]),
                s.text("d", vec![]).unwrap(),
            ]),
        ],
    );
    assert_eq!(
        doc.text_between(0, doc.content.size(), "|", "~").unwrap(),
        "ab||c~d"
    );
    let mut visited = vec![];
    doc.nodes_between(0, doc.content.size(), |n, pos, _, _| {
        visited.push((n.kind.clone(), pos));
        n.kind != "blockquote"
    })
    .unwrap();
    assert_eq!(
        visited,
        [
            ("blockquote", 0),
            ("paragraph", 8),
            ("text", 9),
            ("hard_break", 10),
            ("text", 11)
        ]
        .map(|(s, i)| (s.to_owned(), i))
    );
}

#[test]
fn types_are_bound_to_the_schema_that_allocated_them() {
    let a = schema();
    let b = schema();
    let text_a = a.text("abc", vec![]).unwrap();
    let text_b = b.text("abc", vec![]).unwrap();
    assert!(!text_a.same_markup(&text_b));
    assert_ne!(
        a.mark("em", Attrs::new()).unwrap(),
        b.mark("em", Attrs::new()).unwrap()
    );
    let left = branch(&a, "paragraph", vec![text_a.clone()]);
    let right = branch(&b, "paragraph", vec![text_b.clone()]);
    assert!(
        a.node(
            "paragraph",
            Attrs::new(),
            Fragment::from(text_b.clone()),
            vec![]
        )
        .is_err()
    );
    assert!(!left.can_replace(&a, 0, 0, &Fragment::from(text_b)));
    assert!(
        left.replace(&a, 0, 1, &right.slice(0, 1, false).unwrap())
            .is_err()
    );
    assert!(right.check(&a).is_err());
    assert!(
        a.clone()
            .node("paragraph", Attrs::new(), Fragment::from(text_a), vec![])
            .is_ok()
    );
}
