//! Agent-proposed work stays inside an existing immutable author-authorized envelope.
use crate::authoring_runs::storage_error;
use crate::work_decision_status::{ReadyAction, action};
use crate::{ProjectId, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkActionProposal {
    pub operation: String,
    pub target: ProposalTarget,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalTarget {
    pub chapter_id: Option<String>,
    pub source_task_id: Option<String>,
    pub scope: Option<Value>,
}
fn denied() -> RepositoryError {
    RepositoryError::TaskOutputForbidden
}

pub(crate) fn key(proposal: &WorkActionProposal) -> Result<String, RepositoryError> {
    if proposal.operation.len() > 64
        || [&proposal.target.chapter_id, &proposal.target.source_task_id]
            .into_iter()
            .flatten()
            .any(|id| id.trim().is_empty() || id.len() > 160)
        || proposal
            .target
            .scope
            .as_ref()
            .is_some_and(|s| s.to_string().len() > 4096)
    {
        return Err(denied());
    }
    Ok(action(
        "proposal",
        serde_json::to_value(proposal).map_err(|_| denied())?,
        None,
        None,
        false,
    )
    .key)
}

fn anchor(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    proposal: &WorkActionProposal,
) -> Result<(String, Value), RepositoryError> {
    let row: Option<(String,String)> = db.query_row(
        "SELECT t.task_id,t.target_json FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.operation=?3 AND (?4 IS NULL OR t.task_id=?4) AND (?5 IS NULL OR json_extract(t.target_json,'$.chapterId')=?5) ORDER BY s.ordinal LIMIT 1",
        params![work,project.as_str(),proposal.operation,proposal.target.source_task_id,proposal.target.chapter_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    let (task, encoded) = row.ok_or_else(denied)?;
    let record = crate::tasks::load_task(db, &task)?;
    let manifest: Value = serde_json::from_str(&record.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let original: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    if let (Some(source), Some(revision)) = (
        manifest["authorIntentBinding"]["sourceTaskId"].as_str(),
        manifest["authorIntentBinding"]["intentRevision"].as_i64(),
    ) {
        crate::task_intents::assert_action_intent(db, source, revision)?;
    } else {
        crate::task_intents::assert_action_intent(db, &task, 0)?;
    }
    if let (Some(source), Some(action)) = (
        original["requestedByTaskId"].as_str(),
        original["requestedActionId"].as_str(),
    ) {
        let action = crate::task_actions::load_action(db, project, source, action)?;
        crate::execution_scope_storage::assert_current(db, project, &action)?;
    }
    let target = crate::work_decision_recovery::failure_target(
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?,
    )?;
    Ok((task, target))
}

fn chapter_source(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
    writing: bool,
) -> Result<(i64, String), RepositoryError> {
    let (number, revision, locked): (i64,String,bool) = db.query_row("SELECT c.number,COALESCE(h.candidate_revision_id,h.accepted_revision_id),h.locked FROM story_chapters_v2 c JOIN story_object_heads_v2 h ON h.project_id=c.project_id AND h.object_id='draft-'||c.chapter_id WHERE c.project_id=?1 AND c.chapter_id=?2 AND COALESCE(h.candidate_revision_id,h.accepted_revision_id) IS NOT NULL",params![project.as_str(),chapter],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)?.ok_or_else(denied)?;
    if writing && locked {
        return Err(RepositoryError::ObjectLocked);
    }
    Ok((number, revision))
}

pub(crate) fn validate_artifact(
    db: &Connection,
    project: &ProjectId,
    operation: &str,
    target: &mut Value,
) -> Result<(), RepositoryError> {
    let object = match operation {
        "generateBrief" => "artifact-brief".to_owned(),
        "generateOutline" => "artifact-outline".to_owned(),
        "generatePlan" => "artifact-plan".to_owned(),
        "generatePropertyRules" => "artifact-propertyRules".to_owned(),
        "reviseCommitment" => format!(
            "commitment-{}",
            target["commitmentId"].as_str().ok_or_else(denied)?
        ),
        _ => return Err(denied()),
    };
    let current: Option<(Option<String>,bool)> = db.query_row("SELECT COALESCE(candidate_revision_id,accepted_revision_id),locked FROM story_object_heads_v2 WHERE project_id=?1 AND object_id=?2",params![project.as_str(),object],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
    if current.as_ref().is_some_and(|(_, locked)| *locked) {
        return Err(RepositoryError::ObjectLocked);
    }
    if operation == "reviseCommitment" {
        let revision = current
            .and_then(|(revision, _)| revision)
            .ok_or(RepositoryError::TaskInputStale)?;
        target["revisionId"] = json!(revision);
        crate::commitment_revision::validate_target(db, project, target)?;
    }
    Ok(())
}

fn narrow_scope(
    target: &mut Value,
    proposed: Option<&Value>,
    revision: &str,
) -> Result<(), RepositoryError> {
    let original = target["scope"].clone();
    if let Some(scope) = proposed {
        let kind = scope["type"].as_str().ok_or_else(denied)?;
        let original_kind = original["type"].as_str().unwrap_or("wholeChapter");
        let permitted = match (original_kind, kind) {
            ("wholeChapter", "wholeChapter" | "selectedScenes" | "throughScene") => true,
            ("selectedScenes", "selectedScenes") => {
                let groups_allowed = scope["groupIds"].as_array().is_some_and(|ids| {
                    original["groupIds"]
                        .as_array()
                        .is_some_and(|allowed| ids.iter().all(|id| allowed.contains(id)))
                });
                groups_allowed && original.get("bodyRanges").is_none_or(|ranges| {
                    scope["bodyRanges"].as_array().is_some_and(|next| next.iter().all(|r| {
                        ranges.as_array().is_some_and(|old| old.iter().any(|p| p["groupId"] == r["groupId"]
                            && r["start"].as_u64().zip(p["start"].as_u64()).is_some_and(|(a,b)| a >= b)
                            && r["end"].as_u64().zip(p["end"].as_u64()).is_some_and(|(a,b)| a <= b)))
                    }))
                })
            }
            ("throughScene", "throughScene") => scope["endSceneOrder"]
                .as_u64()
                .zip(original["endSceneOrder"].as_u64())
                .is_some_and(|(next, prior)| next <= prior),
            _ => false,
        };
        if !permitted {
            return Err(denied());
        }
        let allowed: &[&str] = match kind {
            "wholeChapter" => &["type"],
            "throughScene" => &["type", "endSceneOrder"],
            "selectedScenes" => &["type", "groupIds", "sourceRevisionId", "bodyRanges"],
            _ => return Err(denied()),
        };
        if scope
            .as_object()
            .is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str())))
        {
            return Err(denied());
        }
        if scope
            .get("sourceRevisionId")
            .is_some_and(|id| id != revision)
        {
            return Err(RepositoryError::TaskInputStale);
        }
        if scope.get("bodyRanges").is_some() && scope["sourceRevisionId"] != revision {
            return Err(RepositoryError::TaskInputStale);
        }
        target["scope"] = scope.clone();
    }
    if target["scope"]["type"] == "selectedScenes" {
        target["scope"]["sourceRevisionId"] = json!(revision);
    }
    if target["scope"]["type"] == "throughScene" {
        target["sceneOrder"] = target["scope"]["endSceneOrder"].clone();
    }
    crate::task_scope::writer_scene_limit(target)?;
    crate::revision_outcome::desired_outcomes(target)?;
    Ok(())
}

pub(crate) fn derive(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    proposal: &WorkActionProposal,
    ready: &[ReadyAction],
) -> Result<ReadyAction, RepositoryError> {
    key(proposal)?;
    let generation: Option<String> = db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND 'work-'||run_id=?2",params![project.as_str(),work],|r|r.get(0)).optional().map_err(storage_error)?;
    let generation: Option<Value> = generation
        .map(|s| serde_json::from_str(&s).map_err(|_| RepositoryError::CorruptState))
        .transpose()?;
    let mut result = match proposal.operation.as_str() {
        "generateChapter" => {
            let chapter = proposal.target.chapter_id.as_deref().ok_or_else(denied)?;
            let (number, revision) = chapter_source(db, project, chapter, true)?;
            let (source, mut target) = if let Some(state) = &generation {
                if state["status"] != "running"
                    || state["currentChapterId"] != chapter
                    || proposal.target.source_task_id.is_some()
                {
                    return Err(denied());
                }
                if proposal.target.scope.as_ref().is_some_and(|scope| {
                    !matches!(
                        scope["type"].as_str(),
                        Some("wholeChapter" | "selectedScenes")
                    )
                }) {
                    return Err(denied());
                }
                let plan =
                    crate::bound_writer_plan::bound_chapter_plan_for_chapter(db, project, chapter)?;
                (
                    None,
                    json!({"type":"chapter","chapterId":chapter,"chapterNumber":number,"readingChapter":number,"sceneOrder":1,"scope":{"type":"wholeChapter"},"povCharacterId":plan.as_ref().and_then(|p|p.get("povCharacterId")),"storyTime":plan.as_ref().and_then(|p|p.get("storyTime"))}),
                )
            } else {
                let (source, target) = anchor(db, project, work, proposal)?;
                (Some(source), target)
            };
            if !target["chapterNumber"].is_null() && target["chapterNumber"] != number {
                return Err(RepositoryError::TaskInputStale);
            }
            target["chapterNumber"] = json!(number);
            crate::scene_revision::rebase_ranges(db, project, &mut target, &revision)?;
            narrow_scope(&mut target, proposal.target.scope.as_ref(), &revision)?;
            crate::scene_revision::validate_target(db, project, &target)?;
            action("generateChapter", target, source, Some(revision), false)
        }
        "generateBrief"
        | "generateOutline"
        | "generatePlan"
        | "generatePropertyRules"
        | "reviseCommitment" => {
            if proposal.target.chapter_id.is_some()
                || proposal.target.scope.is_some()
                || proposal.target.source_task_id.is_none()
            {
                return Err(denied());
            }
            if generation.is_some() {
                return Err(denied());
            }
            let permitted: Vec<_> = ready
                .iter()
                .filter(|candidate| {
                    candidate.author_action_id.is_some()
                        && candidate.operation == proposal.operation
                        && candidate.source_task == proposal.target.source_task_id
                })
                .collect();
            if permitted.len() > 1 {
                return Err(RepositoryError::InvalidContent("Choose the exact approved planning actionKey when this source has multiple targets".into()));
            }
            let mut candidate = if let Some(approved) = permitted.first() {
                let mut candidate = action(
                    &approved.operation,
                    approved.target.clone(),
                    approved.source_task.clone(),
                    None,
                    true,
                );
                candidate.author_action_id = approved.author_action_id.clone();
                candidate
            } else {
                let (source, target) = anchor(db, project, work, proposal)?;
                action(&proposal.operation, target, Some(source), None, false)
            };
            validate_artifact(db, project, &proposal.operation, &mut candidate.target)?;
            candidate
        }
        "modelKnowledge" | "extractCommitments" => {
            if proposal.target.scope.is_some() || proposal.target.source_task_id.is_some() {
                return Err(denied());
            }
            let chapter = proposal.target.chapter_id.as_deref().ok_or_else(denied)?;
            if generation.as_ref().is_some_and(|state| {
                state["currentChapterId"] != chapter || state["status"] != "running"
            }) {
                return Err(denied());
            }
            let (number, revision) = chapter_source(db, project, chapter, false)?;
            let target = json!({"type":if proposal.operation=="modelKnowledge" {"knowledge"}else{"commitmentExtraction"},"chapterId":chapter,"chapterNumber":number,"sourceRevisionId":revision});
            crate::commitment_extraction::validate_target(db, project, &target)?;
            action(&proposal.operation, target, None, Some(revision), false)
        }
        "assessSceneOutcome" | "assessRevisionOutcome" => {
            if proposal.target.scope.is_some() || proposal.target.source_task_id.is_some() {
                return Err(denied());
            }
            let chapter = proposal.target.chapter_id.as_deref().ok_or_else(denied)?;
            let current = ready
                .iter()
                .find(|a| a.operation == proposal.operation && a.target["chapterId"] == chapter)
                .ok_or_else(denied)?;
            action(
                &current.operation,
                current.target.clone(),
                current.source_task.clone(),
                current.source_revision.clone(),
                false,
            )
        }
        _ => return Err(denied()),
    };
    let pending: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND operation=?2 AND status IN ('queued','running') AND json_extract(target_json,'$.chapterId') IS ?3)",params![work,result.operation,result.target["chapterId"].as_str()],|r|r.get(0)).map_err(storage_error)?;
    if pending {
        return Err(RepositoryError::TaskNotWritable);
    }
    // A differently named proposal cannot reset the known failed-action allowance.
    let mut failures = Vec::new();
    crate::work_decision_recovery::failed_actions(db, work, &mut Vec::new(), &mut failures)?;
    if let Some(failed) = failures.iter().find(|a| {
        a.recovery.is_some()
            && a.operation == result.operation
            && a.target["chapterId"] == result.target["chapterId"]
    }) {
        if failed.target != result.target {
            return Err(RepositoryError::InvalidContent(
                "Use the failed action and a recovery plan before changing its target".into(),
            ));
        }
        result.recovery = failed.recovery.clone();
        result.source_task = failed.source_task.clone();
    }
    result.key = key(proposal)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proposed_scene_repair_narrows_author_scope_and_rebases_only_the_current_source() {
        let base = json!({"scope":{"type":"selectedScenes","sourceRevisionId":"before","groupIds":["a","b"]},"sceneOrder":1});
        let mut target = base.clone();
        narrow_scope(
            &mut target,
            Some(&json!({"type":"selectedScenes","groupIds":["b"]})),
            "current",
        )
        .unwrap();
        assert_eq!(
            target["scope"],
            json!({"type":"selectedScenes","sourceRevisionId":"current","groupIds":["b"]})
        );
        for scope in [
            json!({"type":"wholeChapter"}),
            json!({"type":"selectedScenes","groupIds":["c"]}),
            json!({"type":"selectedScenes","groupIds":["a"],"sourceRevisionId":"old"}),
            json!({"type":"selectedScenes","groupIds":["a"],"ignoreLocks":true}),
        ] {
            assert!(
                narrow_scope(&mut base.clone(), Some(&scope), "current").is_err(),
                "{scope}"
            );
        }
    }
    #[test]
    fn narrowed_repair_cannot_drop_an_author_requested_effect() {
        let mut target = json!({"scope":{"type":"selectedScenes","sourceRevisionId":"before","groupIds":["a","b"]},"sceneOrder":1,
            "desiredOutcomes":[{"id":"goal","criterion":"保留场景a的证据","groupIds":["a"]}]});
        assert!(
            narrow_scope(
                &mut target,
                Some(&json!({"type":"selectedScenes","groupIds":["b"]})),
                "current"
            )
            .is_err()
        );
    }
    #[test]
    fn protected_prose_is_readable_for_analysis_but_not_writable_or_cross_project() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_chapters_v2(project_id TEXT,chapter_id TEXT,number INTEGER); CREATE TABLE story_object_heads_v2(project_id TEXT,object_id TEXT,candidate_revision_id TEXT,accepted_revision_id TEXT,locked INTEGER); INSERT INTO story_chapters_v2 VALUES('project','chapter',1); INSERT INTO story_object_heads_v2 VALUES('project','draft-chapter','current',NULL,1);").unwrap();
        let project = ProjectId::parse("project").unwrap();
        assert_eq!(
            chapter_source(&db, &project, "chapter", false).unwrap(),
            (1, "current".into())
        );
        assert_eq!(
            chapter_source(&db, &project, "chapter", true),
            Err(RepositoryError::ObjectLocked)
        );
        assert!(
            chapter_source(
                &db,
                &ProjectId::parse("other-project").unwrap(),
                "chapter",
                false
            )
            .is_err()
        );
    }
    #[test]
    fn artifact_proposal_rejects_locked_or_missing_sources_and_rebases_commitment_revision() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_object_heads_v2(project_id TEXT, object_id TEXT, object_kind TEXT, candidate_revision_id TEXT, accepted_revision_id TEXT, locked INTEGER); INSERT INTO story_object_heads_v2 VALUES('project','artifact-brief','brief','brief-v1',NULL,1); INSERT INTO story_object_heads_v2 VALUES('project','commitment-clue','commitment','clue-v2',NULL,0);").unwrap();
        let project = ProjectId::parse("project").unwrap();
        assert_eq!(
            validate_artifact(
                &db,
                &project,
                "generateBrief",
                &mut json!({"type":"artifact"})
            ),
            Err(RepositoryError::ObjectLocked)
        );
        let mut target = json!({"type":"commitment","commitmentId":"clue","revisionId":"clue-v1"});
        validate_artifact(&db, &project, "reviseCommitment", &mut target).unwrap();
        assert_eq!(target["revisionId"], "clue-v2");
        target["commitmentId"] = json!("missing");
        assert_eq!(
            validate_artifact(&db, &project, "reviseCommitment", &mut target),
            Err(RepositoryError::TaskInputStale)
        );
    }
    #[test]
    fn proposal_contract_rejects_host_owned_authority_fields() {
        let value = json!({"operation":"generateChapter","target":{"chapterId":"chapter","scope":{"type":"wholeChapter"}}});
        let proposal: WorkActionProposal = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            key(&proposal).unwrap(),
            key(&serde_json::from_value(value.clone()).unwrap()).unwrap()
        );
        for field in [
            "runId",
            "budgetId",
            "projectId",
            "expectedRevision",
            "sourceRevisionId",
        ] {
            let mut forged = value.clone();
            forged["target"][field] = json!("injected");
            assert!(serde_json::from_value::<WorkActionProposal>(forged).is_err());
        }
    }
}
    #[test]
    fn text_ranges_cannot_be_removed_widened_or_reused_without_current_version() {
        let base=json!({"scope":{"type":"selectedScenes","sourceRevisionId":"current","groupIds":["a"],"bodyRanges":[{"groupId":"a","start":5,"end":12}]}});
        for ranges in [Value::Null,json!([]),json!([{"groupId":"a","start":4,"end":12}]),json!([{"groupId":"a","start":5,"end":13}])] {
            let mut scope=base["scope"].clone();
            if ranges.is_null(){scope.as_object_mut().unwrap().remove("bodyRanges");}else{scope["bodyRanges"]=ranges;}
            let result=narrow_scope(&mut base.clone(),Some(&scope),"current");
            assert!(result.is_err(),"{scope}");
        }
        let mut narrower=base["scope"].clone();narrower["bodyRanges"][0]["start"]=json!(7);
        assert!(narrow_scope(&mut base.clone(),Some(&narrower),"current").is_ok());
        assert!(narrow_scope(&mut base.clone(),Some(&narrower),"new-version").is_err());
    }
