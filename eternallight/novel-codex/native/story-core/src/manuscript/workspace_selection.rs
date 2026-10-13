use rusqlite::OptionalExtension;
use rusqlite::TransactionBehavior;
use rusqlite::params;

use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::SqliteStoryRepository;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingBaseline {
    pub object_kind: String,
    pub object_id: String,
    pub revision_id: Option<RevisionId>,
    pub selection_revision: i64,
    pub selected_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuationIntent {
    pub target_kind: String,
    pub target_id: String,
    pub working_revision_id: RevisionId,
    pub author_intent: String,
    pub status: String,
    pub task_id: Option<String>,
    pub updated_at: i64,
}



pub(crate) fn advance_task_working_source(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    task_id: &str,
    object_kind: &str,
    object_id: &str,
    revision_id: &RevisionId,
) -> Result<(), RepositoryError> {
    let captured: Option<(Option<String>, Option<i64>)> = connection.query_row(
        "SELECT working_revision_id,working_selection_revision FROM story_task_input_heads_v2 WHERE task_id=?1 AND object_id=?2",
        params![task_id,object_id],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?;
    let Some((Some(previous), Some(version))) = captured else {
        return Ok(());
    };
    let baseline_object = if object_kind == "draft" {
        object_id
            .strip_prefix("draft-")
            .ok_or(RepositoryError::CorruptState)?
    } else if matches!(object_kind, "brief" | "outline" | "plan") {
        object_kind
    } else {
        return Err(RepositoryError::CorruptState);
    };
    let changed = connection.execute(
        "UPDATE story_working_baselines_v2 SET revision_id=?1,selection_revision=selection_revision+1,selected_at=CAST(strftime('%s','now') AS INTEGER)
         WHERE project_id=?2 AND object_kind=?3 AND object_id=?4 AND revision_id=?5 AND selection_revision=?6",
        params![revision_id.as_str(),project_id.as_str(),object_kind,baseline_object,previous,version],
    ).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::TaskInputStale);
    }
    let next_version = version
        .checked_add(1)
        .ok_or(RepositoryError::CorruptState)?;
    let captured = connection.execute(
        "UPDATE story_task_input_heads_v2 SET working_revision_id=?1,working_selection_revision=?2
         WHERE task_id=?3 AND object_id=?4 AND working_revision_id=?5 AND working_selection_revision=?6",
        params![revision_id.as_str(),next_version,task_id,object_id,previous,version],
    ).map_err(storage_error)?;
    if captured != 1 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(())
}

/// The CAS parent tracks the newest candidate. The creative base can instead be
/// an author-selected historical or accepted revision and must be recorded apart.
pub(crate) fn task_output_working_base(
    connection: &rusqlite::Connection,
    task_id: &str,
    object_id: &str,
    candidate_parent: Option<&RevisionId>,
) -> Result<Option<RevisionId>, RepositoryError> {
    let task = crate::tasks::load_task(connection, task_id)?;
    if crate::query_scope::uses_candidate_source(&task)? {
        return Ok(candidate_parent.cloned());
    }
    let captured: Option<(Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT working_revision_id,accepted_revision_id FROM story_task_input_heads_v2
         WHERE task_id=?1 AND object_id=?2",
            params![task_id, object_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let source = captured
        .and_then(|(working, accepted)| working.or(accepted))
        .map(RevisionId::parse)
        .transpose()
        .map_err(|_| RepositoryError::CorruptState)
        .map(|source| source.or_else(|| candidate_parent.cloned()))?;
    if let Some(source) = source.as_ref() {
        let belongs = connection.query_row(
            "SELECT 1 FROM story_revisions_v2 WHERE revision_id=?1 AND project_id=?2 AND object_id=?3",
            params![source.as_str(),task.project_id.as_str(),object_id],
            |_| Ok(()),
        ).optional().map_err(storage_error)?.is_some();
        if !belongs {
            return Err(RepositoryError::CorruptState);
        }
    }
    Ok(source)
}

pub(crate) fn advance_author_working_source(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_kind: &str,
    object_id: &str,
    previous_revision_id: Option<&RevisionId>,
    revision_id: &RevisionId,
) -> Result<(), RepositoryError> {
    let Some(previous) = previous_revision_id else {
        return Ok(());
    };
    let baseline_object = if object_kind == "draft" {
        object_id
            .strip_prefix("draft-")
            .ok_or(RepositoryError::CorruptState)?
    } else if matches!(object_kind, "brief" | "outline" | "plan") {
        object_kind
    } else {
        return Ok(());
    };
    connection.execute(
        "UPDATE story_working_baselines_v2 SET revision_id=?1,selection_revision=selection_revision+1,selected_at=CAST(strftime('%s','now') AS INTEGER)
         WHERE project_id=?2 AND object_kind=?3 AND object_id=?4 AND revision_id=?5",
        params![revision_id.as_str(),project_id.as_str(),object_kind,baseline_object,previous.as_str()],
    ).map_err(storage_error)?;
    Ok(())
}

fn validate_target(kind: &str, object_id: &str) -> Result<(), RepositoryError> {
    if !matches!(kind, "brief" | "outline" | "plan" | "draft")
        || object_id.trim().is_empty()
        || (kind != "draft" && object_id != kind)
    {
        return Err(RepositoryError::InvalidObjectKind);
    }
    Ok(())
}

fn selection_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    kind: &str,
    object_id: &str,
) -> Result<Option<i64>, RepositoryError> {
    connection.query_row(
        "SELECT selection_revision FROM story_working_baselines_v2 WHERE project_id = ?1 AND object_kind = ?2 AND object_id = ?3",
        params![project_id.as_str(), kind, object_id], |row| row.get(0),
    ).optional().map_err(storage_error)
}

fn baseline_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkingBaseline> {
    let project_id = row.get::<_, String>(0)?;
    let object_kind = row.get::<_, String>(1)?;
    let object_id = row.get::<_, String>(2)?;
    let revision = row.get::<_, Option<String>>(3)?;
    if revision.is_some() {
        let expected_object = if object_kind == "draft" {
            format!("draft-{object_id}")
        } else {
            format!("artifact-{object_kind}")
        };
        if row.get::<_, Option<String>>(6)?.as_deref() != Some(project_id.as_str())
            || row.get::<_, Option<String>>(7)?.as_deref() != Some(object_kind.as_str())
            || row.get::<_, Option<String>>(8)?.as_deref() != Some(expected_object.as_str())
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
    }
    Ok(WorkingBaseline {
        object_kind,
        object_id,
        revision_id: revision
            .map(|value| RevisionId::parse(value).map_err(|_| rusqlite::Error::InvalidQuery))
            .transpose()?,
        selection_revision: row.get(4)?,
        selected_at: row.get(5)?,
    })
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public workspace selection capability; SQLite retains transaction ownership.
pub trait WorkspaceSelectionRepository {
    fn working_baselines(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<WorkingBaseline>, RepositoryError>;
    fn continuation_intents(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ContinuationIntent>, RepositoryError>;
    fn set_working_baseline(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        revision_id: &RevisionId,
        expected_selection_revision: i64,
        now: i64,
    ) -> Result<WorkingBaseline, RepositoryError>;
    fn clear_working_baseline(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        expected_selection_revision: i64,
        now: i64,
    ) -> Result<WorkingBaseline, RepositoryError>;
    fn save_continuation_intent(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        working_revision_id: &RevisionId,
        author_intent: &str,
        now: i64,
    ) -> Result<ContinuationIntent, RepositoryError>;
}

impl WorkspaceSelectionRepository for SqliteStoryRepository {
    fn working_baselines(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<WorkingBaseline>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT b.project_id,b.object_kind,b.object_id,b.revision_id,b.selection_revision,b.selected_at,
                    r.project_id,r.object_kind,r.object_id
             FROM story_working_baselines_v2 b LEFT JOIN story_revisions_v2 r ON r.revision_id=b.revision_id
             WHERE b.project_id=?1 ORDER BY b.object_kind,b.object_id",
        ).map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], baseline_from_row)
            .map_err(storage_error)?
            .map(|row| {
                row.map_err(|error| match error {
                    rusqlite::Error::InvalidQuery => RepositoryError::CorruptState,
                    other => storage_error(other),
                })
            })
            .collect()
    }

    fn continuation_intents(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ContinuationIntent>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT target_kind, target_id, working_revision_id, author_intent, status, task_id, updated_at FROM story_continuation_intents_v2 WHERE project_id = ?1 ORDER BY updated_at DESC",
        ).map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .map_err(storage_error)?
            .map(|row| {
                let (target_kind, target_id, revision, author_intent, status, task_id, updated_at) =
                    row.map_err(storage_error)?;
                Ok(ContinuationIntent {
                    target_kind,
                    target_id,
                    working_revision_id: RevisionId::parse(revision)
                        .map_err(|_| RepositoryError::CorruptState)?,
                    author_intent,
                    status,
                    task_id,
                    updated_at,
                })
            })
            .collect()
    }

    fn set_working_baseline(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        revision_id: &RevisionId,
        expected_selection_revision: i64,
        now: i64,
    ) -> Result<WorkingBaseline, RepositoryError> {
        validate_target(object_kind, object_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let revision_object = transaction
            .query_row(
                "SELECT object_kind,object_id FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
                params![project_id.as_str(), revision_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?)),
            )
            .optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
        let expected_object = if object_kind == "draft" {
            format!("draft-{object_id}")
        } else {
            format!("artifact-{object_kind}")
        };
        if revision_object != (object_kind.to_owned(), expected_object) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let current =
            selection_revision(&transaction, project_id, object_kind, object_id)?.unwrap_or(0);
        if current != expected_selection_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: expected_selection_revision,
                actual: current,
            });
        }
        let next = current
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        transaction.execute(
            "INSERT INTO story_working_baselines_v2(project_id, object_kind, object_id, revision_id, selection_revision, selected_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(project_id, object_kind, object_id) DO UPDATE SET revision_id = excluded.revision_id, selection_revision = excluded.selection_revision, selected_at = excluded.selected_at",
            params![project_id.as_str(), object_kind, object_id, revision_id.as_str(), next, now],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(WorkingBaseline {
            object_kind: object_kind.into(),
            object_id: object_id.into(),
            revision_id: Some(revision_id.clone()),
            selection_revision: next,
            selected_at: now,
        })
    }

    fn clear_working_baseline(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        expected_selection_revision: i64,
        now: i64,
    ) -> Result<WorkingBaseline, RepositoryError> {
        validate_target(object_kind, object_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let current = selection_revision(&transaction, project_id, object_kind, object_id)?
            .ok_or(RepositoryError::CandidateNotFound)?;
        if current != expected_selection_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: expected_selection_revision,
                actual: current,
            });
        }
        let next = current
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        transaction.execute(
            "UPDATE story_working_baselines_v2 SET revision_id = NULL, selection_revision = ?1, selected_at = ?2 WHERE project_id = ?3 AND object_kind = ?4 AND object_id = ?5",
            params![next, now, project_id.as_str(), object_kind, object_id],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(WorkingBaseline {
            object_kind: object_kind.into(),
            object_id: object_id.into(),
            revision_id: None,
            selection_revision: next,
            selected_at: now,
        })
    }

    fn save_continuation_intent(
        &mut self,
        project_id: &ProjectId,
        object_kind: &str,
        object_id: &str,
        working_revision_id: &RevisionId,
        author_intent: &str,
        now: i64,
    ) -> Result<ContinuationIntent, RepositoryError> {
        validate_target(object_kind, object_id)?;
        let intent = author_intent.trim();
        if intent.is_empty() || intent.len() > 64_000 {
            return Err(RepositoryError::InvalidContent(
                "invalid author intent".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let selected = transaction.query_row(
            "SELECT revision_id FROM story_working_baselines_v2 WHERE project_id = ?1 AND object_kind = ?2 AND object_id = ?3",
            params![project_id.as_str(), object_kind, object_id],
            |row| row.get::<_, Option<String>>(0),
        ).optional().map_err(storage_error)?.flatten();
        if selected.as_deref() != Some(working_revision_id.as_str()) {
            return Err(RepositoryError::StaleCandidate {
                expected: Some(working_revision_id.clone()),
                actual: selected
                    .map(RevisionId::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptState)?,
            });
        }
        transaction.execute(
            "INSERT INTO story_continuation_intents_v2(project_id, target_kind, target_id, working_revision_id, author_intent, status, task_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'ready', NULL, ?6) ON CONFLICT(project_id, target_kind, target_id) DO UPDATE SET working_revision_id = excluded.working_revision_id, author_intent = excluded.author_intent, status = 'ready', task_id = NULL, updated_at = excluded.updated_at",
            params![project_id.as_str(), object_kind, object_id, working_revision_id.as_str(), intent, now],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(ContinuationIntent {
            target_kind: object_kind.into(),
            target_id: object_id.into(),
            working_revision_id: working_revision_id.clone(),
            author_intent: intent.into(),
            status: "ready".into(),
            task_id: None,
            updated_at: now,
        })
    }
}
