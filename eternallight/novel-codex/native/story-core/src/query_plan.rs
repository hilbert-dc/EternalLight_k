use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    BranchId, KnowledgePerspective, KnowledgeStateQuery, QueryRanking, RelationDirection,
    RelationGraphQuery, RelationGraphScope, RepositoryError, SearchQuery, SearchScope,
    SqliteStoryRepository, TaskRecord,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryQueryPlan {
    pub question: String,
    pub byte_budget: usize,
    pub steps: Vec<QueryStep>,
    pub ranking: QueryRanking,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryStep {
    pub id: String,
    pub operation: QueryOperation,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum QueryOperation {
    Passages {
        cursor: Option<String>,
        max_chars: usize,
        limit: usize,
    },
    Text {
        query: String,
        limit: usize,
    },
    ResolveEntity {
        name: String,
    },
    State {
        entity_id: String,
        predicate: Option<String>,
        perspective: QueryPerspective,
        owner_id: Option<String>,
        reading_chapter: Option<i64>,
        world_time: Option<i64>,
        epistemic_time: Option<i64>,
    },
    Relations {
        entity_id: String,
        relation_type: Option<String>,
        direction: QueryDirection,
        world_time: Option<i64>,
        max_hops: usize,
        limit: usize,
    },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QueryPerspective {
    Author,
    World,
    Character,
    Reader,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QueryDirection {
    Outgoing,
    Incoming,
    Both,
}

impl StoryQueryPlan {
    pub(crate) fn validate(&self) -> Result<(), RepositoryError> {
        self.ranking.validate()?;
        if !(4_000..=9_000).contains(&self.byte_budget) {
            return Err(RepositoryError::InvalidContent(
                "query plan byteBudget must be between 4000 and 9000 bytes inclusive; reduce steps or results when coverage is truncated, not below this minimum".into(),
            ));
        }
        if self.question.trim().is_empty()
            || self.question.len() > 1_000
            || !(1..=4).contains(&self.steps.len())
            || serde_json::to_vec(self)
                .map_err(|_| RepositoryError::CorruptState)?
                .len()
                > 9_000
        {
            return Err(RepositoryError::InvalidContent(
                "invalid query plan budget or question".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if step.id.is_empty() || step.id.len() > 32 || !ids.insert(&step.id) {
                return Err(RepositoryError::InvalidContent(
                    "invalid or duplicate query step id".into(),
                ));
            }
            let valid = match &step.operation {
                QueryOperation::Passages {
                    cursor,
                    max_chars,
                    limit,
                } => {
                    (80..=400).contains(max_chars)
                        && (1..=4).contains(limit)
                        && cursor
                            .as_ref()
                            .is_none_or(|cursor| !cursor.is_empty() && cursor.len() <= 768)
                }
                QueryOperation::Text { query, limit } => {
                    !query.trim().is_empty()
                        && query.chars().count() <= 240
                        && (1..=20).contains(limit)
                }
                QueryOperation::ResolveEntity { name } => {
                    !name.trim().is_empty() && name.chars().count() <= 160
                }
                QueryOperation::State { entity_id, .. } => {
                    !entity_id.is_empty() && entity_id.len() <= 160
                }
                QueryOperation::Relations {
                    entity_id,
                    max_hops,
                    limit,
                    ..
                } => {
                    !entity_id.is_empty()
                        && entity_id.len() <= 160
                        && (1..=3).contains(max_hops)
                        && (1..=20).contains(limit)
                }
            };
            if !valid {
                return Err(RepositoryError::InvalidContent(
                    "invalid query operation".into(),
                ));
            }
        }
        Ok(())
    }
}

impl SqliteStoryRepository {
    /// Executes a finite plan within current task inputs; Writer scope is derived only from its binding.
    pub fn query_task_plan(
        &self,
        task: &TaskRecord,
        plan: &StoryQueryPlan,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| self.query_task_plan_in_snapshot(task, plan, now))?
    }

    /// Caller owns the read or explicit write transaction; never opens a nested transaction.
    pub(crate) fn query_task_plan_in_snapshot(
        &self,
        task: &TaskRecord,
        plan: &StoryQueryPlan,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        plan.validate()?;
        let task = self.authorize_task_capability(
            &task.project_id,
            &task.task_id,
            &task.thread_id,
            task.input_revision,
            &task.capability_hash,
            now,
        )?;
        let query_scope = crate::query_scope::TaskQueryScope::from_task(self, &task)?;
        self.assert_task_inputs_current(&task)?;
        if plan.steps.iter().any(|step| {
            matches!(
                &step.operation,
                QueryOperation::ResolveEntity { .. }
                    | QueryOperation::State { .. }
                    | QueryOperation::Relations { .. }
            )
        }) && !self.task_source_projection_aligned(&task)?
        {
            return Err(RepositoryError::InvalidContent(
                "selected source differs from the modeled draft; structured knowledge is unavailable until that source is modeled".into(),
            ));
        }
        let head = self
            .branch_head(
                &task.project_id,
                &BranchId::parse("main").map_err(|_| RepositoryError::CorruptState)?,
            )?
            .ok_or(RepositoryError::CorruptState)?;
        let watermarks = self.task_query_source_watermarks(&task)?;
        let mut steps = Vec::new();
        for step in &plan.steps {
            let value = if let crate::query_scope::TaskQueryScope::Bounded(scope) = &query_scope {
                match &step.operation {
                    QueryOperation::Passages {
                        cursor,
                        max_chars,
                        limit,
                    } => crate::query_passages::scan(
                        self,
                        &task,
                        &query_scope,
                        crate::query_passages::PassageQuery {
                            question: &plan.question,
                            step_id: &step.id,
                            cursor: cursor.as_deref(),
                            max_chars: *max_chars,
                            limit: *limit,
                        },
                    )?,
                    QueryOperation::Text { query, limit } => {
                        let mut value = self.search_documents_for_task(
                            &task,
                            SearchQuery {
                                project_id: &task.project_id,
                                snapshot_id: None,
                                text: if scope.pov.is_some() { "" } else { query },
                                kind: Some("draft"),
                                scope: scope.text_scope(),
                                limit: *limit,
                                cursor: None,
                            },
                        )?;
                        if scope.pov.is_some() {
                            value["coverage"]["complete"] = json!(false);
                            value["coverage"]["povTextUnverified"] = json!(true);
                        }
                        value
                    }
                    QueryOperation::ResolveEntity { name } => scope.resolve(self, &task, name)?,
                    QueryOperation::State { .. } => scope.state(self, &task, &step.operation)?,
                    QueryOperation::Relations {
                        entity_id,
                        relation_type,
                        direction,
                        world_time,
                        max_hops,
                        limit,
                    } => {
                        if (scope.pov.is_some() || scope.story_time.is_some())
                            && world_time.is_some_and(|at| Some(at) != scope.story_time)
                        {
                            return Err(RepositoryError::TaskOutputForbidden);
                        }
                        self.query_relation_graph(RelationGraphQuery {
                            project_id: &task.project_id,
                            scope: scope.pov.as_deref().map_or(
                                RelationGraphScope::WriterBeforeChapter(
                                    scope.structured_through + 1,
                                ),
                                |character_id| RelationGraphScope::WriterPov {
                                    chapter: scope.structured_through + 1,
                                    character_id,
                                },
                            ),
                            entity_id,
                            relation_type: relation_type.as_deref(),
                            direction: match direction {
                                QueryDirection::Outgoing => RelationDirection::Outgoing,
                                QueryDirection::Incoming => RelationDirection::Incoming,
                                QueryDirection::Both => RelationDirection::Both,
                            },
                            world_time: scope.story_time.or(*world_time),
                            max_hops: *max_hops,
                            limit: *limit,
                        })?
                    }
                }
            } else {
                match &step.operation {
                    QueryOperation::Passages {
                        cursor,
                        max_chars,
                        limit,
                    } => crate::query_passages::scan(
                        self,
                        &task,
                        &query_scope,
                        crate::query_passages::PassageQuery {
                            question: &plan.question,
                            step_id: &step.id,
                            cursor: cursor.as_deref(),
                            max_chars: *max_chars,
                            limit: *limit,
                        },
                    )?,
                    QueryOperation::Text { query, limit } => self.search_documents_for_task(
                        &task,
                        SearchQuery {
                            project_id: &task.project_id,
                            snapshot_id: None,
                            text: query,
                            kind: Some("draft"),
                            scope: SearchScope::All,
                            limit: *limit,
                            cursor: None,
                        },
                    )?,
                    QueryOperation::ResolveEntity { name } => {
                        self.resolve_entity_name(&task.project_id, name, 9_000)?
                    }
                    QueryOperation::State {
                        entity_id,
                        predicate,
                        perspective,
                        owner_id,
                        reading_chapter,
                        world_time,
                        epistemic_time,
                    } => self.knowledge_state_in_snapshot(KnowledgeStateQuery {
                        project_id: &task.project_id,
                        snapshot_id: None,
                        perspective: match perspective {
                            QueryPerspective::Author => KnowledgePerspective::Author,
                            QueryPerspective::World => KnowledgePerspective::World,
                            QueryPerspective::Character => KnowledgePerspective::Character,
                            QueryPerspective::Reader => KnowledgePerspective::Reader,
                        },
                        entity_id: Some(entity_id),
                        predicate: predicate.as_deref(),
                        owner_id: owner_id.as_deref(),
                        reading_chapter: *reading_chapter,
                        world_time: *world_time,
                        epistemic_time: *epistemic_time,
                    })?,
                    QueryOperation::Relations {
                        entity_id,
                        relation_type,
                        direction,
                        world_time,
                        max_hops,
                        limit,
                    } => self.query_relation_graph(RelationGraphQuery {
                        project_id: &task.project_id,
                        scope: RelationGraphScope::Current,
                        entity_id,
                        relation_type: relation_type.as_deref(),
                        direction: match direction {
                            QueryDirection::Outgoing => RelationDirection::Outgoing,
                            QueryDirection::Incoming => RelationDirection::Incoming,
                            QueryDirection::Both => RelationDirection::Both,
                        },
                        world_time: *world_time,
                        max_hops: *max_hops,
                        limit: *limit,
                    })?,
                }
            };
            steps.push(crate::query_report::step_report(step, value)?);
        }
        let policy = query_scope.policy();
        let policy_digest = format!("{:x}", Sha256::digest(policy.to_string().as_bytes()));
        let mut baseline_plan = plan.clone();
        baseline_plan.ranking = QueryRanking::Literal;
        let mut report = json!({"taskId":task.task_id,"projectId":task.project_id.as_str(),"snapshotId":head.snapshot_id.as_str(),
                "plan":baseline_plan,"policy":policy,"policyDigest":policy_digest,"sourceWatermarks":watermarks,"steps":steps,
                "coverage":{"complete":steps.iter().all(|step| step["coverage"]["complete"] == true),
                    "semanticCompleteness":"unknown","responseTruncated":false},"contentHash":""});
        crate::query_report::fit_report(&mut report, plan.byte_budget)?;
        if matches!(plan.ranking, QueryRanking::ModelSemantic { .. }) {
            crate::query_semantic::apply(&mut report, &plan.ranking)?;
            crate::query_report::bound_report(&mut report, plan.byte_budget)?;
        }
        Ok(report)
    }

    /// Source watermark for the query's authorized scope; never returns future object IDs.
    pub fn task_query_source_watermarks(
        &self,
        task: &TaskRecord,
    ) -> Result<Value, RepositoryError> {
        crate::query_scope::TaskQueryScope::from_task(self, task)?.watermarks(self, task)
    }
}
