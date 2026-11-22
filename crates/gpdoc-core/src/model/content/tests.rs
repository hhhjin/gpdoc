use super::*;
use crate::model::Attrs;
use serde_json::json;

fn schema(expression: &str) -> Schema {
    Schema::from_json(&json!({"nodes":{
        "doc":{"content":expression},
        "paragraph":{"content":"inline*","group":"block"},
        "heading":{"content":"inline*","group":"block"},
        "horizontal_rule":{"group":"block"},
        "code_block":{"content":"text*","group":"block"},
        "image":{"inline":true,"group":"inline","attrs":{"src":{}}},
        "hard_break":{"inline":true,"group":"inline"},
        "text":{"group":"inline"}
    }}))
    .unwrap()
}

fn nodes(schema: &Schema, names: &str) -> Fragment {
    Fragment::from(
        names
            .split_whitespace()
            .map(|kind| {
                let mut attrs = Attrs::new();
                if kind == "image" {
                    attrs.insert("src".into(), json!("img.png"));
                }
                schema
                    .node(kind, attrs, Fragment::default(), vec![])
                    .unwrap()
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn match_type() {
    let cases = &[
        ("", "", true),
        ("", "image", false),
        ("image*", "", true),
        ("image*", "image", true),
        ("image*", "image image image image", true),
        ("image*", "image text", false),
        ("inline*", "image text", true),
        ("inline*", "paragraph", false),
        ("(paragraph | heading)", "paragraph", true),
        ("(paragraph | heading)", "image", false),
        (
            "paragraph horizontal_rule paragraph",
            "paragraph horizontal_rule paragraph",
            true,
        ),
        (
            "paragraph horizontal_rule",
            "paragraph horizontal_rule paragraph",
            false,
        ),
        (
            "paragraph horizontal_rule paragraph",
            "paragraph horizontal_rule",
            false,
        ),
        (
            "paragraph horizontal_rule",
            "horizontal_rule paragraph horizontal_rule",
            false,
        ),
        ("heading paragraph*", "heading", true),
        ("heading paragraph*", "heading paragraph paragraph", true),
        ("heading paragraph+", "heading paragraph", true),
        ("heading paragraph+", "heading paragraph paragraph", true),
        ("heading paragraph+", "heading", false),
        ("heading paragraph+", "paragraph paragraph", false),
        ("image?", "image", true),
        ("image?", "", true),
        ("image?", "image image", false),
        (
            "(heading paragraph+)+",
            "heading paragraph heading paragraph paragraph",
            true,
        ),
        (
            "(heading paragraph+)+",
            "heading paragraph heading paragraph paragraph horizontal_rule",
            false,
        ),
        ("hard_break{2}", "hard_break hard_break", true),
        ("hard_break{2}", "hard_break", false),
        ("hard_break{2}", "hard_break hard_break hard_break", false),
        ("hard_break{2, 4}", "hard_break hard_break", true),
        (
            "hard_break{2, 4}",
            "hard_break hard_break hard_break hard_break",
            true,
        ),
        ("hard_break{2, 4}", "hard_break hard_break hard_break", true),
        ("hard_break{2, 4}", "hard_break", false),
        (
            "hard_break{2, 4}",
            "hard_break hard_break hard_break hard_break hard_break",
            false,
        ),
        (
            "hard_break{2, 4} text*",
            "hard_break hard_break image",
            false,
        ),
        (
            "hard_break{2, 4} image?",
            "hard_break hard_break image",
            true,
        ),
        ("hard_break{2,}", "hard_break hard_break", true),
        (
            "hard_break{2,}",
            "hard_break hard_break hard_break hard_break",
            true,
        ),
        ("hard_break{2,}", "hard_break", false),
    ];
    for &(expr, children, expected) in cases {
        let schema = schema(expr);
        let matched = children.split_whitespace().try_fold(
            schema.top_node_type().content_match.clone(),
            |state, kind| state.match_type(kind),
        );
        assert_eq!(
            matched.is_some_and(|s| s.valid_end()),
            expected,
            "{expr:?} with {children:?}"
        );
    }
}

#[test]
fn fill_before() {
    let cases = &[
        (
            "paragraph horizontal_rule paragraph",
            "paragraph horizontal_rule",
            "paragraph",
            Some(""),
        ),
        (
            "paragraph horizontal_rule paragraph",
            "paragraph",
            "paragraph",
            Some("horizontal_rule"),
        ),
        ("hard_break*", "hard_break", "hard_break", Some("")),
        ("hard_break*", "hard_break", "", Some("")),
        ("hard_break*", "", "hard_break", Some("")),
        ("hard_break*", "", "", Some("")),
        ("hard_break+", "hard_break", "hard_break", Some("")),
        ("hard_break+", "", "", Some("hard_break")),
        ("hard_break+", "", "image", None),
        ("heading* paragraph*", "heading", "paragraph", Some("")),
        ("heading* paragraph*", "heading", "", Some("")),
        ("heading+ paragraph+", "heading", "paragraph", Some("")),
        ("heading+ paragraph+", "heading", "", Some("paragraph")),
        (
            "hard_break{3}",
            "hard_break",
            "hard_break",
            Some("hard_break"),
        ),
        (
            "hard_break{3}",
            "hard_break hard_break",
            "hard_break hard_break",
            None,
        ),
        (
            "code_block{2} paragraph{2}",
            "code_block",
            "paragraph",
            Some("code_block paragraph"),
        ),
        (
            "heading paragraph? horizontal_rule",
            "heading",
            "",
            Some("horizontal_rule"),
        ),
    ];
    for &(expr, before, after, expected) in cases {
        let schema = schema(expr);
        let start = schema
            .top_node_type()
            .content_match
            .match_fragment(&nodes(&schema, before))
            .unwrap();
        let filled = start.fill_before(&schema, &nodes(&schema, after), true, 0);
        assert_eq!(
            filled,
            expected.map(|s| nodes(&schema, s)),
            "{expr:?}, {before:?}, {after:?}"
        );
    }
}

#[test]
fn fill_across_two_boundaries() {
    let cases = &[
        (
            "paragraph horizontal_rule paragraph horizontal_rule paragraph",
            "paragraph",
            "paragraph",
            "paragraph",
            Some(("horizontal_rule", "horizontal_rule")),
        ),
        (
            "code_block+ paragraph+",
            "code_block",
            "code_block",
            "paragraph",
            Some(("", "")),
        ),
        (
            "code_block+ paragraph+",
            "",
            "",
            "",
            Some(("", "code_block paragraph")),
        ),
        (
            "code_block{3} paragraph{3}",
            "code_block",
            "paragraph",
            "",
            Some(("code_block code_block", "paragraph paragraph")),
        ),
        ("paragraph*", "paragraph", "code_block", "paragraph", None),
        (
            "paragraph{4}",
            "paragraph",
            "paragraph",
            "paragraph",
            Some(("", "paragraph")),
        ),
        ("paragraph{2}", "paragraph", "paragraph", "paragraph", None),
    ];
    for &(expr, before, mid, after, expected) in cases {
        let schema = schema(expr);
        let before = nodes(&schema, before);
        let mid = nodes(&schema, mid);
        let start = &schema.top_node_type().content_match;
        let left = start
            .match_fragment(&before)
            .unwrap()
            .fill_before(&schema, &mid, false, 0);
        let both = left.and_then(|left| {
            let joined = before.append(&left).append(&mid);
            let right = start.match_fragment(&joined)?.fill_before(
                &schema,
                &nodes(&schema, after),
                true,
                0,
            )?;
            Some((left, right))
        });
        assert_eq!(
            both,
            expected.map(|(a, b)| (nodes(&schema, a), nodes(&schema, b))),
            "{expr:?}"
        );
    }
}
