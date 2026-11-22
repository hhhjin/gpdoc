use gpdoc_core::model::{DocPos, Schema};
use gpdoc_core::transform::{Affinity, Step, split};
use serde_json::json;

#[test]
fn replay_and_invert_a_structural_edit_on_an_independently_loaded_document() {
    let schema_spec = json!({"nodes": {
        "doc": {"content": "paragraph+"},
        "paragraph": {"content": "text*"},
        "text": {}
    }});
    let source = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"abc"}]}]}"#;
    let sender = Schema::from_json(&schema_spec).unwrap();
    let original = sender.document_from_json(source).unwrap();
    let wire_step = split(&original, DocPos(2)).unwrap().to_json();

    let receiver = Schema::from_json(&schema_spec).unwrap();
    let replica = receiver.document_from_json(source).unwrap();
    let step = Step::from_json(&receiver, &wire_step).unwrap();
    let undo = step.invert(&receiver, replica.root()).unwrap();
    let (changed, map) = step.preview(&replica).unwrap();
    assert_eq!(changed.blocks().len(), 2);
    assert_eq!(map.map(DocPos(3), Affinity::After).position, DocPos(5));
    assert_eq!(undo.preview(&changed).unwrap().0, replica);
}
