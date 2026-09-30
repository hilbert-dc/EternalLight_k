use std::collections::HashSet;

use serde_json::Value;

use crate::RepositoryError;

/// Generated plans must state testable scene effects before a chapter can be
/// written. These descriptions are intentions, never realized world state.
pub(crate) fn validate_generated_plan(content_json: &str) -> Result<(), RepositoryError> {
    let invalid = || RepositoryError::TaskOutputForbidden;
    let plan: Value = serde_json::from_str(content_json).map_err(|_| invalid())?;
    let chapters = plan["payload"]["chapters"].as_array().ok_or_else(invalid)?;
    if chapters.is_empty() || chapters.len() > 12 {
        return Err(invalid());
    }
    let mut reference_count = 0;
    for chapter in chapters {
        crate::bound_writer_plan::validate_plan_position(chapter).map_err(|_| invalid())?;
        let scenes = chapter["scenes"].as_array().ok_or_else(invalid)?;
        if scenes.is_empty() || scenes.len() > 24 {
            return Err(invalid());
        }
        let mut scene_ids = HashSet::new();
        for scene in scenes {
            let scene_id = scene["id"]
                .as_str()
                .filter(|id| !id.trim().is_empty() && id.len() <= 160)
                .ok_or_else(invalid)?;
            if !scene_ids.insert(scene_id) {
                return Err(invalid());
            }
            let effects = scene["expectedEffects"].as_array().ok_or_else(invalid)?;
            if effects.is_empty() || effects.len() > 12 {
                return Err(invalid());
            }
            let mut effect_ids = HashSet::new();
            for effect in effects {
                let references = effect["commitmentRefs"].as_array().ok_or_else(invalid)?;
                reference_count += references.len();
                if references.len() > 4 || reference_count > 64 {
                    return Err(invalid());
                }
                let mut seen = HashSet::new();
                for reference in references {
                    if !seen.insert(crate::plan_commitments::CommitmentRef::parse(reference)?) {
                        return Err(invalid());
                    }
                }
                let effect_id = effect["id"]
                    .as_str()
                    .filter(|id| !id.trim().is_empty() && id.len() <= 160)
                    .ok_or_else(invalid)?;
                effect["description"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty() && text.chars().count() <= 500)
                    .ok_or_else(invalid)?;
                if !effect_ids.insert(effect_id)
                    || !matches!(
                        effect["dimension"].as_str(),
                        Some("world" | "knowledge" | "relationship" | "narrative")
                    )
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "plan_contract_tests.rs"]
mod tests;
