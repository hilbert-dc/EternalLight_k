use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{ProjectId, RepositoryError, RevisionId, SqliteStoryRepository};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_catalogs_v1 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK(revision >= 0)
);
CREATE TABLE IF NOT EXISTS story_volumes_v1 (
  volume_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  title TEXT NOT NULL, position INTEGER NOT NULL,
  source_outline_revision_id TEXT REFERENCES story_revisions_v2(revision_id), source_node_id TEXT,
  CHECK((source_outline_revision_id IS NULL) = (source_node_id IS NULL)),
  UNIQUE(project_id, volume_id), UNIQUE(project_id, position)
);
CREATE UNIQUE INDEX IF NOT EXISTS story_volume_outline_node_v1 ON story_volumes_v1(project_id,source_node_id) WHERE source_node_id IS NOT NULL;
CREATE TABLE IF NOT EXISTS story_chapter_placements_v1 (
  project_id TEXT NOT NULL, chapter_id TEXT NOT NULL REFERENCES story_chapters_v2(chapter_id) ON DELETE CASCADE,
  volume_id TEXT NOT NULL,
  PRIMARY KEY(project_id, chapter_id),
  FOREIGN KEY(project_id, volume_id) REFERENCES story_volumes_v1(project_id, volume_id) ON DELETE CASCADE
);
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VolumeSource {
    pub outline_revision_id: String,
    pub node_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptVolume {
    pub id: String,
    pub title: String,
    pub position: i64,
    pub source: Option<VolumeSource>,
    pub outline_status: OutlineStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutlineStatus {
    Linked,
    Unavailable,
    Independent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterPlacement {
    pub chapter_id: String,
    pub volume_id: String,
}

#[derive(Serialize)]
pub struct ManuscriptCatalog {
    pub revision: i64,
    pub volumes: Vec<ManuscriptVolume>,
    pub placements: Vec<ChapterPlacement>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum CatalogChapterDestination {
    Volume(String),
    Unassigned(()),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum CatalogOperation {
    CreateVolume {
        title: String,
        source: Option<VolumeSource>,
    },
    LinkOutline {
        #[serde(rename = "volumeId")]
        volume_id: String,
        source: VolumeSource,
    },
    PlaceChapter {
        #[serde(rename = "chapterId")]
        chapter_id: String,
        #[serde(rename = "volumeId")]
        volume_id: CatalogChapterDestination,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogRequest {
    pub expected_revision: i64,
    pub idempotency_key: String,
    pub operation: CatalogOperation,
}

impl SqliteStoryRepository {
    pub fn manuscript_catalog(
        &self,
        project: &ProjectId,
    ) -> Result<ManuscriptCatalog, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let catalog = read_catalog(&read, project)?;
        read.commit().map_err(storage_error)?;
        Ok(catalog)
    }

    pub fn update_manuscript_catalog(
        &mut self,
        project: &ProjectId,
        request: &CatalogRequest,
    ) -> Result<Value, RepositoryError> {
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_string(request)
                    .map_err(|_| RepositoryError::CorruptState)?
                    .as_bytes()
            )
        );
        RevisionId::parse(request.idempotency_key.clone())
            .map_err(|_| invalid("invalid_catalog_key"))?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let replay: Option<(String, String)> = tx.query_row(
            "SELECT request_digest,result_json FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation='catalog' AND idempotency_key=?2",
            params![project.as_str(), request.idempotency_key], |row| Ok((row.get(/*idx*/ 0)?, row.get(/*idx*/ 1)?)),
        ).optional().map_err(storage_error)?;
        if let Some((stored_digest, result)) = replay {
            if stored_digest != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return serde_json::from_str(&result).map_err(|_| RepositoryError::CorruptState);
        }
        let before = read_catalog(&tx, project)?;
        if before.revision != request.expected_revision {
            return Err(invalid("catalog_revision_changed"));
        }
        assert_writable(&tx, project)?;
        let mut volume_id = None;
        match &request.operation {
            CatalogOperation::CreateVolume { title, source } => {
                if before.volumes.len() >= 100 {
                    return Err(invalid("catalog_volume_limit"));
                }
                let title = match source {
                    Some(source) => {
                        let title = source_title(&tx, project, source)?;
                        assert_unbound_node(
                            &tx,
                            project,
                            &source.node_id,
                            /*excluding_volume*/ None,
                        )?;
                        title
                    }
                    None => title.clone(),
                };
                if title.trim().is_empty() || title.chars().count() > 160 {
                    return Err(invalid("invalid_volume_text"));
                }
                let id = format!("volume-{}", uuid::Uuid::new_v4().simple());
                tx.execute("INSERT INTO story_volumes_v1(volume_id,project_id,title,position,source_outline_revision_id,source_node_id) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![id,project.as_str(),title,before.volumes.len(),source.as_ref().map(|s| &s.outline_revision_id),source.as_ref().map(|s| &s.node_id)]).map_err(storage_error)?;
                volume_id = Some(id);
            }
            CatalogOperation::LinkOutline { volume_id, source } => {
                RevisionId::parse(volume_id.clone())
                    .map_err(|_| invalid("invalid_catalog_volume"))?;
                if !before.volumes.iter().any(|volume| volume.id == *volume_id) {
                    return Err(invalid("catalog_volume_not_found"));
                }
                let title = source_title(&tx, project, source)?;
                if title.trim().is_empty() || title.chars().count() > 160 {
                    return Err(invalid("invalid_volume_text"));
                }
                assert_unbound_node(&tx, project, &source.node_id, Some(volume_id))?;
                tx.execute("UPDATE story_volumes_v1 SET source_outline_revision_id=?3,source_node_id=?4 WHERE project_id=?1 AND volume_id=?2",
                    params![project.as_str(),volume_id,source.outline_revision_id,source.node_id]).map_err(storage_error)?;
            }
            CatalogOperation::PlaceChapter {
                chapter_id,
                volume_id,
            } => {
                RevisionId::parse(chapter_id.clone())
                    .map_err(|_| invalid("invalid_catalog_chapter"))?;
                let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_chapters_v2 WHERE project_id=?1 AND chapter_id=?2)",
                    params![project.as_str(),chapter_id], |row| row.get(/*idx*/ 0)).map_err(storage_error)?;
                if !exists {
                    return Err(invalid("catalog_chapter_not_found"));
                }
                match volume_id {
                    CatalogChapterDestination::Volume(id) => {
                        place_new_chapter(&tx, project, chapter_id, id)?
                    }
                    CatalogChapterDestination::Unassigned(()) => {
                        tx.execute("DELETE FROM story_chapter_placements_v1 WHERE project_id=?1 AND chapter_id=?2", params![project.as_str(),chapter_id]).map_err(storage_error)?;
                    }
                }
            }
        }
        advance_revision(&tx, project)?;
        let mut result = serde_json::json!({"catalog":read_catalog(&tx, project)?});
        if let Some(id) = volume_id {
            result["volumeId"] = Value::String(id);
        }
        tx.execute("INSERT INTO story_domain_receipts_v2(project_id,operation,idempotency_key,request_digest,result_json) VALUES(?1,'catalog',?2,?3,?4)",
            params![project.as_str(),request.idempotency_key,digest,result.to_string()]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

pub(crate) fn assert_writable(db: &Connection, project: &ProjectId) -> Result<(), RepositoryError> {
    let active: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 WHERE project_id=?1 AND status IN ('queued','running','waiting'))",
        [project.as_str()], |row| row.get(/*idx*/ 0)).map_err(storage_error)?;
    if active {
        return Err(invalid("catalog_task_active"));
    }
    Ok(())
}

pub(crate) fn place_new_chapter(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
    volume: &str,
) -> Result<(), RepositoryError> {
    RevisionId::parse(volume.to_owned()).map_err(|_| invalid("invalid_catalog_volume"))?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_volumes_v1 WHERE project_id=?1 AND volume_id=?2)",
            params![project.as_str(), volume],
            |row| row.get(/*idx*/ 0),
        )
        .map_err(storage_error)?;
    if !exists {
        return Err(invalid("catalog_volume_not_found"));
    }
    db.execute("INSERT INTO story_chapter_placements_v1(project_id,chapter_id,volume_id) VALUES(?1,?2,?3) ON CONFLICT(project_id,chapter_id) DO UPDATE SET volume_id=excluded.volume_id",
        params![project.as_str(),chapter,volume]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn advance_revision(
    db: &Connection,
    project: &ProjectId,
) -> Result<(), RepositoryError> {
    db.execute("INSERT INTO story_catalogs_v1(project_id,revision) VALUES(?1,1) ON CONFLICT(project_id) DO UPDATE SET revision=revision+1",[project.as_str()]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn read_catalog(
    db: &Connection,
    project: &ProjectId,
) -> Result<ManuscriptCatalog, RepositoryError> {
    let revision = db.query_row("SELECT COALESCE(c.revision,0) FROM story_projects_v2 p LEFT JOIN story_catalogs_v1 c ON c.project_id=p.project_id WHERE p.project_id=?1",
        [project.as_str()], |row| row.get(/*idx*/ 0)).optional().map_err(storage_error)?.ok_or(RepositoryError::ProjectNotFound)?;
    let current: Option<String> = db.query_row("SELECT r.content_json FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.object_id=h.object_id AND r.revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id) WHERE h.project_id=?1 AND h.object_id='artifact-outline' AND r.object_kind='outline'",
        [project.as_str()], |row| row.get(/*idx*/ 0)).optional().map_err(storage_error)?;
    let current: Option<Value> = current
        .map(|content| serde_json::from_str(&content))
        .transpose()
        .map_err(|_| RepositoryError::CorruptState)?;
    let mut statement = db.prepare("SELECT volume_id,title,position,source_outline_revision_id,source_node_id FROM story_volumes_v1 WHERE project_id=?1 ORDER BY position,volume_id").map_err(storage_error)?;
    let mut volumes = statement
        .query_map([project.as_str()], |row| {
            let revision: Option<String> = row.get(/*idx*/ 3)?;
            let node: Option<String> = row.get(/*idx*/ 4)?;
            Ok(ManuscriptVolume {
                id: row.get(/*idx*/ 0)?,
                title: row.get(/*idx*/ 1)?,
                position: row.get(/*idx*/ 2)?,
                source: revision
                    .zip(node)
                    .map(|(outline_revision_id, node_id)| VolumeSource {
                        outline_revision_id,
                        node_id,
                    }),
                outline_status: OutlineStatus::Independent,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    for volume in &mut volumes {
        let Some(source) = &volume.source else {
            continue;
        };
        let title = current
            .as_ref()
            .and_then(|value| unique_volume_title(value, &source.node_id));
        volume.outline_status = if let Some(title) = title {
            volume.title = title.to_owned();
            OutlineStatus::Linked
        } else {
            OutlineStatus::Unavailable
        };
    }
    let mut statement = db.prepare("SELECT p.chapter_id,p.volume_id FROM story_chapter_placements_v1 p JOIN story_chapters_v2 c ON c.project_id=p.project_id AND c.chapter_id=p.chapter_id WHERE p.project_id=?1 ORDER BY c.number,p.chapter_id").map_err(storage_error)?;
    let placements = statement
        .query_map([project.as_str()], |row| {
            Ok(ChapterPlacement {
                chapter_id: row.get(/*idx*/ 0)?,
                volume_id: row.get(/*idx*/ 1)?,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    Ok(ManuscriptCatalog {
        revision,
        volumes,
        placements,
    })
}

fn source_title(
    db: &Connection,
    project: &ProjectId,
    source: &VolumeSource,
) -> Result<String, RepositoryError> {
    RevisionId::parse(source.outline_revision_id.clone())
        .map_err(|_| invalid("invalid_volume_source"))?;
    RevisionId::parse(source.node_id.clone()).map_err(|_| invalid("invalid_volume_source"))?;
    let content: String = db.query_row("SELECT r.content_json FROM story_revisions_v2 r JOIN story_object_heads_v2 h ON h.project_id=r.project_id AND h.object_id=r.object_id WHERE r.project_id=?1 AND r.revision_id=?2 AND r.object_id='artifact-outline' AND r.object_kind='outline' AND r.revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id)",
        params![project.as_str(),source.outline_revision_id], |row| row.get(/*idx*/ 0)).optional().map_err(storage_error)?.ok_or_else(|| invalid("catalog_source_changed"))?;
    let value: Value = serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    unique_volume_title(&value, &source.node_id)
        .map(str::to_owned)
        .ok_or_else(|| invalid("invalid_volume_source"))
}

fn unique_volume_title<'a>(value: &'a Value, node_id: &str) -> Option<&'a str> {
    let nodes = value["payload"]["nodes"].as_array()?;
    let mut matched = nodes
        .iter()
        .filter(|node| node["id"].as_str().or_else(|| node["clientKey"].as_str()) == Some(node_id));
    let node = matched.next().filter(|node| node["kind"] == "volume")?;
    if matched.next().is_some() {
        return None;
    }
    node["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty() && title.chars().count() <= 160)
}

fn assert_unbound_node(
    db: &Connection,
    project: &ProjectId,
    node: &str,
    excluding_volume: Option<&str>,
) -> Result<(), RepositoryError> {
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_volumes_v1 WHERE project_id=?1 AND source_node_id=?2 AND (?3 IS NULL OR volume_id<>?3))",
        params![project.as_str(),node,excluding_volume], |row| row.get(/*idx*/ 0)).map_err(storage_error)?;
    if exists {
        return Err(invalid("catalog_node_already_linked"));
    }
    Ok(())
}

fn invalid(code: &str) -> RepositoryError {
    RepositoryError::InvalidContent(code.into())
}
fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
