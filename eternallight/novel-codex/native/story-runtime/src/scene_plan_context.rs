use novel_codex_story_core::{ProjectId, RepositoryError, RevisionId, SqliteStoryRepository};
use serde_json::{Value, json};

pub(crate) struct ScenePlanLinks {
    pub links: Vec<Value>,
    pub scene_limit_reached: bool,
    pub event_limit_reached: bool,
}

/// Supplies source-bound plan and draft references for rolling planning. These are
/// comparison inputs, not claims that a planned effect was realized.
pub(crate) fn scene_plan_draft_links(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    drafts: &[Value],
    plan_from_chapter: i64,
    working_knowledge: Option<&Value>,
) -> Result<ScenePlanLinks, RepositoryError> {
    let mut links = Vec::new();
    let mut event_limit_reached = false;
    let knowledge_entries = working_knowledge.and_then(|knowledge| knowledge["entries"].as_array());
    let bindings = repository.chapter_bindings(project_id)?;
    for draft in drafts.iter().rev() {
        let content = &draft["content"];
        let chapter_number = content["chapterNumber"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?;
        if chapter_number >= plan_from_chapter || content["sourcePlanRefs"]["localOnly"] != false {
            continue;
        }
        let source = &content["sourcePlanRefs"];
        let plan_revision_id = source["planRevisionId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let chapter_plan_id = source["chapterPlanId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let chapter_id = content["chapterId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let assessment = repository.scene_assessment(project_id, chapter_id)?;
        let current_assessment = assessment.as_ref().filter(|assessment| {
            assessment.current
                && assessment.content["planRevisionId"] == plan_revision_id
                && assessment.content["draftRevisionId"] == draft["sourceRevisionId"]
        });
        if !bindings.iter().any(|binding| {
            binding.chapter_id == chapter_id
                && binding.chapter_number == chapter_number
                && binding.chapter_plan_id == chapter_plan_id
                && binding.plan_revision_id == plan_revision_id
        }) {
            return Err(RepositoryError::CorruptState);
        }
        let revision_id = RevisionId::parse(plan_revision_id.to_owned())
            .map_err(|_| RepositoryError::CorruptState)?;
        let plan_json = repository
            .revision_content(&revision_id)?
            .ok_or(RepositoryError::CorruptState)?;
        let plan: Value =
            serde_json::from_str(&plan_json).map_err(|_| RepositoryError::CorruptState)?;
        let chapters = plan
            .get("payload")
            .unwrap_or(&plan)
            .get("chapters")
            .and_then(Value::as_array)
            .ok_or(RepositoryError::CorruptState)?;
        let chapter = chapters
            .iter()
            .find(|chapter| {
                chapter["chapterPlanId"] == chapter_plan_id
                    && chapter["chapterNumber"] == chapter_number
            })
            .ok_or(RepositoryError::CorruptState)?;
        let Some(scenes) = chapter["scenes"].as_array() else {
            continue;
        };
        let body = content["body"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let groups = content["sceneGroups"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        let mut offset = body.chars().count() - body.trim_start().chars().count();
        let group_ranges = groups
            .iter()
            .map(|group| {
                let text = group["body"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?;
                let start = offset;
                let end = start + text.trim().chars().count();
                offset = end + "\n\n---\n\n".chars().count();
                Ok((group, start, end))
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        for scene in scenes {
            if links.len() == 24 {
                return Ok(ScenePlanLinks {
                    links,
                    scene_limit_reached: true,
                    event_limit_reached,
                });
            }
            let scene_id = scene["id"].as_str().ok_or(RepositoryError::CorruptState)?;
            let scene_assessment = current_assessment.and_then(|assessment| {
                assessment.content["assessments"]
                    .as_array()?
                    .iter()
                    .find(|item| item["sceneId"] == scene_id)
                    .map(|item| {
                        json!({
                            "revisionId": assessment.revision_id,
                            "verdict": item["verdict"],
                            "recommendation": item["recommendation"],
                            "rationale": item["rationale"],
                        "effectAssessments": item["effectAssessments"],
                            "authority": "candidateAnalysis"
                        })
                    })
            });
            let matching_groups = group_ranges
                .iter()
                .filter(|(group, _, _)| group["sceneId"] == scene_id)
                .collect::<Vec<_>>();
            let actual_group_ids = matching_groups
                .iter()
                .filter_map(|(group, _, _)| group["id"].as_str())
                .collect::<Vec<_>>();
            let mut actual_event_refs = Vec::new();
            for entry in knowledge_entries.into_iter().flatten() {
                if entry["sourceRevisionId"] != draft["sourceRevisionId"]
                    || entry["assertion"]["kind"] != "event"
                {
                    continue;
                }
                let start = entry["assertion"]["evidence"]["start"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok());
                let end = entry["assertion"]["evidence"]["end"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok());
                if !start.zip(end).is_some_and(|(start, end)| {
                    matching_groups.iter().any(|(_, group_start, group_end)| {
                        start >= *group_start && end <= *group_end
                    })
                }) {
                    continue;
                }
                if actual_event_refs.len() == 12 {
                    event_limit_reached = true;
                    break;
                }
                actual_event_refs.push(json!({
                    "modelingBatchId": entry["modelingBatchId"],
                    "assertionIndex": entry["assertionIndex"],
                    "sourceRevisionId": entry["sourceRevisionId"],
                    "predicate": entry["assertion"]["predicate"],
                    "evidence": entry["assertion"]["evidence"],
                    "confirmationState": entry["confirmationState"]
                }));
            }
            links.push(json!({
                "chapterNumber": chapter_number,
                "planRevisionId": plan_revision_id,
                "chapterPlanId": chapter_plan_id,
                "draftRevisionId": draft["sourceRevisionId"],
                "sceneId": scene_id,
                "plannedGoal": scene["goal"],
                "plannedConflict": scene.get("conflict"),
                "plannedExpectedEffects": scene["expectedEffects"],
                "plannedBeats": scene["beats"].as_array().map(|beats| beats.iter().map(|beat| json!({"id": beat["id"], "description": beat["description"]})).collect::<Vec<_>>()).unwrap_or_default(),
                "actualSceneGroupIds": actual_group_ids,
                "actualEventRefs": actual_event_refs,
                "eventEvidenceScope": "projectedKnowledgeOnly",
                "assessmentStatus": if scene_assessment.is_some() { "assessed" } else if assessment.is_some() { "stale" } else { "notAssessed" },
                "sceneOutcomeAssessment": scene_assessment
            }));
        }
    }
    Ok(ScenePlanLinks {
        links,
        scene_limit_reached: false,
        event_limit_reached,
    })
}
