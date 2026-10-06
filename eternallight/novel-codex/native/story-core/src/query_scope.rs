use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::query_bound::BoundedQueryScope;
use crate::{RepositoryError, SearchScope, SqliteStoryRepository, TaskRecord};

/// Derived exclusively from a persisted task; caller arguments cannot widen this scope.
pub(crate) enum TaskQueryScope {
    Project { operation: String },
    Bounded(BoundedQueryScope),
}

impl TaskQueryScope {
    pub fn from_task(
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
    ) -> Result<Self, RepositoryError> {
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        let bounded = |chapter, structured_through, text_scope, scope_name, source_revision_id| {
            Self::Bounded(BoundedQueryScope {
                chapter,
                structured_through,
                text_scope,
                scope_name,
                source_revision_id,
                end_scene: None,
                pov: None,
                story_time: None,
            })
        };
        match task.operation.as_str() {
            "advisory" if target["type"] == "project" => Ok(Self::Project {
                operation: task.operation.clone(),
            }),
            "advisory" if crate::work_decision::binding(&repo.connection, task)?.is_some() => {
                Ok(Self::Project {
                    operation: task.operation.clone(),
                })
            }
            "assessChapterStructure" => {
                crate::chapter_structure::assert_task(&repo.connection, task)?;
                Ok(Self::Project {
                    operation: task.operation.clone(),
                })
            }
            "generateBrief"
            | "generateOutline"
            | "reviewPlanningSeat"
            | "reviseCommitment"
            | "generatePropertyRules" => Ok(Self::Project {
                operation: task.operation.clone(),
            }),
            "generatePlan" => {
                if let Some(from) = target.get("planFromChapter") {
                    let from = from
                        .as_i64()
                        .filter(|from| (1..i64::MAX).contains(from))
                        .ok_or(RepositoryError::TaskBindingInvalid)?;
                    Ok(bounded(
                        from,
                        from - 1,
                        SearchScope::ThroughChapter(from - 1),
                        "rollingPlan",
                        /*source_revision_id*/ None,
                    ))
                } else if target.get("runId").is_some() {
                    Err(RepositoryError::TaskBindingInvalid)
                } else {
                    Ok(Self::Project {
                        operation: task.operation.clone(),
                    })
                }
            }
            "generateChapter" => Ok(Self::Bounded(BoundedQueryScope::from_task(repo, task)?)),
            "modelKnowledge" => {
                let batch = repo.modeling_batch(
                    &task.project_id,
                    target["batchId"]
                        .as_str()
                        .ok_or(RepositoryError::TaskBindingInvalid)?,
                )?;
                if !matches!(batch.status.as_str(), "queued" | "ready")
                    || batch.input_project_revision != task.input_revision
                {
                    return Err(RepositoryError::TaskInputStale);
                }
                let chapter_id = batch
                    .object_id
                    .strip_prefix("draft-")
                    .ok_or(RepositoryError::TaskBindingInvalid)?;
                let chapter =
                    bound_source_chapter(repo, task, chapter_id, &batch.source_revision_id)?;
                Ok(bounded(
                    chapter,
                    chapter,
                    SearchScope::Chapter(chapter),
                    "modelingSource",
                    Some(batch.source_revision_id),
                ))
            }
            "assessSceneOutcome" | "assessRevisionOutcome" => {
                let chapter = bound_source_chapter(
                    repo,
                    task,
                    target["chapterId"]
                        .as_str()
                        .ok_or(RepositoryError::TaskBindingInvalid)?,
                    target["draftRevisionId"]
                        .as_str()
                        .ok_or(RepositoryError::TaskBindingInvalid)?,
                )?;
                if target["chapterNumber"].as_i64() != Some(chapter) {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                if task.operation == "assessSceneOutcome" || !target["knowledgeBatchId"].is_null() {
                    let knowledge = repo.chapter_knowledge(
                        &task.project_id,
                        target["chapterId"]
                            .as_str()
                            .ok_or(RepositoryError::TaskBindingInvalid)?,
                    )?;
                    if knowledge["status"] != "ready"
                        || knowledge["batchId"] != target["knowledgeBatchId"]
                        || knowledge["sourceRevisionId"] != target["draftRevisionId"]
                    {
                        return Err(RepositoryError::TaskInputStale);
                    }
                }
                Ok(bounded(
                    chapter,
                    chapter,
                    SearchScope::Chapter(chapter),
                    if task.operation == "assessRevisionOutcome" {
                        "revisionOutcome"
                    } else {
                        "sceneAssessment"
                    },
                    Some(
                        target["draftRevisionId"]
                            .as_str()
                            .ok_or(RepositoryError::TaskBindingInvalid)?
                            .to_owned(),
                    ),
                ))
            }
            "extractCommitments" => {
                let chapter = crate::commitment_extraction::validate_target(
                    &repo.connection,
                    &task.project_id,
                    &target,
                )?;
                Ok(bounded(
                    chapter,
                    chapter,
                    SearchScope::Chapter(chapter),
                    "commitmentExtraction",
                    Some(
                        target["sourceRevisionId"]
                            .as_str()
                            .ok_or(RepositoryError::TaskBindingInvalid)?
                            .to_owned(),
                    ),
                ))
            }
            "advisory" => Err(RepositoryError::TaskBindingInvalid),
            _ => Err(RepositoryError::TaskOutputForbidden),
        }
    }

    pub fn policy(&self) -> Value {
        match self {
            Self::Project { operation } => {
                json!({"scope":if operation == "advisory" {"advisoryProject"} else if operation=="assessChapterStructure" {"chapterStructureAssessment"} else {"creativeProject"}, "operation":operation})
            }
            Self::Bounded(scope) => scope.policy(),
        }
    }

    pub fn text_scope(&self) -> SearchScope {
        match self {
            Self::Project { .. } => SearchScope::All,
            Self::Bounded(scope) => scope.text_scope(),
        }
    }

    pub fn watermarks(
        &self,
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
    ) -> Result<Value, RepositoryError> {
        match self {
            Self::Project { .. } => repo.project_source_watermarks(&task.project_id),
            Self::Bounded(scope) => scope.watermarks(repo, task),
        }
    }
}

/// A continuous run or source-bound extraction reads its own candidate checkpoint.
/// Ordinary author tasks read the explicitly selected working source instead.
pub(crate) fn uses_candidate_source(task: &TaskRecord) -> Result<bool, RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    Ok(target["runId"].as_str().is_some()
        || matches!(
            task.operation.as_str(),
            "modelKnowledge"
                | "assessSceneOutcome"
                | "assessRevisionOutcome"
                | "extractCommitments"
        ))
}

fn bound_source_chapter(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    chapter_id: &str,
    revision_id: &str,
) -> Result<i64, RepositoryError> {
    repo.connection.query_row(
        "SELECT c.number FROM story_chapters_v2 c JOIN story_object_heads_v2 h ON h.project_id = c.project_id AND h.object_id = 'draft-' || c.chapter_id WHERE c.project_id = ?1 AND c.chapter_id = ?2 AND COALESCE(h.candidate_revision_id, h.accepted_revision_id) = ?3",
        params![task.project_id.as_str(), chapter_id, revision_id], |row| row.get::<_, i64>(0),
    ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?
        .filter(|chapter| (1..i64::MAX).contains(chapter)).ok_or(RepositoryError::TaskInputStale)
}

impl SqliteStoryRepository {
    /// Structured knowledge is indexed against the current candidate source. It may
    /// not be mixed with a different author-selected or accepted text revision.
    pub fn task_source_projection_aligned(
        &self,
        task: &TaskRecord,
    ) -> Result<bool, RepositoryError> {
        if uses_candidate_source(task)? {
            return Ok(true);
        }
        let through = match TaskQueryScope::from_task(self, task)? {
            TaskQueryScope::Project { .. } => None,
            // Writer state queries stop before the chapter being written, but the
            // same task also reads that chapter as its editable text source. If
            // it is an older author-selected revision, an empty state result
            // must not be reported as complete merely because the structured
            // query's prior-chapter window is empty.
            TaskQueryScope::Bounded(scope) if scope.scope_name == "writer" => Some(scope.chapter),
            TaskQueryScope::Bounded(scope) => Some(scope.structured_through),
        };
        let mismatch = self
            .connection
            .query_row(
                "SELECT EXISTS(
               SELECT 1 FROM story_chapters_v2 c JOIN story_object_heads_v2 h
                 ON h.project_id=c.project_id AND h.object_id='draft-' || c.chapter_id
               LEFT JOIN story_working_baselines_v2 b
                 ON b.project_id=c.project_id AND b.object_kind='draft' AND b.object_id=c.chapter_id
               WHERE c.project_id=?1 AND (?2 IS NULL OR c.number<=?2)
                 AND COALESCE(b.revision_id,h.accepted_revision_id,h.candidate_revision_id)
                   IS NOT COALESCE(h.candidate_revision_id,h.accepted_revision_id)
             )",
                params![task.project_id.as_str(), through],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(mismatch == 0)
    }

    /// Describes the effective read policy that Context Pack digests must bind.
    pub fn task_query_policy(&self, task: &TaskRecord) -> Result<Value, RepositoryError> {
        Ok(TaskQueryScope::from_task(self, task)?.policy())
    }

    /// The ordinary text endpoint and composed plans share this trusted task scope.
    pub fn task_text_search_scope(
        &self,
        task: &TaskRecord,
    ) -> Result<SearchScope, RepositoryError> {
        Ok(TaskQueryScope::from_task(self, task)?.text_scope())
    }

    /// Base Context Packs use the same raw source projection as bounded query plans.
    pub fn task_context_knowledge(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<Value>, RepositoryError> {
        self.with_read_snapshot(|| {
            self.assert_task_inputs_current(task)?;
            match TaskQueryScope::from_task(self, task)? {
                TaskQueryScope::Project { .. } => Ok(None),
                TaskQueryScope::Bounded(scope) => {
                    if !self.task_source_projection_aligned(task)? {
                        return Ok(Some(json!({"entries":[],"findings":[],"coverage":{
                            "complete":false,"sourceModelingIncomplete":true,
                            "semanticCompleteness":"unknown"
                        }})));
                    }
                    scope.context_state(self, task).map(Some)
                }
            }
        })?
    }
}
