// Ported from prosemirror-model 1.25.13, test/test-replace.ts.
// Positions count UTF-16 units from the start of the document's content.
use gpdoc_core::model::{Node, Schema, Slice};
use serde_json::{Value, json};

fn schema() -> Schema {
    Schema::from_json(&json!({ "nodes": {
        "doc": { "content": "block+" },
        "paragraph": { "group": "block", "content": "text*" },
        "heading": { "group": "block", "content": "text*" },
        "blockquote": { "group": "block", "content": "block+" },
        "list": { "group": "block", "content": "item+" },
        "item": { "content": "paragraph block*" },
        "text": {}
    }}))
    .unwrap()
}

fn parse(schema: &Schema, tree: Value) -> Node {
    schema.node_from_json(&tree).unwrap()
}

fn container<const N: usize>(kind: &str, children: [Value; N]) -> Value {
    let mut node = json!({"type": kind});
    if N > 0 {
        node["content"] = Vec::from(children).into();
    }
    node
}

fn text(s: &str) -> Value {
    json!({"type":"text","text":s})
}
fn doc<const N: usize>(children: [Value; N]) -> Value {
    container("doc", children)
}
fn blockquote<const N: usize>(children: [Value; N]) -> Value {
    container("blockquote", children)
}
fn ul<const N: usize>(children: [Value; N]) -> Value {
    container("list", children)
}
fn li<const N: usize>(children: [Value; N]) -> Value {
    container("item", children)
}
fn p(s: &str) -> Value {
    if s.is_empty() {
        container("paragraph", [])
    } else {
        container("paragraph", [text(s)])
    }
}
fn h1(s: &str) -> Value {
    container("heading", [text(s)])
}
#[test]
fn joins_on_delete() {
    let schema = schema();
    let document = parse(&schema, doc([p("one"), p("two")]));
    let slice = Slice::empty();
    let result = document.replace(&schema, 3, 7, &slice);
    assert_eq!(result.unwrap().to_json(), doc([p("onwo")]));
}

#[test]
fn merges_matching_blocks() {
    let schema = schema();
    let document = parse(&schema, doc([p("one"), p("two")]));
    let slice = parse(&schema, doc([p("xxxx"), p("yyyy")]))
        .slice(3, 9, false)
        .unwrap();
    let result = document.replace(&schema, 3, 7, &slice);
    assert_eq!(result.unwrap().to_json(), doc([p("onxx"), p("yywo")]));
}

#[test]
fn merges_when_adding_text() {
    let schema = schema();
    let document = parse(&schema, doc([p("one"), p("two")]));
    let slice = parse(&schema, doc([p("H")])).slice(1, 2, false).unwrap();
    let result = document.replace(&schema, 3, 7, &slice);
    assert_eq!(result.unwrap().to_json(), doc([p("onHwo")]));
}

#[test]
fn can_insert_text() {
    let schema = schema();
    let document = parse(&schema, doc([p("before"), p("one"), p("after")]));
    let slice = parse(&schema, doc([p("H")])).slice(1, 2, false).unwrap();
    let result = document.replace(&schema, 11, 11, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([p("before"), p("onHe"), p("after")])
    );
}

#[test]
fn doesnt_merge_non_matching_blocks() {
    let schema = schema();
    let document = parse(&schema, doc([p("one"), p("two")]));
    let slice = parse(&schema, doc([h1("H")])).slice(1, 2, false).unwrap();
    let result = document.replace(&schema, 3, 7, &slice);
    assert_eq!(result.unwrap().to_json(), doc([p("onHwo")]));
}

#[test]
fn can_merge_a_nested_node() {
    let schema = schema();
    let document = parse(
        &schema,
        doc([blockquote([blockquote([p("one"), p("two")])])]),
    );
    let slice = parse(&schema, doc([p("H")])).slice(1, 2, false).unwrap();
    let result = document.replace(&schema, 5, 9, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([blockquote([p("onHwo")])])])
    );
}

#[test]
fn can_replace_within_a_block() {
    let schema = schema();
    let document = parse(&schema, doc([blockquote([p("abcd")])]));
    let slice = parse(&schema, doc([p("xyz")])).slice(2, 3, false).unwrap();
    let result = document.replace(&schema, 3, 5, &slice);
    assert_eq!(result.unwrap().to_json(), doc([blockquote([p("ayd")])]));
}

#[test]
fn can_insert_a_lopsided_slice() {
    let schema = schema();
    let document = parse(
        &schema,
        doc([blockquote([blockquote([p("one"), p("two"), p("three")])])]),
    );
    let slice = parse(
        &schema,
        doc([blockquote([p("aaaa"), p("bb"), p("cc"), p("dd")])]),
    )
    .slice(4, 15, false)
    .unwrap();
    let result = document.replace(&schema, 5, 12, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([blockquote([
            p("onaa"),
            p("bb"),
            p("cc"),
            p("three")
        ])])])
    );
}

#[test]
fn can_insert_a_deep_lopsided_slice() {
    let schema = schema();
    let document = parse(
        &schema,
        doc([blockquote([
            blockquote([p("one"), p("two"), p("three")]),
            p("x"),
        ])]),
    );
    let slice = parse(
        &schema,
        doc([blockquote([p("aaaa"), p("bb"), p("cc")]), p("dd")]),
    )
    .slice(4, 16, false)
    .unwrap();
    let result = document.replace(&schema, 5, 20, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([
            blockquote([p("onaa"), p("bb"), p("cc")]),
            p("x")
        ])])
    );
}

#[test]
fn can_merge_multiple_levels() {
    let schema = schema();
    let document = parse(
        &schema,
        doc([
            blockquote([blockquote([p("hello")])]),
            blockquote([blockquote([p("a")])]),
        ]),
    );
    let slice = Slice::empty();
    let result = document.replace(&schema, 7, 14, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([blockquote([p("hella")])])])
    );
}

#[test]
fn can_merge_multiple_levels_while_inserting() {
    let schema = schema();
    let document = parse(
        &schema,
        doc([
            blockquote([blockquote([p("hello")])]),
            blockquote([blockquote([p("a")])]),
        ]),
    );
    let slice = parse(&schema, doc([p("i")])).slice(1, 2, false).unwrap();
    let result = document.replace(&schema, 7, 14, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([blockquote([p("hellia")])])])
    );
}

#[test]
fn can_insert_a_split() {
    let schema = schema();
    let document = parse(&schema, doc([p("foobar")]));
    let slice = parse(&schema, doc([p("x"), p("y")]))
        .slice(1, 5, false)
        .unwrap();
    let result = document.replace(&schema, 4, 4, &slice);
    assert_eq!(result.unwrap().to_json(), doc([p("foox"), p("ybar")]));
}

#[test]
fn can_insert_a_deep_split() {
    let schema = schema();
    let document = parse(&schema, doc([blockquote([p("fooxbar")])]));
    let slice = parse(&schema, doc([blockquote([p("x")]), blockquote([p("y")])]))
        .slice(2, 8, false)
        .unwrap();
    let result = document.replace(&schema, 5, 6, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([p("foox")]), blockquote([p("ybar")])])
    );
}

#[test]
fn can_add_a_split_one_level_up() {
    let schema = schema();
    let document = parse(&schema, doc([blockquote([p("foou"), p("vbar")])]));
    let slice = parse(&schema, doc([blockquote([p("x")]), blockquote([p("y")])]))
        .slice(2, 8, false)
        .unwrap();
    let result = document.replace(&schema, 5, 9, &slice);
    assert_eq!(
        result.unwrap().to_json(),
        doc([blockquote([p("foox")]), blockquote([p("ybar")])])
    );
}

#[test]
fn keeps_the_node_type_of_the_left_node() {
    let schema = schema();
    let document = parse(&schema, doc([h1("foobar")]));
    let slice = parse(&schema, doc([p("foobaz")]))
        .slice(4, 8, false)
        .unwrap();
    let result = document.replace(&schema, 4, 8, &slice);
    assert_eq!(result.unwrap().to_json(), doc([h1("foobaz")]));
}

#[test]
fn keeps_the_node_type_even_when_empty() {
    let schema = schema();
    let document = parse(&schema, doc([h1("bar")]));
    let slice = parse(&schema, doc([p("foobaz")]))
        .slice(4, 8, false)
        .unwrap();
    let result = document.replace(&schema, 1, 5, &slice);
    assert_eq!(result.unwrap().to_json(), doc([h1("baz")]));
}

#[test]
fn doesnt_allow_the_left_side_to_be_too_deep() {
    let schema = schema();
    let document = parse(&schema, doc([p("")]));
    let slice = parse(&schema, doc([blockquote([p("")])]))
        .slice(2, 4, false)
        .unwrap();
    let result = document.replace(&schema, 1, 1, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("deeper"), "{error}");
}

#[test]
fn doesnt_allow_a_depth_mismatch() {
    let schema = schema();
    let document = parse(&schema, doc([p("")]));
    let slice = parse(&schema, doc([p("")])).slice(0, 1, false).unwrap();
    let result = document.replace(&schema, 1, 1, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("inconsistent"), "{error}");
}

#[test]
fn rejects_a_bad_fit() {
    let schema = schema();
    let document = parse(&schema, doc([]));
    let slice = parse(&schema, doc([p("foo")])).slice(1, 4, false).unwrap();
    let result = document.replace(&schema, 0, 0, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("invalid content"), "{error}");
}

#[test]
fn rejects_unjoinable_content() {
    let schema = schema();
    let document = parse(&schema, doc([ul([li([p("a")])])]));
    let slice = parse(&schema, doc([p("foo")])).slice(4, 5, false).unwrap();
    let result = document.replace(&schema, 6, 7, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("cannot join"), "{error}");
}

#[test]
fn rejects_an_unjoinable_delete() {
    let schema = schema();
    let document = parse(&schema, doc([blockquote([p("a")]), ul([li([p("b")])])]));
    let slice = Slice::empty();
    let result = document.replace(&schema, 4, 6, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("cannot join"), "{error}");
}

#[test]
fn check_content_validity() {
    let schema = schema();
    let document = parse(&schema, doc([blockquote([p("hi")])]));
    let slice = parse(&schema, doc([blockquote([text("hi")])]))
        .slice(3, 4, false)
        .unwrap();
    let result = document.replace(&schema, 1, 6, &slice);
    let error = result.unwrap_err().to_string().to_lowercase();
    assert!(error.contains("invalid content"), "{error}");
}
