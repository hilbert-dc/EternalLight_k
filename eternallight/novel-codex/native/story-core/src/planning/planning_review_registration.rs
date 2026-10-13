use crate::planning_reviews::{current_revision, json_error, load_run, storage_error};
use crate::{CreatePlanningReview, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) enum Origin<'a> {
    Direct,
    Task {
        work_id: String,
        task_id: &'a str,
        intent_revision: i64,
        focus: &'a str,
        history: &'a str,
        key: &'a str,
        hash: &'a str,
    },
}
impl<'a> Origin<'a> {
    fn history(&self, author: &'a str) -> &'a str {
        match self {
            Self::Direct => author,
            Self::Task { history, .. } => history,
        }
    }
    fn focus(&self) -> &'a str {
        match self {
            Self::Direct => "",
            Self::Task { focus, .. } => focus,
        }
    }
}

pub(crate) fn create(
    db: &Connection,
    request: &CreatePlanningReview<'_>,
    origin: Origin<'_>,
) -> Result<Value, RepositoryError> {
    let input_revision = current_revision(db, request.project_id)?;
    let content = db
            .query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2 AND object_id = ?3",
                params![request.project_id.as_str(), request.revision_id, format!("artifact-{}", request.artifact_kind)],
                |row| row.get::<_, String>(/*idx*/0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CandidateNotFound)?;
    let envelope: Value = serde_json::from_str(&content).map_err(json_error)?;
    let artifact = envelope.get("payload").cloned().unwrap_or(envelope);
    let entries = artifact
        .get(if request.artifact_kind == "outline" {
            "nodes"
        } else {
            "chapters"
        })
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let id_key = if request.artifact_kind == "outline" {
        "id"
    } else {
        "chapterPlanId"
    };
    let all_ids = entries
        .iter()
        .filter_map(|entry| entry.get(id_key).and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    let selected = match request.target_node_id {
        Some(target) if all_ids.iter().any(|id| id == target) => vec![target.to_owned()],
        Some(_) => {
            return Err(RepositoryError::InvalidContent(
                "planning review target is not in the revision".into(),
            ));
        }
        None => all_ids,
    };
    if selected.len() > 120 {
        return Err(RepositoryError::InvalidContent(
            "planning review scope exceeds 120 checks".into(),
        ));
    }
    let checks = request
        .roles
        .iter()
        .map(|role| {
            (
                role.clone(),
                Value::Array(
                    selected
                        .iter()
                        .map(|id| Value::String(format!("{role}:{id}")))
                        .collect(),
                ),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let packet = serde_json::json!({
        "artifactKind": request.artifact_kind, "revisionId": request.revision_id,
        "artifact": artifact, "scope": {"targetNodeId": request.target_node_id, "nodeIds": selected},
        "authorIntent": request.author_intent, "authorMessageId": request.author_message_id, "authorHistory": origin.history(request.author_intent), "focus": origin.focus()
    });
    let packet = packet.to_string();
    if packet.len() > crate::planning_review_author_source::MAX_PACKET_BYTES {
        return Err(RepositoryError::InvalidContent(
            "planning review source exceeds storage bound".into(),
        ));
    }
    let source_packet_hash = format!("{:x}", Sha256::digest(packet.as_bytes()));
    let (
        work_id,
        source_task_id,
        source_intent_revision,
        focus,
        _history,
        request_key,
        request_hash,
    ) = match origin {
        Origin::Direct => (
            crate::planning_review_work::create(db, request, input_revision)?,
            None,
            None,
            "",
            request.author_intent,
            None,
            None,
        ),
        Origin::Task {
            work_id,
            task_id,
            intent_revision,
            focus,
            history,
            key,
            hash,
        } => {
            crate::planning_review_work::append_seats(db, &work_id, request, input_revision)?;
            (
                work_id,
                Some(task_id),
                Some(intent_revision),
                focus,
                history,
                Some(key),
                Some(hash),
            )
        }
    };
    db.execute(
            "INSERT INTO story_planning_review_runs_v2(run_id, project_id, artifact_kind, revision_id, target_node_id, author_intent, input_revision, roles_json, source_packet_json, source_packet_hash, checks_json, status, created_at, updated_at, author_message_id, work_task_id, source_task_id, source_intent_revision, focus, request_key, request_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'open', ?12, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![request.run_id, request.project_id.as_str(), request.artifact_kind, request.revision_id, request.target_node_id, request.author_intent, input_revision, serde_json::to_string(request.roles).map_err(json_error)?, packet, source_packet_hash, Value::Object(checks).to_string(), request.now, request.author_message_id, work_id, source_task_id, source_intent_revision, focus, request_key, request_hash],
        ).map_err(storage_error)?;
    for role in request.roles {
        db.execute(
            "INSERT INTO story_planning_review_seats_v2(run_id, role) VALUES (?1, ?2)",
            params![request.run_id, role],
        )
        .map_err(storage_error)?;
    }
    if source_task_id.is_none() {
        db.execute("INSERT INTO story_author_messages_v1(message_id,project_id,work_task_id,review_run_id,intent_revision,author_text,created_at) VALUES (?1,?2,?3,?4,0,?5,?6)",
            params![request.author_message_id,request.project_id.as_str(),work_id,request.run_id,request.author_intent,request.now]).map_err(storage_error)?;
    }
    load_run(db, request.project_id, request.run_id)
}
