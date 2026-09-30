use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

pub struct StoredSceneAssessment {
    pub revision_id: String,
    pub content: Value,
    pub current: bool,
}

impl SqliteStoryRepository {
    pub fn scene_assessment(
        &self,
        project_id: &ProjectId,
        chapter_id: &str,
    ) -> Result<Option<StoredSceneAssessment>, RepositoryError> {
        let read = || {
            let heads = self.object_heads(project_id)?;
            let Some(head) = heads.iter().find(|head| {
                head.object_id.as_str() == format!("scene-assessment-{chapter_id}")
                    && head.object_kind == "sceneAssessment"
            }) else {
                return Ok(None);
            };
            let revision_id = head
                .candidate_revision_id
                .as_ref()
                .or(head.accepted_revision_id.as_ref())
                .ok_or(RepositoryError::CorruptState)?;
            let content_json = self
                .revision_content(revision_id)?
                .ok_or(RepositoryError::CorruptState)?;
            let content: Value =
                serde_json::from_str(&content_json).map_err(|_| RepositoryError::CorruptState)?;
            let draft_revision = heads
                .iter()
                .find(|head| head.object_id.as_str() == format!("draft-{chapter_id}"))
                .and_then(|head| {
                    head.candidate_revision_id
                        .as_ref()
                        .or(head.accepted_revision_id.as_ref())
                });
            let knowledge = self.chapter_knowledge(project_id, chapter_id)?;
            let draft_json = draft_revision
                .map(|id| self.revision_content(id))
                .transpose()?
                .flatten();
            let draft_content: Option<Value> = draft_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| RepositoryError::CorruptState)?;
            let binding = self
                .chapter_bindings(project_id)?
                .into_iter()
                .find(|binding| binding.chapter_id == chapter_id);
            let current = content["chapterId"] == chapter_id
                && draft_revision.is_some_and(|id| content["draftRevisionId"] == id.as_str())
                && knowledge["status"] == "ready"
                && knowledge["batchId"] == content["knowledgeBatchId"]
                && binding.as_ref().is_some_and(|binding| {
                    content["planRevisionId"] == binding.plan_revision_id
                        && draft_content.as_ref().is_some_and(|draft| {
                            draft["chapterNumber"] == binding.chapter_number
                                && draft["sourcePlanRefs"]["planRevisionId"]
                                    == binding.plan_revision_id
                                && draft["sourcePlanRefs"]["chapterPlanId"]
                                    == binding.chapter_plan_id
                        })
                })
                && if let Some(binding) = binding.as_ref() {
                    crate::commitment_assessment::inputs_current(
                        &self.connection,
                        project_id,
                        binding.chapter_number,
                        &content,
                    )?
                } else {
                    false
                };
            Ok(Some(StoredSceneAssessment {
                revision_id: revision_id.as_str().to_owned(),
                content,
                current,
            }))
        };
        if self.connection.is_autocommit() {
            self.with_read_snapshot(read)?
        } else {
            read()
        }
    }
}

/// An assessment is a task-bound interpretation of a frozen plan, draft, and
/// modeling batch. It is never a Canon fact or a substitute for author review.
pub(crate) fn validate_scene_assessment_write(
    connection: &Connection,
    task: &TaskRecord,
    content_json: &str,
) -> Result<(), RepositoryError> {
    let invalid = || RepositoryError::TaskOutputForbidden;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let output: Value = serde_json::from_str(content_json).map_err(|_| invalid())?;
    let chapter_id = target["chapterId"].as_str().ok_or_else(invalid)?;
    let plan_revision_id = target["planRevisionId"].as_str().ok_or_else(invalid)?;
    let draft_revision_id = target["draftRevisionId"].as_str().ok_or_else(invalid)?;
    let batch_id = target["knowledgeBatchId"].as_str().ok_or_else(invalid)?;
    let input_snapshot_id = target["inputSnapshotId"].as_str().ok_or_else(invalid)?;
    let intent_generation = target["intentGeneration"]
        .as_i64()
        .filter(|generation| *generation >= 0)
        .ok_or_else(invalid)?;
    if output["chapterId"] != chapter_id
        || output["planRevisionId"] != plan_revision_id
        || output["draftRevisionId"] != draft_revision_id
        || output["knowledgeBatchId"] != batch_id
        || output["inputSnapshotId"] != input_snapshot_id
        || output["intentGeneration"] != intent_generation
    {
        return Err(invalid());
    }
    let current_snapshot: Option<String> = connection
        .query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = 'main'",
            [task.project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if current_snapshot.as_deref() != Some(input_snapshot_id) {
        return Err(RepositoryError::TaskInputStale);
    }
    let binding: Option<(String, i64)> = connection
        .query_row(
            "SELECT b.chapter_plan_id, c.number FROM story_chapter_plan_bindings_v2 b JOIN story_chapters_v2 c ON c.chapter_id = b.chapter_id AND c.project_id = b.project_id WHERE b.project_id = ?1 AND b.chapter_id = ?2 AND b.plan_revision_id = ?3",
            params![task.project_id.as_str(), chapter_id, plan_revision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let (chapter_plan_id, chapter_number) = binding.ok_or_else(invalid)?;
    let draft_object_id = format!("draft-{chapter_id}");
    let current_draft: Option<String> = connection
        .query_row(
            "SELECT COALESCE(candidate_revision_id, accepted_revision_id) FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2 AND object_kind = 'draft'",
            params![task.project_id.as_str(), draft_object_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .flatten();
    if current_draft.as_deref() != Some(draft_revision_id) {
        return Err(RepositoryError::TaskInputStale);
    }
    let draft_json: Option<String> = connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
            params![task.project_id.as_str(), draft_object_id, draft_revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let draft: Value = serde_json::from_str(&draft_json.ok_or_else(invalid)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    if draft["chapterId"] != chapter_id
        || draft["chapterNumber"] != chapter_number
        || draft["sourcePlanRefs"]["planRevisionId"] != plan_revision_id
        || draft["sourcePlanRefs"]["chapterPlanId"] != chapter_plan_id
    {
        return Err(invalid());
    }
    let plan_json: Option<String> = connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = 'artifact-plan' AND revision_id = ?2",
            params![task.project_id.as_str(), plan_revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let plan: Value = serde_json::from_str(&plan_json.ok_or_else(invalid)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    let chapters = plan
        .get("payload")
        .unwrap_or(&plan)
        .get("chapters")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let chapter = chapters
        .iter()
        .find(|chapter| {
            chapter["chapterPlanId"] == chapter_plan_id
                && chapter["chapterNumber"] == chapter_number
        })
        .ok_or_else(invalid)?;
    let scenes = chapter["scenes"].as_array().ok_or_else(invalid)?;
    if scenes.is_empty() || scenes.len() > 24 {
        return Err(invalid());
    }
    let mut planned_scene_ids = HashSet::new();
    for scene in scenes {
        let scene_id = scene["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(invalid)?;
        if !planned_scene_ids.insert(scene_id) {
            return Err(invalid());
        }
    }
    let assertions_json: Option<String> = connection
        .query_row(
            "SELECT b.assertions_json FROM story_modeling_batches_v2 b JOIN story_knowledge_heads_v2 h ON h.project_id = b.project_id AND h.object_id = b.object_id AND h.batch_id = b.batch_id WHERE b.project_id = ?1 AND b.object_id = ?2 AND b.source_revision_id = ?3 AND b.batch_id = ?4 AND b.status = 'ready'",
            params![task.project_id.as_str(), draft_object_id, draft_revision_id, batch_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let assertions: Vec<Value> = serde_json::from_str(&assertions_json.ok_or_else(invalid)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    let body = draft["body"].as_str().ok_or_else(invalid)?;
    let groups = draft["sceneGroups"].as_array().ok_or_else(invalid)?;
    let joined = groups
        .iter()
        .map(|group| group["body"].as_str().map(str::trim))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(invalid)?
        .join("\n\n---\n\n");
    if body.trim() != joined {
        return Err(invalid());
    }
    let mut cursor = body.chars().count() - body.trim_start().chars().count();
    let mut scene_ranges: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
    for group in groups {
        let text = group["body"].as_str().ok_or_else(invalid)?;
        let end = cursor + text.trim().chars().count();
        if let Some(scene_id) = group["sceneId"].as_str() {
            scene_ranges
                .entry(scene_id)
                .or_default()
                .push((cursor, end));
        }
        cursor = end + "\n\n---\n\n".chars().count();
    }
    let assessments = output["assessments"].as_array().ok_or_else(invalid)?;
    if assessments.len() != scenes.len() {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    for assessment in assessments {
        let scene_id = assessment["sceneId"].as_str().ok_or_else(invalid)?;
        if !planned_scene_ids.contains(scene_id) || !seen.insert(scene_id) {
            return Err(invalid());
        }
        let planned = scenes
            .iter()
            .find(|scene| scene["id"] == scene_id)
            .ok_or_else(invalid)?;
        let effects = planned["expectedEffects"].as_array().ok_or_else(invalid)?;
        let evaluated = assessment["effectAssessments"]
            .as_array()
            .ok_or_else(invalid)?;
        if effects.is_empty()
            || effects.len() > 12
            || evaluated.len() != effects.len()
            || assessment.get("actualEventRefs").is_some()
        {
            return Err(invalid());
        }
        let verdict = assessment["verdict"].as_str().ok_or_else(invalid)?;
        if !matches!(verdict, "realized" | "deviated" | "insufficientEvidence")
            || !matches!(
                assessment["recommendation"].as_str(),
                Some("continue" | "repairDraft" | "replan" | "askAuthor")
            )
            || !assessment["rationale"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty() && text.chars().count() <= 1000)
        {
            return Err(invalid());
        }
        let mut evaluated_ids = HashSet::new();
        let mut has_deviation = false;
        let mut has_insufficient = false;
        for effect in evaluated {
            let effect_id = effect["effectId"].as_str().ok_or_else(invalid)?;
            let planned_effect = effects
                .iter()
                .find(|planned| planned["id"] == effect_id)
                .ok_or_else(invalid)?;
            if !evaluated_ids.insert(effect_id)
                || !effect["rationale"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty() && text.chars().count() <= 1000)
            {
                return Err(invalid());
            }
            let effect_verdict = effect["verdict"].as_str().ok_or_else(invalid)?;
            match effect_verdict {
                "realized" => {}
                "deviated" => has_deviation = true,
                "insufficientEvidence" => has_insufficient = true,
                _ => return Err(invalid()),
            }
            let refs = effect["actualEventRefs"].as_array().ok_or_else(invalid)?;
            if refs.len() > 12 || (effect_verdict == "realized" && refs.is_empty()) {
                return Err(invalid());
            }
            let mut seen_refs = HashSet::new();
            for reference in refs {
                let index = reference["assertionIndex"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(invalid)?;
                if reference["modelingBatchId"] != batch_id || !seen_refs.insert(index) {
                    return Err(invalid());
                }
                let assertion = assertions.get(index).ok_or_else(invalid)?;
                let start = assertion["evidence"]["start"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(invalid)?;
                let end = assertion["evidence"]["end"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(invalid)?;
                if assertion["kind"] != "event"
                    || start >= end
                    || !scene_ranges.get(scene_id).is_some_and(|ranges| {
                        ranges.iter().any(|(lo, hi)| start >= *lo && end <= *hi)
                    })
                {
                    return Err(invalid());
                }
            }
            let (deviation, insufficient) = crate::commitment_assessment::validate(
                connection,
                &task.project_id,
                chapter_number,
                planned_effect,
                effect,
            )?;
            has_deviation |= deviation;
            has_insufficient |= insufficient;
        }
        let derived_verdict = if has_deviation {
            "deviated"
        } else if has_insufficient {
            "insufficientEvidence"
        } else {
            "realized"
        };
        if verdict != derived_verdict {
            return Err(invalid());
        }
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "scene_assessment_tests.rs"]
mod tests;
