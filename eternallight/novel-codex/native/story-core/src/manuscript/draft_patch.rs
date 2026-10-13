//! Exact edits against an immutable, task-authorized chapter baseline.
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Patch {
    chapter_id: String,
    source_revision_id: String,
    edits: Vec<Edit>,
}
#[derive(Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Edit {
    group_id: String,
    start: usize,
    end: usize,
    old_text: String,
    new_text: String,
}
fn invalid(
    index: Option<usize>,
    constraint: &'static str,
    reason: &'static str,
) -> RepositoryError {
    RepositoryError::PayloadValidation {
        code: "draft_patch_invalid",
        instance_path: match (index, constraint) {
            (Some(i), "old_text_mismatch") => format!("/patch/edits/{i}/oldText"),
            (Some(i), "group_locked" | "group_not_unique" | "outside_visible_scope") => {
                format!("/patch/edits/{i}/groupId")
            }
            (Some(i), _) => format!("/patch/edits/{i}"),
            (None, "source_cas_mismatch") => "/patch/sourceRevisionId".into(),
            (None, _) => "/patch".into(),
        },
        details: json!({"constraint":constraint,"expected":reason}),
    }
}

impl SqliteStoryRepository {
    

    fn draft_patch_payload(
        &self,
        project: &ProjectId,
        task_id: &str,
        expected_candidate: Option<&str>,
        patch: &Value,
    ) -> Result<Value, RepositoryError> {
        let patch: Patch = serde_json::from_value(patch.clone()).map_err(|_| {
            invalid(
                None,
                "invalid_shape",
                "sourceRevisionId and exact edits are required",
            )
        })?;
        let task = crate::tasks::load_task(&self.connection, task_id)?;
        if task.project_id != *project || task.operation != "generateChapter" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        // expected_task_target advances to this task's latest output after saving.
        // A patch replay must instead expand from the originally captured baseline.
        let manifest: Value = serde_json::from_str(&task.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let object = crate::ObjectId::parse(
            manifest["outputObjectId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?
                .to_owned(),
        )
        .map_err(|_| RepositoryError::CorruptState)?;
        let candidate = manifest["expectedCandidateRevisionId"]
            .as_str()
            .map(|s| crate::RevisionId::parse(s.to_owned()))
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?;
        let source = crate::workspace_selection::task_output_working_base(
            &self.connection,
            task_id,
            object.as_str(),
            candidate.as_ref(),
        )?;
        let original = source
            .as_ref()
            .is_some_and(|source| source.as_str() == patch.source_revision_id)
            && expected_candidate == candidate.as_ref().map(crate::RevisionId::as_str);
        let own_checkpoint = expected_candidate==Some(patch.source_revision_id.as_str()) && self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id WHERE o.task_id=?1 AND o.revision_id=?2 AND r.project_id=?3 AND r.object_id=?4)",
            params![task_id,patch.source_revision_id,project.as_str(),object.as_str()],|row|row.get::<_,bool>(0)
        ).map_err(|e|RepositoryError::Storage(e.to_string()))?;
        if !original && !own_checkpoint {
            return Err(invalid(
                None,
                "source_cas_mismatch",
                "sourceRevisionId and candidate CAS must identify the frozen baseline or this task's exact saved checkpoint",
            ));
        }
        let content:String=self.connection.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",params![project.as_str(),object.as_str(),patch.source_revision_id],|row|row.get(0)).map_err(|e|RepositoryError::Storage(e.to_string()))?;
        let mut payload: Value =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        if payload["chapterId"] != patch.chapter_id {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        apply(
            &mut payload,
            &patch,
            crate::task_scope::writer_scene_limit(&target)?,
        )?;
        // Preserve every field of the immutable baseline; save_candidate enforces
        // selectedScenes/bodyRanges and locked metadata again inside its transaction.
        Ok(payload)
    }
}

fn apply(
    payload: &mut Value,
    patch: &Patch,
    scene_limit: Option<usize>,
) -> Result<(), RepositoryError> {
    if patch.edits.is_empty() || patch.edits.len() > 100 {
        return Err(invalid(
            None,
            "edit_count",
            "provide 1 through 100 exact edits",
        ));
    }
    let groups = payload["sceneGroups"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?;
    let mut edits = Vec::new();
    for (index, edit) in patch.edits.iter().enumerate() {
        let matching: Vec<_> = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g["id"].as_str() == Some(edit.group_id.as_str()))
            .map(|(i, _)| i)
            .collect();
        if matching.len() != 1 {
            return Err(invalid(
                Some(index),
                "group_not_unique",
                "groupId must identify exactly one original scene group",
            ));
        }
        let group = matching[0];
        if groups[group]["locked"] != false {
            return Err(invalid(
                Some(index),
                "group_locked",
                "Locked prose must remain unchanged",
            ));
        }
        if scene_limit.is_some_and(|limit| group >= limit) {
            return Err(invalid(
                Some(index),
                "outside_visible_scope",
                "The edited group must be inside the task's visible scope",
            ));
        }
        let original = groups[group]["body"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let offsets: Vec<_> = original
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(original.len()))
            .collect();
        if edit.start > edit.end || edit.end >= offsets.len() {
            return Err(invalid(
                Some(index),
                "range_out_of_bounds",
                "start/end must be valid Unicode scalar offsets [start,end) in the original group body",
            ));
        }
        let (start, end) = (offsets[edit.start], offsets[edit.end]);
        if original[start..end] != edit.old_text {
            return Err(invalid(
                Some(index),
                "old_text_mismatch",
                "oldText must exactly match the selected original range",
            ));
        }
        edits.push((group, start, end, index));
    }
    edits.sort_unstable();
    for pair in edits.windows(2) {
        let (g, a, b, _) = pair[0];
        let (next, c, _, index) = pair[1];
        if g == next && (c < b || c == a) {
            return Err(invalid(
                Some(index),
                "overlapping_edits",
                "edits must not overlap or share an insertion position",
            ));
        }
    }
    for (group, start, end, index) in edits.into_iter().rev() {
        let mut body = groups[group]["body"].as_str().unwrap().to_owned();
        body.replace_range(start..end, &patch.edits[index].new_text);
        groups[group]["body"] = body.into();
    }
    let body =
        crate::scene_body::canonical_scene_body(groups.iter().map(|g| g["body"].as_str().unwrap()));
    if body.chars().count() > 120_000 {
        return Err(invalid(
            None,
            "chapter_too_large",
            "resulting chapter must not exceed 120000 characters",
        ));
    }
    payload["body"] = body.into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Value {
        json!({"chapterId":"c","title":"原题","sceneGroups":[{"id":"g","body":"风🌙风月","locked":false,"sceneId":"scene"},{"id":"locked","body":"原文","locked":true}],"sourcePlanRefs":{"unchanged":true}})
    }
    fn patch(edits: Value) -> Patch {
        serde_json::from_value(json!({"chapterId":"c","sourceRevisionId":"r","edits":edits}))
            .unwrap()
    }
    #[test]
    fn unicode_exact_ranges_preserve_metadata_and_duplicate_text() {
        let mut v = base();
        let original = v.clone();
        apply(&mut v,&patch(json!([{"groupId":"g","start":2,"end":3,"oldText":"风","newText":"雨"},{"groupId":"g","start":4,"end":4,"oldText":"","newText":"。"}])),None).unwrap();
        assert_eq!(v["sceneGroups"][0]["body"], "风🌙雨月。");
        assert_eq!(v["sceneGroups"][1], original["sceneGroups"][1]);
        assert_eq!(v["sourcePlanRefs"], original["sourcePlanRefs"]);
        assert_eq!(v["sceneGroups"][0]["sceneId"], "scene");
    }
    #[test]
    fn invalid_anchors_overlap_and_hidden_or_locked_groups_do_not_apply() {
        for edits in [
            json!([{"groupId":"g","start":1,"end":2,"oldText":"风","newText":"雨"}]),
            json!([{"groupId":"g","start":0,"end":2,"oldText":"风🌙","newText":"雨"},{"groupId":"g","start":1,"end":3,"oldText":"🌙风","newText":"夜"}]),
            json!([{"groupId":"g","start":9,"end":10,"oldText":"","newText":"雨"}]),
            json!([{"groupId":"locked","start":0,"end":1,"oldText":"原","newText":"改"}]),
        ] {
            let mut v = base();
            assert!(apply(&mut v, &patch(edits), None).is_err());
            assert_eq!(v, base());
        }
        let mut v = base();
        assert!(
            apply(
                &mut v,
                &patch(json!([{"groupId":"g","start":0,"end":1,"oldText":"风","newText":"雨"}])),
                Some(0)
            )
            .is_err()
        );
    }
}

/// Public draft patch capability; SQLite retains transaction ownership.
pub trait DraftPatchRepository {
    /// No write occurs here. save_candidate remains the transaction/CAS/permission
    /// authority. Loading the frozen baseline (not the current head) preserves replay.
    fn expand_draft_patch(
        &mut self,
        project: &ProjectId,
        task_id: &str,
        expected_candidate: Option<&str>,
        patch: &Value,
    ) -> Result<Value, RepositoryError>;
}

impl DraftPatchRepository for SqliteStoryRepository {
    /// No write occurs here. save_candidate remains the transaction/CAS/permission
    /// authority. Loading the frozen baseline (not the current head) preserves replay.
    fn expand_draft_patch(
        &mut self,
        project: &ProjectId,
        task_id: &str,
        expected_candidate: Option<&str>,
        patch: &Value,
    ) -> Result<Value, RepositoryError> {
        let task = crate::tasks::load_task(&self.connection, task_id)?;
        if task.project_id != *project {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let outcome = self.draft_patch_payload(project, task_id, expected_candidate, patch);
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            &task,
            "save_draft_checkpoint",
            &outcome,
        )?;
        outcome
    }
}
