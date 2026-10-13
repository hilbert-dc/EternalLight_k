use rusqlite::{Connection, params};

use crate::{ProjectId, RepositoryError};

// Only derived edges: source authority and caller visibility remain separately checked.
const EDGES: &str = "SELECT b.project_id,b.batch_id,json_extract(CASE WHEN a.type='object' THEN a.value ELSE '{}' END,'$.propositionSourceBatchId') AS source_batch_id \
 FROM story_modeling_batches_v2 b,json_each(CASE WHEN json_valid(b.assertions_json) THEN b.assertions_json ELSE '[]' END) a WHERE b.published_at IS NOT NULL \
 AND json_type(CASE WHEN a.type='object' THEN a.value ELSE '{}' END,'$.propositionSourceBatchId')='text' \
 UNION SELECT b.project_id,b.batch_id,json_extract(CASE WHEN p.type='object' THEN p.value ELSE '{}' END,'$.sourceBatchId') \
 FROM story_modeling_batches_v2 b,json_each(CASE WHEN json_valid(b.assertions_json) THEN b.assertions_json ELSE '[]' END) a,json_each(CASE WHEN a.type='object' THEN a.value ELSE '{}' END,'$.value.preconditions') p \
 WHERE b.published_at IS NOT NULL AND json_extract(CASE WHEN a.type='object' THEN a.value ELSE '{}' END,'$.kind')='event' AND json_type(CASE WHEN p.type='object' THEN p.value ELSE '{}' END,'$.sourceBatchId')='text'";

pub(crate) fn schema() -> String {
    let mut schema = String::from(
        "CREATE TABLE IF NOT EXISTS story_model_dependencies_v1 (project_id TEXT NOT NULL,batch_id TEXT NOT NULL,source_batch_id TEXT NOT NULL,PRIMARY KEY(project_id,batch_id,source_batch_id)); CREATE INDEX IF NOT EXISTS story_model_dependencies_source_v1 ON story_model_dependencies_v1(project_id,source_batch_id,batch_id);",
    );
    for (name, action, row) in [
        ("insert", "INSERT", "NEW"),
        ("update", "UPDATE OF assertions_json,published_at", "NEW"),
        ("delete", "DELETE", "OLD"),
    ] {
        let populate = if name == "delete" {
            String::new()
        } else {
            format!(
                "INSERT INTO story_model_dependencies_v1 SELECT * FROM ({EDGES}) WHERE project_id=NEW.project_id AND batch_id=NEW.batch_id;"
            )
        };
        let changed = if name == "update" {
            "WHEN OLD.assertions_json IS NOT NEW.assertions_json OR OLD.published_at IS NOT NEW.published_at"
        } else {
            ""
        };
        schema.push_str(&format!("CREATE TRIGGER IF NOT EXISTS story_model_dependencies_{name}_v1 AFTER {action} ON story_modeling_batches_v2 {changed} BEGIN DELETE FROM story_model_dependencies_v1 WHERE project_id={row}.project_id AND batch_id={row}.batch_id; {populate} END;"));
    }
    schema
}

pub(crate) fn differences(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
) -> Result<(usize, usize), RepositoryError> {
    let expected =
        format!("SELECT source_batch_id FROM ({EDGES}) WHERE project_id=?1 AND batch_id=?2");
    let actual = "SELECT source_batch_id FROM story_model_dependencies_v1 WHERE project_id=?1 AND batch_id=?2";
    let sql = format!(
        "SELECT (SELECT COUNT(*) FROM ({expected} EXCEPT {actual})),(SELECT COUNT(*) FROM ({actual} EXCEPT {expected}))"
    );
    connection
        .query_row(&sql, params![project_id.as_str(), batch_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(storage_error)
}

pub(crate) fn orphan_count(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<usize, RepositoryError> {
    connection.query_row("SELECT COUNT(*) FROM story_model_dependencies_v1 d WHERE d.project_id=?1 AND NOT EXISTS(SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id=d.project_id AND b.batch_id=d.batch_id AND b.published_at IS NOT NULL)",[project_id.as_str()],|row|row.get(0)).map_err(storage_error)
}

pub(crate) fn rebuild(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "DELETE FROM story_model_dependencies_v1 WHERE project_id=?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
    connection.execute(&format!("INSERT INTO story_model_dependencies_v1 SELECT * FROM ({EDGES}) WHERE project_id=?1"),[project_id.as_str()]).map_err(storage_error)?;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
