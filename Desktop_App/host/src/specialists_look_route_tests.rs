use super::*;

#[test]
fn explicit_image_capability_overrides_reference_inference() {
    let mut context = DirectorContext::default();
    for (slot, model) in [("image.text_to_image", "chosen-model"), ("image.character_reference", "wrong-model")] {
        context.capability_defaults.insert(slot.into(), Some(ModelRef { api_id: "fixture".into(), model_id: model.into() }));
    }
    let inputs = json!({"image_capability":"image.text_to_image", "reference_images":["ref.png"]});
    let (model, _, slot) = resolve_still_image_model(&context, inputs.as_object().unwrap()).unwrap();
    assert_eq!(slot, "image.text_to_image");
    assert_eq!(model.model_id, "chosen-model");
}

#[test]
fn approval_numbering_starts_at_one_without_source_frame() {
    let mut context = DirectorContext::default();
    context.capability_defaults.insert("image.image_to_image".into(), Some(ModelRef { api_id: "fixture".into(), model_id: "model".into() }));
    let inputs = json!({"image_capability":"image.image_to_image", "reference_images":["first.png", "second.png"]});
    let notes = still_image_approval_notes(&context, inputs.as_object().unwrap(), None);
    assert!(notes.contains("Image 1: `first.png`"), "{notes}");
    assert!(notes.contains("Image 2: `second.png`"), "{notes}");
}
