use novel_codex_story_core::{RepositoryError, writer_scene_limit};
use serde_json::json;

#[test]
fn writer_scene_scope_requires_a_complete_explicit_boundary() {
    assert_eq!(writer_scene_limit(&json!({})).unwrap(), None);
    assert_eq!(
        writer_scene_limit(&json!({"scope": {"type": "wholeChapter"}})).unwrap(),
        None
    );
    assert_eq!(
        writer_scene_limit(&json!({
            "sceneOrder": 2,
            "scope": {"type": "throughScene", "endSceneOrder": 2}
        }))
        .unwrap(),
        Some(2)
    );
    for target in [
        json!({"scope": {}}),
        json!({"scope": "throughScene"}),
        json!({"scope": {"type": "throughScene", "endSceneOrder": 1}}),
        json!({"sceneOrder": 2, "scope": {"type": "throughScene", "endSceneOrder": 1}}),
        json!({"sceneOrder": 0, "scope": {"type": "throughScene", "endSceneOrder": 0}}),
        json!({"sceneOrder": 101, "scope": {"type": "throughScene", "endSceneOrder": 101}}),
    ] {
        assert!(matches!(
            writer_scene_limit(&target),
            Err(RepositoryError::TaskBindingInvalid)
        ));
    }
}
