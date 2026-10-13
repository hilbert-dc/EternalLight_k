//! `GET /api/projects/{project_id}/context`: bounded context pack for authors and bound tasks.

use axum::Extension;
use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::BranchId;
use novel_codex_story_core::ContextSourceScope;
use novel_codex_story_core::DelegationTargetSelection;
use novel_codex_story_core::KnowledgePerspective;
use novel_codex_story_core::KnowledgeStateQuery;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::server::{
    Actor, RuntimeState, bad_request, candidate_task, error, internal_error, repository_error,
    unix_now,
};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ContextReadApiContextQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ContextQuery {
    task_id: Option<String>,
    thread_id: Option<String>,
    project_revision: i64,
    purpose: String,
    story_time: Option<i64>,
    reading_chapter: Option<i64>,
    scene_order: Option<i64>,
    pov_character_id: Option<String>,
    #[serde(default = "default_context_budget")]
    #[ts(as = "Option<usize>", optional)]
    budget: usize,
    #[serde(default = "default_context_byte_budget")]
    #[ts(as = "Option<usize>", optional)]
    byte_budget: usize,
    query_plan: Option<String>,
}

fn default_context_budget() -> usize {
    24
}

fn default_context_byte_budget() -> usize {
    24_000
}

pub(crate) async fn read_context(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(mut query): Query<ContextQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    if !matches!(query.purpose.as_str(), "planner" | "writer" | "critic")
        || query.budget == 0
        || query.budget > 100_000
        || !(4_000..=60_000).contains(&query.byte_budget)
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_context_request",
            "Context scope or budget is invalid",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };

    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        query.task_id.as_deref(),
        query.thread_id.as_deref(),
        Some(query.project_revision),
    ) {
        Ok(task) => task,
        Err(response) => return *response,
    };
    if task
        .as_ref()
        .is_some_and(|task| task.purpose != query.purpose)
    {
        return error(
            StatusCode::UNAUTHORIZED,
            "task_scope_invalid",
            "Task does not permit this context purpose",
        );
    }
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.assert_task_inputs_current(task)
    {
        return repository_error(source);
    }
    let writer_baseline = match task
        .as_ref()
        .map(|task| repository.task_writer_baseline(task))
        .transpose()
    {
        Ok(source) => source.flatten(),
        Err(source) => return repository_error(source),
    };
    let writer_scene_limit =
        if let Some(task) = task.as_ref().filter(|task| task.purpose == "writer") {
            let target: Value = match serde_json::from_str(&task.target_json) {
                Ok(value) => value,
                Err(_) => return internal_error("Writer task target is corrupt"),
            };
            query.reading_chapter = query
                .reading_chapter
                .or_else(|| target["readingChapter"].as_i64());
            query.scene_order = query.scene_order.or_else(|| target["sceneOrder"].as_i64());
            query.story_time = query.story_time.or_else(|| target["storyTime"].as_i64());
            query.pov_character_id = query
                .pov_character_id
                .or_else(|| target["povCharacterId"].as_str().map(str::to_owned));
            if target["readingChapter"].as_i64() != query.reading_chapter
                || target["sceneOrder"].as_i64() != query.scene_order
                || target["storyTime"].as_i64() != query.story_time
                || target["povCharacterId"].as_str() != query.pov_character_id.as_deref()
            {
                return error(
                    StatusCode::FORBIDDEN,
                    "writer_scope_mismatch",
                    "Writer context position must match its frozen task target",
                );
            }
            match novel_codex_story_core::reader_scene_limit(&target) {
                Ok(limit) => limit,
                Err(source) => return repository_error(source),
            }
        } else {
            None
        };
    let commitment_source_task = task
        .as_ref()
        .is_some_and(|task| task.operation == "extractCommitments");
    let scene_assessment_task = task
        .as_ref()
        .filter(|task| task.operation == "assessSceneOutcome");
    let chapter_structure_input = match task
        .as_ref()
        .filter(|task| task.operation == "assessChapterStructure")
        .map(|task| repository.chapter_structure_context(task, unix_now()))
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let revision_outcome_task = task
        .as_ref()
        .filter(|task| task.operation == "assessRevisionOutcome");
    let revision_outcome_input = match revision_outcome_task
        .map(|task| repository.revision_outcome_input(task))
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let revision_repair_input = match task
        .as_ref()
        .map(|task| repository.revision_repair_input(task))
        .transpose()
    {
        Ok(value) => value.flatten(),
        Err(source) => return repository_error(source),
    };
    let mandatory_input_bytes = if task.is_some() {
        0
    } else {
        revision_outcome_input
            .iter()
            .chain(revision_repair_input.iter())
            .chain(chapter_structure_input.iter())
            .map(|input| input.to_string().len())
            .sum::<usize>()
    };
    let assessment_chapter = task
        .as_ref()
        .filter(|task| {
            matches!(
                task.operation.as_str(),
                "assessSceneOutcome" | "extractCommitments" | "assessRevisionOutcome"
            )
        })
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .and_then(|target| target["chapterNumber"].as_i64());
    let rolling_plan_position = task
        .as_ref()
        .filter(|task| task.operation == "generatePlan")
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .and_then(|target| target["planFromChapter"].as_i64())
        .filter(|chapter| *chapter >= 1)
        .map(|chapter| chapter - 1)
        .or(assessment_chapter);
    let revision = match repository.project_revision(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    if revision != query.project_revision {
        return error(
            StatusCode::CONFLICT,
            "context_revision_mismatch",
            "Context must be read from the current project revision",
        );
    }
    let main_branch = BranchId::parse("main").expect("main branch id");
    let source_snapshot = match repository.branch_head(&project_id, &main_branch) {
        Ok(Some(snapshot)) => snapshot.snapshot_id,
        Ok(None) => return internal_error("Main branch is missing"),
        Err(source) => return repository_error(source),
    };
    let source_heads = match repository.project_source_watermarks(&project_id) {
        Ok(watermarks) => watermarks,
        Err(source) => return repository_error(source),
    };
    let query_report = match crate::context_query::retrieve(
        &repository,
        task.as_ref(),
        query.query_plan.as_deref(),
        crate::context_query::ContextQueryBinding {
            snapshot_id: source_snapshot.as_str(),
            byte_limit: query.byte_budget,
            now: unix_now(),
        },
    ) {
        Ok(report) => report,
        Err(RepositoryError::TaskOutputForbidden) => {
            return error(
                StatusCode::FORBIDDEN,
                "context_query_forbidden",
                "Query plan exceeds the bound task scope",
            );
        }
        Err(source) => return repository_error(source),
    };
    let query_scope_policy = match task
        .as_ref()
        .map(|task| repository.task_query_policy(task))
        .transpose()
    {
        Ok(policy) => policy,
        Err(source) => return repository_error(source),
    };
    let project = match repository.project(&project_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            );
        }
        Err(source) => return repository_error(source),
    };
    let (heads, working_baselines) = match task
        .as_ref()
        .map(|task| repository.task_context_origins(task))
        .unwrap_or_else(|| {
            repository.object_heads(&project_id).and_then(|heads| {
                repository
                    .working_baselines(&project_id)
                    .map(|baselines| (heads, baselines))
            })
        }) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let run_bound_task = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .is_some_and(|target| target["runId"].as_str().is_some());
    let source_policy = if run_bound_task || commitment_source_task || assessment_chapter.is_some()
    {
        crate::context_working_source::SourcePolicy::RunOrBoundTask
    } else {
        crate::context_working_source::SourcePolicy::AuthorSelected
    };
    let draft_sources =
        crate::context_working_source::draft_sources(&heads, &working_baselines, source_policy);
    let mut artifacts = serde_json::Map::new();
    if let Some(task) = task
        .as_ref()
        .filter(|task| task.operation == "generatePropertyRules")
    {
        let baseline = match repository.task_property_rule_input(task) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        artifacts.insert("propertyRules".into(), baseline);
    }
    for kind in ["brief", "outline", "plan"] {
        if query.purpose == "writer" {
            artifacts.insert(kind.into(), Value::Null);
            continue;
        }
        let object_id = format!("artifact-{kind}");
        let head = heads
            .iter()
            .find(|head| head.object_id.as_str() == object_id);
        let selected = head.and_then(|head| {
            crate::context_working_source::context_source(
                head,
                crate::context_working_source::selected_baseline(&working_baselines, kind, kind),
                source_policy,
            )
        });
        let value = if let Some(selected) = selected {
            let content = match repository.revision_content(selected.revision_id) {
                Ok(Some(content)) => content,
                Ok(None) => return repository_error(RepositoryError::CorruptState),
                Err(source) => return repository_error(source),
            };
            let mut value: Value = match serde_json::from_str(&content) {
                Ok(value) => value,
                Err(_) => return repository_error(RepositoryError::CorruptState),
            };
            value = value.get("payload").cloned().unwrap_or(value);
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "sourceRevisionId".into(),
                    json!(selected.revision_id.as_str()),
                );
                object.insert("sourceAuthority".into(), json!(selected.authority));
                object.insert(
                    "workingSelectionRevision".into(),
                    json!(selected.selection_revision),
                );
            }
            value
        } else {
            Value::Null
        };
        artifacts.insert(kind.into(), value);
    }
    if query.purpose == "writer" {
        let bindings = match repository.chapter_bindings(&project_id) {
            Ok(bindings) => bindings,
            Err(source) => return repository_error(source),
        };
        if let Some(binding) = bindings
            .iter()
            .find(|binding| Some(binding.chapter_number) == query.reading_chapter)
        {
            let chapter = match repository.bound_chapter_plan(&project_id, binding) {
                Ok(chapter) => chapter,
                Err(source) => return repository_error(source),
            };
            let mut plan = match writer_scene_limit {
                Some(end) => crate::context_visibility::visible_scene_plan(&chapter, end),
                None => json!({"chapters": [chapter]}),
            };
            if !plan.is_null() {
                plan["planRevisionId"] = json!(binding.plan_revision_id);
            }
            artifacts.insert("plan".into(), plan);
        }
    }
    if commitment_source_task || revision_outcome_task.is_some() || scene_assessment_task.is_some()
    {
        for value in artifacts.values_mut() {
            *value = Value::Null;
        }
    }
    let chapters = match repository.chapters(&project_id) {
        Ok(chapters) => chapters,
        Err(source) => return repository_error(source),
    };
    let pov_writer = query.purpose == "writer" && query.pov_character_id.is_some();
    let pov_prior_text_withheld = pov_writer
        && chapters.iter().any(|chapter| {
            query
                .reading_chapter
                .is_some_and(|position| chapter.number < position)
        });
    let writer_allowed_objects = if query.purpose == "writer" {
        let mut allowed = chapters
            .iter()
            .filter(|chapter| {
                query
                    .reading_chapter
                    .is_some_and(|position| chapter.number <= position)
            })
            .map(|chapter| format!("draft-{}", chapter.chapter_id))
            .collect::<HashSet<_>>();
        allowed.insert("artifact-plan".to_owned());
        allowed
    } else {
        HashSet::new()
    };
    // Transport budgets cannot change the material registered for a bound source.
    let draft_limit = if task.is_some() {
        80
    } else {
        query.budget.min(80)
    };
    let mut scoped_chapters = chapters
        .iter()
        .filter(|chapter| {
            revision_outcome_task.is_none()
                && (query.purpose != "writer"
                    || query
                        .reading_chapter
                        .is_some_and(|position| chapter.number <= position))
                && (!pov_writer || query.reading_chapter == Some(chapter.number))
                && assessment_chapter.is_none_or(|position| chapter.number == position)
                && rolling_plan_position.is_none_or(|position| chapter.number <= position)
        })
        .collect::<Vec<_>>();
    let drafts_in_scope = scoped_chapters.len();
    if query.purpose == "writer" || rolling_plan_position.is_some() {
        scoped_chapters.reverse();
        scoped_chapters.truncate(draft_limit);
    } else {
        scoped_chapters.truncate(draft_limit);
    }
    let mut drafts_truncated = drafts_in_scope > scoped_chapters.len();
    let mut used_bytes = mandatory_input_bytes;
    let mut draft_context = Vec::new();
    for chapter in scoped_chapters {
        let Some(head) = heads
            .iter()
            .find(|head| head.object_id.as_str() == format!("draft-{}", chapter.chapter_id))
        else {
            continue;
        };
        let Some(source) = crate::context_working_source::context_source(
            head,
            crate::context_working_source::selected_baseline(
                &working_baselines,
                "draft",
                &chapter.chapter_id,
            ),
            source_policy,
        ) else {
            continue;
        };
        let content = match repository.revision_content(source.revision_id) {
            Ok(Some(content)) => content,
            Ok(None) => return repository_error(RepositoryError::CorruptState),
            Err(source) => return repository_error(source),
        };
        let mut value: Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => return internal_error("Draft revision is corrupt"),
        };
        if query.purpose == "writer"
            && query.reading_chapter == Some(chapter.number)
            && let Some(end) = writer_scene_limit
            && let Err(source) = crate::context_visibility::visible_draft_prefix(&mut value, end)
        {
            return repository_error(source);
        }
        let draft = json!({
            "objectId": head.object_id.as_str(),
            "sourceRevisionId": source.revision_id.as_str(),
            "content": value,
            "authority": source.authority,
            "workingSelectionRevision": source.selection_revision
        });
        let draft = writer_baseline
            .as_ref()
            .filter(|baseline| baseline["objectId"] == draft["objectId"])
            .cloned()
            .unwrap_or(draft);
        // Budget the model projection while retaining canonical text for source linking.
        let mut projected_draft = draft.clone();
        if task.is_some() {
            projected_draft = json!({"objectId":draft["objectId"],"sourceRevisionId":draft["sourceRevisionId"],"textDelivered":false});
        } else if scene_assessment_task.is_some()
            || writer_baseline
                .as_ref()
                .is_some_and(|baseline| baseline["objectId"] == draft["objectId"])
        {
            match crate::context_assessment_source::project(&mut projected_draft) {
                Ok(_) => {}
                Err(source) => return repository_error(source),
            }
        } else {
            crate::context_model_item_budget::project_text(&mut projected_draft);
        }
        let visible_bytes = projected_draft.to_string().len();
        if task.is_none() && used_bytes + visible_bytes > query.byte_budget.saturating_sub(2_000) {
            if query.purpose == "writer" && query.reading_chapter == Some(chapter.number) {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "context_budget_exceeded",
                    "The target draft exceeds byteBudget; narrow the scene scope or increase the budget",
                );
            }
            drafts_truncated = true;
            break;
        }
        used_bytes += visible_bytes;
        draft_context.push(draft);
    }
    if commitment_source_task
        && !draft_context.iter().any(|draft| {
            query_scope_policy
                .as_ref()
                .is_some_and(|policy| draft["sourceRevisionId"] == policy["sourceRevisionId"])
        })
    {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_budget_exceeded",
            "The bound extraction source must fit the context budget",
        );
    }
    if query.purpose == "writer" || rolling_plan_position.is_some() {
        draft_context.sort_by_key(|draft| {
            draft["content"]["chapterNumber"]
                .as_i64()
                .unwrap_or(i64::MAX)
        });
    }
    let available_facts = match repository.facts(&project_id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let mut facts = Vec::new();
    let mut facts_truncated = false;
    let mut typed_state_facts_withheld = 0;
    let mut typed_event_facts_withheld = 0;
    let mut fact_bytes = 0;
    let fact_byte_budget = query.byte_budget.saturating_sub(used_bytes + 2_000);
    for fact in available_facts {
        if fact["sourceRevisionId"]
            .as_str()
            .is_some_and(|revision| !draft_sources.values().any(|selected| selected == revision))
        {
            continue;
        }
        if query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
            .is_some_and(|position| {
                fact["readingChapter"]
                    .as_i64()
                    .is_none_or(|chapter| chapter > position)
            })
        {
            continue;
        }
        if query.purpose == "writer" {
            let in_past = fact["readingChapter"].as_i64().is_some_and(|chapter| {
                query
                    .reading_chapter
                    .is_some_and(|current| chapter < current)
            });
            let visible = if pov_writer {
                fact["factKind"] == "character"
                    && fact["epistemicOwnerId"].as_str() == query.pov_character_id.as_deref()
                    && query.story_time.is_some_and(|at| {
                        novel_codex_story_core::epistemic_claim_active_at(
                            &fact["modelingAssertion"],
                            at,
                        )
                    })
            } else {
                match fact["factKind"].as_str() {
                    Some("world" | "reader") => true,
                    Some("character") => {
                        fact["epistemicOwnerId"].as_str() == query.pov_character_id.as_deref()
                    }
                    _ => false,
                }
            };
            if !in_past || !visible {
                continue;
            }
        }
        if fact["modelingAssertion"]["kind"] == "state" {
            // Confirmation does not erase the interval or make this a timeless fact.
            // Consume these through the typed, time-scoped knowledge query instead.
            typed_state_facts_withheld += 1;
            continue;
        }
        if fact["modelingAssertion"]["kind"] == "event" {
            // Event references retain their source-batch identity in typed queries.
            typed_event_facts_withheld += 1;
            continue;
        }
        let size = fact.to_string().len();
        if facts.len() >= query.budget.min(200) || fact_bytes + size > fact_byte_budget {
            facts_truncated = true;
            break;
        }
        fact_bytes += size;
        facts.push(fact);
    }
    used_bytes += fact_bytes;
    let modeling_through = if query.purpose == "writer" {
        query.reading_chapter
    } else {
        query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
    };
    let modeling_scope = modeling_through.map_or(
        ContextSourceScope::Project,
        ContextSourceScope::ThroughChapter,
    );
    let modeling_sources = match repository.context_sources(&project_id, modeling_scope) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let modeling_sources_in_scope = modeling_sources.total_sources;
    let mut modeling = modeling_sources.sources;
    let chapter_numbers = chapters
        .iter()
        .map(|chapter| (chapter.chapter_id.as_str(), chapter.number))
        .collect::<HashMap<_, _>>();
    let mut modeling_source_mismatch = heads.iter().any(|head| {
        head.object_id
            .as_str()
            .strip_prefix("draft-")
            .and_then(|chapter_id| chapter_numbers.get(chapter_id))
            .is_some_and(|chapter| modeling_through.is_none_or(|through| *chapter <= through))
            && draft_sources
                .get(head.object_id.as_str())
                .is_some_and(|selected| {
                    head.candidate_revision_id
                        .as_ref()
                        .or(head.accepted_revision_id.as_ref())
                        .is_none_or(|current| selected != current.as_str())
                })
    });
    for source in &mut modeling {
        if let Some(selected) = source["objectId"]
            .as_str()
            .and_then(|object_id| draft_sources.get(object_id))
            && source["sourceRevisionId"] != *selected
        {
            source["sourceRevisionId"] = json!(selected);
            source["batchId"] = Value::Null;
            source["status"] = json!("model_pending");
            modeling_source_mismatch = true;
        }
    }
    let planning_review = match task.as_ref() {
        Some(task) if task.operation == "reviewPlanningSeat" => {
            match repository.planning_review_context(&project_id, &task.task_id) {
                Ok(value) => value,
                Err(source) => return repository_error(source),
            }
        }
        _ => None,
    };
    let modeling_batch = match task.as_ref() {
        Some(task) if task.operation == "modelKnowledge" => {
            let target = serde_json::from_str::<Value>(&task.target_json).unwrap_or(Value::Null);
            match target["batchId"].as_str() {
                Some(batch_id) => match repository.modeling_batch(&project_id, batch_id) {
                    Ok(batch)
                        if batch.input_project_revision == revision && batch.status == "queued" =>
                    {
                        Some(batch.as_json())
                    }
                    Ok(_) => {
                        return error(
                            StatusCode::CONFLICT,
                            "modeling_batch_stale",
                            "Modeling batch is not current",
                        );
                    }
                    Err(source) => return repository_error(source),
                },
                None => {
                    return bad_request("modeling_batch_missing", "Modeling task has no batchId");
                }
            }
        }
        _ => None,
    };
    if let Some(batch) = modeling_batch.as_ref() {
        let revision_id = match RevisionId::parse(
            batch["sourceRevisionId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ) {
            Ok(value) => value,
            Err(_) => return internal_error("Modeling batch source revision is invalid"),
        };
        let content = match repository.revision_content(&revision_id) {
            Ok(Some(content)) => content,
            Ok(None) => return internal_error("Modeling batch source revision is missing"),
            Err(source) => return repository_error(source),
        };
        let draft = match serde_json::from_str::<Value>(&content) {
            Ok(value) => json!({
                "objectId": batch["objectId"],
                "sourceRevisionId": revision_id.as_str(),
                "content": value,
                "authority": "candidate"
            }),
            Err(_) => return internal_error("Modeling source revision is corrupt"),
        };
        let mut projected_draft = draft.clone();
        crate::context_model_item_budget::project_text(&mut projected_draft);
        let source_bytes = if task.is_some() {
            128
        } else {
            projected_draft.to_string().len()
        };
        if source_bytes + mandatory_input_bytes > query.byte_budget.saturating_sub(2_000) {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "modeling_source_too_large",
                "Source draft exceeds the modeling context budget",
            );
        }
        draft_context = vec![draft];
        used_bytes = source_bytes + mandatory_input_bytes;
        drafts_truncated = false;
    }
    let entity_catalog = if modeling_batch.is_some() {
        let report_bytes = query_report
            .as_ref()
            .map_or(0, |report| report.to_string().len());
        let available = if task.is_some() {
            64 * 1024 * 1024
        } else {
            query
                .byte_budget
                .saturating_sub(used_bytes + report_bytes + 2_000)
        };
        let catalog = match query_scope_policy
            .as_ref()
            .and_then(|policy| policy["structuredThroughChapter"].as_i64())
        {
            Some(position) => {
                repository.modeling_entity_catalog_through_chapter(&project_id, position, available)
            }
            None => return repository_error(RepositoryError::TaskBindingInvalid),
        };
        match catalog {
            Ok(value) => Some(value),
            Err(source) => return repository_error(source),
        }
    } else {
        None
    };
    let working_knowledge = if query.purpose == "writer" || rolling_plan_position.is_some() {
        let projection_result = if let Some(task) = task.as_ref() {
            repository
                .task_context_knowledge(task)
                .and_then(|projection| projection.ok_or(RepositoryError::TaskBindingInvalid))
        } else {
            repository.knowledge_state(KnowledgeStateQuery {
                project_id: &project_id,
                snapshot_id: None,
                perspective: if pov_writer {
                    KnowledgePerspective::Character
                } else {
                    KnowledgePerspective::Reader
                },
                world_time: None,
                epistemic_time: if pov_writer { query.story_time } else { None },
                reading_chapter: rolling_plan_position.or_else(|| {
                    query
                        .reading_chapter
                        .map(|chapter| chapter.saturating_sub(1))
                }),
                owner_id: if pov_writer {
                    query.pov_character_id.as_deref()
                } else {
                    None
                },
                entity_id: None,
                predicate: None,
            })
        };
        let mut projection = match projection_result {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        };
        if modeling_source_mismatch {
            if let Some(entries) = projection["entries"].as_array_mut() {
                entries.retain(|entry| {
                    entry["sourceObjectId"]
                        .as_str()
                        .and_then(|object_id| draft_sources.get(object_id))
                        .is_some_and(|selected| entry["sourceRevisionId"] == *selected)
                });
            }
            projection["findings"] = json!([]);
            projection["coverage"]["complete"] = json!(false);
            projection["coverage"]["sourceModelingIncomplete"] = json!(true);
        }
        if pov_writer {
            if query.story_time.is_none() {
                projection["entries"] = json!([]);
            }
            projection["findings"] = json!([]);
            projection["coverage"]["complete"] = json!(false);
            projection["coverage"]["povWorldEvidenceWithheld"] = json!(true);
        }
        let available = query.byte_budget.saturating_sub(used_bytes + 2_000);
        let mut retained_bytes = 0;
        if let Some(entries) = projection["entries"].as_array_mut() {
            let before = entries.len();
            entries.retain(|entry| {
                let size = entry.to_string().len();
                if task.is_none() && retained_bytes + size > available {
                    return false;
                }
                retained_bytes += size;
                true
            });
            let truncated = entries.len() < before;
            let confirmed_entries = entries
                .iter()
                .filter(|entry| entry["confirmationState"] == "confirmed")
                .count();
            let candidate_entries = entries.len() - confirmed_entries;
            if truncated {
                projection["coverage"]["complete"] = json!(false);
                projection["coverage"]["truncatedEntries"] = json!(true);
                projection["coverage"]["findingsTruncated"] = json!(true);
                projection["findings"] = json!([]);
            }
            projection["coverage"]["confirmedEntries"] = json!(confirmed_entries);
            projection["coverage"]["candidateEntries"] = json!(candidate_entries);
        }
        Some(projection)
    } else {
        None
    };
    if let Some(task) = scene_assessment_task {
        let target: Value = match serde_json::from_str(&task.target_json) {
            Ok(target) => target,
            Err(_) => return repository_error(RepositoryError::CorruptState),
        };
        let Some(draft) = draft_context
            .iter()
            .find(|draft| draft["sourceRevisionId"] == target["draftRevisionId"])
        else {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "context_budget_exceeded",
                "The frozen assessment source exceeds byteBudget; no target chapter text was delivered",
            );
        };
        if draft["content"]["chapterId"] != target["chapterId"]
            || draft["content"]["sourcePlanRefs"]["planRevisionId"] != target["planRevisionId"]
        {
            return repository_error(RepositoryError::TaskBindingInvalid);
        }
    }
    let scene_links = if let Some(last_completed_chapter) = rolling_plan_position
        && revision_outcome_task.is_none()
    {
        match crate::scene_plan_context::scene_plan_draft_links(
            &repository,
            &project_id,
            &draft_context,
            last_completed_chapter + 1,
            working_knowledge.as_ref(),
            task.as_ref(),
        ) {
            Ok(links) => links,
            Err(source) => return repository_error(source),
        }
    } else {
        crate::scene_plan_context::ScenePlanLinks {
            links: Vec::new(),
            scene_limit_reached: false,
            event_limit_reached: false,
        }
    };
    let modeling_complete = modeling.len() == modeling_sources_in_scope
        && modeling.iter().all(|item| item["status"] == "ready");
    let working_knowledge_complete = working_knowledge
        .as_ref()
        .is_none_or(|value| value["coverage"]["complete"] == true);
    let watermark_result = if let Some(task) = task.as_ref() {
        repository.task_query_source_watermarks(task)
    } else if query.purpose == "writer" {
        repository.writer_source_watermarks(&project_id, &writer_allowed_objects)
    } else {
        repository.project_source_watermarks(&project_id)
    };
    let mut source_watermarks = match watermark_result {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    source_watermarks["snapshotId"] = json!(source_snapshot.as_str());
    source_watermarks["projectRevision"] = json!(revision);
    let task_target = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.target_json).ok())
        .unwrap_or(Value::Null);
    let input_manifest = task
        .as_ref()
        .and_then(|task| serde_json::from_str::<Value>(&task.input_manifest_json).ok());
    let mut commitments = match task.as_ref() {
        Some(task) => match repository.task_context_commitments(task) {
            Ok(value) => value,
            Err(source) => return repository_error(source),
        },
        None => {
            json!({"entries":[],"proposalContext":{"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}},"coverage":{"complete":false,"taskBound":false,
            "semanticCompleteness":"unknown","includedEntries":0}})
        }
    };
    if modeling_source_mismatch
        && matches!(
            source_policy,
            crate::context_working_source::SourcePolicy::AuthorSelected
        )
    {
        crate::context_working_source::withhold_mismatched_commitments(
            &mut commitments,
            &draft_sources,
        );
    }
    let delegation_targets = match task
        .as_ref()
        .map(|task| {
            repository.task_delegation_targets(
                task,
                unix_now(),
                query.reading_chapter.map_or(
                    DelegationTargetSelection::Project,
                    DelegationTargetSelection::ChapterNumber,
                ),
            )
        })
        .transpose()
    {
        Ok(value) => value.flatten(),
        Err(source) => return repository_error(source),
    };
    // Scene links consume canonical text before the final transport projection.
    let mut required_source_read = None;
    for draft in &mut draft_context {
        let writer_source = writer_baseline
            .as_ref()
            .is_some_and(|baseline| baseline["objectId"] == draft["objectId"]);
        if task.is_some() {
            crate::context_model_item_budget::project_text(draft);
        } else if scene_assessment_task.is_some() || writer_source {
            match crate::context_assessment_source::project(draft) {
                Ok(mut source) => {
                    if writer_source {
                        source["plan"]["steps"][0]["id"] = json!("writer-baseline");
                        source["plan"]["steps"][0]["operation"]["type"] =
                            json!("writerBaselinePassages");
                    }
                    required_source_read = Some(source);
                }
                Err(source) => return repository_error(source),
            }
        } else {
            crate::context_model_item_budget::project_text(draft);
        }
    }
    let mut context =
        crate::context_pack::build_context_pack(crate::context_pack::ContextPackInput {
            query_report,
            query_scope_policy,
            project_id: project_id.as_str().to_owned(),
            project_revision: revision,
            snapshot_id: source_snapshot.as_str().to_owned(),
            task_id: task.as_ref().map(|task| task.task_id.clone()),
            purpose: query.purpose.clone(),
            actor: if actor == Actor::Author {
                "author"
            } else {
                "candidate"
            }
            .to_owned(),
            position: json!({
                "storyTime": query.story_time,
                "readingChapter": query.reading_chapter,
                "sceneOrder": query.scene_order,
                "povCharacterId": query.pov_character_id
            }),
            task_target,
            delegation_targets,
            input_manifest,
            title: project.title,
            logline: project.logline,
            artifacts,
            drafts: draft_context,
            facts,
            modeling,
            commitments,
            planning_review,
            modeling_batch,
            entity_catalog,
            working_knowledge,
            scene_plan_draft_links: scene_links.links,
            scene_links_truncated: scene_links.scene_limit_reached,
            scene_event_refs_truncated: scene_links.event_limit_reached,
            source_watermarks,
            chapters_in_scope: drafts_in_scope,
            modeling_sources_in_scope,
            drafts_truncated,
            facts_truncated,
            typed_state_facts_withheld,
            typed_event_facts_withheld,
            modeling_complete,
            working_knowledge_complete,
            pov_evidence_withheld: pov_writer,
            pov_prior_text_withheld,
            byte_limit: query.byte_budget,
            entry_limit: query.budget,
        });
    if let Some(task) = task.as_ref().filter(|task| task.operation == "advisory") {
        let intents = match repository.task_intent_store().task_intents(&project_id, &task.task_id) {
            Ok(intents) => intents,
            Err(source) => return repository_error(source),
        };
        let latest = intents.last();
        let pending = latest.is_some_and(|intent| intent.status == "pending");
        context["authorIntentState"] = json!({
            "expectedIntentRevision": latest.map_or(0, |intent| intent.intent_revision),
            "canRequestActions": !pending,
            "status": if pending { "pendingDelivery" } else { "current" }
        });
    }
    let has_mandatory_input = revision_outcome_input.is_some()
        || revision_repair_input.is_some()
        || chapter_structure_input.is_some()
        || required_source_read.is_some();
    if let Some(source) = required_source_read {
        context["operationContext"]["requiredSourceReads"] = source;
        context["coverage"]["complete"] = json!(false);
        let missing = context["coverage"]["missingScope"]
            .as_array_mut()
            .expect("missing scope array");
        if !missing.iter().any(|entry| entry == "chapterText") {
            missing.push(json!("chapterText"));
        }
    }
    if let Some(input) = chapter_structure_input {
        context["operationContext"]["chapterStructureChange"] = input;
    }
    if let Some(input) = revision_outcome_input {
        if input["coverage"]["complete"] != true {
            context["coverage"]["complete"] = json!(false);
            context["coverage"]["missingScope"]
                .as_array_mut()
                .expect("missing scope array")
                .push(json!("revisionOutcomeScenes"));
            context["coverage"]["truncationReasons"]
                .as_array_mut()
                .expect("truncation reasons array")
                .push(json!("revisionOutcomeTextBudget"));
        }
        // Critic sees frozen standards, saved current/original prose and only necessary
        // prior prose sources. Author prose and planner material cannot grade their own draft.
        context["activeIntentRefs"] = json!([]);
        context["narrativeGoals"] = json!({});
        context["queryReport"] = json!(null);
        for field in [
            "relevantCommitments",
            "candidateCommitmentProposals",
            "sceneOutcomeRefs",
            "plannedEffects",
            "stateFacts",
            "evidenceRefs",
            "relevantText",
            "openQuestions",
        ] {
            context[field] = json!([]);
        }
        context["operationContext"] = json!({"revisionOutcomeInput":input});
    }
    if let Some(input) = revision_repair_input {
        context["operationContext"]["revisionRepairInput"] = input;
    }
    if let Some(task) = task.as_ref() {
        match repository.task_generation_scene_feedback(task) {
            Ok(Some(feedback)) => context["operationContext"]["generationSceneFeedback"] = feedback,
            Ok(None) => (),
            Err(source) => return repository_error(source),
        }
    }
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.project_task_sources(task, &mut context)
    {
        return repository_error(source);
    }
    if has_mandatory_input || task.is_some() {
        // Cache full source-bound inputs first, then fit actual finalized candidates.
        crate::context_budget::fit_optional_context(&mut context, query.byte_budget);
    }
    let final_size = context["budgetsUsed"]["byteSize"]
        .as_u64()
        .unwrap_or(u64::MAX);
    if !crate::context_model_item_budget::fits(&context) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_model_item_budget_exceeded",
            "Required scoped context exceeds the model-item budget; no chapter text was delivered. Reduce optional budget or read authorized passages with query_story in the same frozen scope. Do not infer missing facts or complete reading from this error.",
        );
    }
    if final_size > query.byte_budget as u64 {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_budget_exceeded",
            "The scoped context exceeds byteBudget; narrow the task scope or increase the budget",
        );
    }
    if let Some(task) = task.as_ref()
        && let Err(source) = repository.assert_task_inputs_current(task)
    {
        return repository_error(source);
    }
    let final_snapshot = match repository.branch_head(&project_id, &main_branch) {
        Ok(Some(snapshot)) => snapshot.snapshot_id,
        Ok(None) => return internal_error("Main branch is missing"),
        Err(source) => return repository_error(source),
    };
    if final_snapshot != source_snapshot {
        return error(
            StatusCode::CONFLICT,
            "context_snapshot_changed",
            "Context source snapshot changed during construction",
        );
    }
    let final_heads = match repository.project_source_watermarks(&project_id) {
        Ok(watermarks) => watermarks,
        Err(source) => return repository_error(source),
    };
    if final_heads != source_heads {
        return error(
            StatusCode::CONFLICT,
            "context_sources_changed",
            "Context object or knowledge sources changed during construction",
        );
    }
    Json(context).into_response()

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/context",
        axum::routing::get(crate::context_read_api::read_context),
    )
}

use novel_codex_story_core::{BoundWriterPlanRepository, ChaptersRepository, CommitmentContextRepository, ContextSourcesRepository, EntityIdentityRepository, KnowledgeProjectionRepository, ModelingRepository, PlanningReviewsRepository, ProjectRevisionRepository, PropertyRuleInputRepository, QueryPlanRepository, QueryScopeRepository, RevisionOutcomeContextRepository, RevisionRepairRepository, TaskChapterStructureRepository, TaskDelegationTargetsRepository, TaskInputRepository, TaskSourcesRepository, WorkflowRepository, WorkspaceSelectionRepository, WriterBaselineRepository};
