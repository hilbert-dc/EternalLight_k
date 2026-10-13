use std::collections::HashSet;

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::knowledge_projection::StateVisibility;
use crate::query_plan::{QueryOperation, QueryPerspective};
use crate::{
    KnowledgePerspective, KnowledgeStateQuery, RepositoryError, SearchScope, SqliteStoryRepository,
    TaskRecord, WriterKnowledgeScope,
};

pub(crate) struct BoundedQueryScope {
    pub chapter: i64,
    pub end_scene: Option<usize>,
    pub pov: Option<String>,
    pub story_time: Option<i64>,
    pub structured_through: i64,
    pub text_scope: SearchScope,
    pub scope_name: &'static str,
    pub source_revision_id: Option<String>,
}

impl BoundedQueryScope {
    pub fn from_task(
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
    ) -> Result<Self, RepositoryError> {
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        let chapter_id = target["chapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let chapter: i64 = repo
            .connection
            .query_row(
                "SELECT number FROM story_chapters_v2 WHERE project_id = ?1 AND chapter_id = ?2",
                params![task.project_id.as_str(), chapter_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        if !(1..i64::MAX).contains(&chapter)
            || ["chapterNumber", "readingChapter"].iter().any(|key| {
                target
                    .get(key)
                    .is_some_and(|value| value.as_i64() != Some(chapter))
            })
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let pov = match target.get("povCharacterId") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 160)
                    .ok_or(RepositoryError::TaskBindingInvalid)?
                    .to_owned(),
            ),
        };
        let story_time = match target.get("storyTime") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_i64().ok_or(RepositoryError::TaskBindingInvalid)?),
        };
        let end_scene = crate::task_scope::reader_scene_limit(&target)?;
        Ok(Self {
            chapter,
            end_scene,
            pov,
            story_time,
            structured_through: chapter - 1,
            text_scope: end_scene.map_or(SearchScope::ThroughChapter(chapter), |end_scene_order| {
                SearchScope::ThroughScene {
                    chapter,
                    end_scene_order,
                }
            }),
            scope_name: "writer",
            source_revision_id: None,
        })
    }

    pub fn policy(&self) -> Value {
        let text_scope = match self.text_scope {
            SearchScope::All => json!({"type":"all"}),
            SearchScope::Chapter(chapter) => json!({"type":"chapter", "chapterNumber":chapter}),
            SearchScope::ThroughChapter(chapter) => {
                json!({"type":"throughChapter", "chapterNumber":chapter})
            }
            SearchScope::ThroughScene {
                chapter,
                end_scene_order,
            } => {
                json!({"type":"throughScene", "chapterNumber":chapter, "endSceneOrder":end_scene_order})
            }
        };
        json!({"scope":self.scope_name, "readingChapter":self.chapter, "structuredThroughChapter":self.structured_through,
            "textScope":text_scope,
            "sourceRevisionId":self.source_revision_id,
            "endSceneOrder":self.end_scene, "povCharacterId":self.pov, "storyTime":self.story_time,
            "identityPolicy":"rawSourceClaims", "confirmationPolicy":"candidateOnly"})
    }

    pub fn text_scope(&self) -> SearchScope {
        self.text_scope
    }

    pub fn watermarks(
        &self,
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
    ) -> Result<Value, RepositoryError> {
        let through = match self.text_scope {
            SearchScope::All => return Err(RepositoryError::TaskBindingInvalid),
            SearchScope::Chapter(chapter)
            | SearchScope::ThroughChapter(chapter)
            | SearchScope::ThroughScene { chapter, .. } => chapter,
        };
        let mut statement = repo.connection.prepare(
            "SELECT 'draft-' || chapter_id FROM story_chapters_v2 WHERE project_id = ?1 AND number <= ?2"
        ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut allowed = statement
            .query_map(params![task.project_id.as_str(), through], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        allowed.insert("artifact-plan".into());
        if self.scope_name != "writer" {
            allowed.extend(["artifact-brief".to_owned(), "artifact-outline".to_owned()]);
        }
        repo.writer_source_watermarks(&task.project_id, &allowed)
    }

    pub fn resolve(
        &self,
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
        name: &str,
    ) -> Result<Value, RepositoryError> {
        let scope = self.pov.as_deref().map_or(
            WriterKnowledgeScope::PriorChapters(self.structured_through + 1),
            |character_id| WriterKnowledgeScope::CharacterPov {
                chapter: self.structured_through + 1,
                character_id,
                at_story_time: self.story_time,
            },
        );
        let found = repo.query_writer_knowledge_by_name(&task.project_id, scope, name, None, 1)?;
        let mut entries = Vec::new();
        for candidate in found["candidates"].as_array().into_iter().flatten() {
            for claim in candidate["claims"].as_array().into_iter().flatten() {
                let mut claim = claim.clone();
                claim["id"] = candidate["id"].clone();
                claim["kind"] = candidate["kind"].clone();
                claim["confirmationState"] = json!("candidate");
                entries.push(claim);
            }
        }
        Ok(json!({"entries":entries, "identityRevisionId":null,
            "truncated":self.pov.is_some() || found["coverage"]["catalogTruncated"] == true || found["coverage"]["claimsTruncated"] == true
                || found["coverage"]["totalSources"] != found["coverage"]["modeledSources"],
            "ambiguousNames":if found["coverage"]["requiresDisambiguation"] == true { vec![name] } else { Vec::new() }}))
    }

    pub fn state(
        &self,
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
        operation: &QueryOperation,
    ) -> Result<Value, RepositoryError> {
        let QueryOperation::State {
            entity_id,
            predicate,
            perspective,
            owner_id,
            reading_chapter,
            world_time,
            epistemic_time,
        } = operation
        else {
            return Err(RepositoryError::TaskBindingInvalid);
        };
        if matches!(perspective, QueryPerspective::Author)
            || reading_chapter.is_some_and(|position| position != self.chapter)
            || (self.story_time.is_some()
                && [*world_time, *epistemic_time]
                    .into_iter()
                    .flatten()
                    .any(|at| Some(at) != self.story_time))
            || (self.pov.is_some()
                && (!matches!(perspective, QueryPerspective::Character)
                    || owner_id
                        .as_ref()
                        .is_some_and(|owner| Some(owner) != self.pov.as_ref())
                    || epistemic_time.is_some_and(|at| Some(at) != self.story_time)))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if self.pov.is_some() && self.story_time.is_none() {
            return Ok(json!({"entries":[], "coverage":{"complete":false,"povTimeUnbound":true}}));
        }
        self.project_state(
            repo,
            KnowledgeStateQuery {
                project_id: &task.project_id,
                snapshot_id: None,
                entity_id: Some(entity_id),
                predicate: predicate.as_deref(),
                perspective: match perspective {
                    QueryPerspective::World => KnowledgePerspective::World,
                    QueryPerspective::Character => KnowledgePerspective::Character,
                    QueryPerspective::Reader => KnowledgePerspective::Reader,
                    QueryPerspective::Author => return Err(RepositoryError::TaskOutputForbidden),
                },
                owner_id: self.pov.as_deref().or(owner_id.as_deref()),
                reading_chapter: Some(self.structured_through),
                world_time: if matches!(perspective, QueryPerspective::Character) {
                    *world_time
                } else {
                    self.story_time.or(*world_time)
                },
                epistemic_time: if matches!(perspective, QueryPerspective::Character) {
                    self.story_time.or(*epistemic_time)
                } else {
                    *epistemic_time
                },
            },
        )
    }

    pub fn context_state(
        &self,
        repo: &SqliteStoryRepository,
        task: &TaskRecord,
    ) -> Result<Value, RepositoryError> {
        if self.pov.is_some() && self.story_time.is_none() {
            return Ok(json!({"entries":[], "coverage":{"complete":false,"povTimeUnbound":true}}));
        }
        self.project_state(
            repo,
            KnowledgeStateQuery {
                project_id: &task.project_id,
                snapshot_id: None,
                entity_id: None,
                predicate: None,
                perspective: if self.pov.is_some() {
                    KnowledgePerspective::Character
                } else {
                    KnowledgePerspective::Reader
                },
                owner_id: self.pov.as_deref(),
                reading_chapter: Some(self.structured_through),
                world_time: if self.pov.is_some() {
                    None
                } else {
                    self.story_time
                },
                epistemic_time: if self.pov.is_some() {
                    self.story_time
                } else {
                    None
                },
            },
        )
    }

    fn project_state(
        &self,
        repo: &SqliteStoryRepository,
        query: KnowledgeStateQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        let mut result = repo.knowledge_state_with_visibility(
            query,
            StateVisibility::ThroughChapter(self.structured_through),
        )?;
        if self.pov.is_some() {
            // Do not expose counts or missing-source diagnostics for withheld POV claims.
            result["coverage"] = json!({"complete":false,"semanticCompleteness":"unknown","povEvidenceUnverified":true});
        }
        Ok(result)
    }
}

use crate::{TaskInputRepository, TaskKnowledgeSearchRepository};
