//! Edition scopes and typed media use the existing revision/snapshot authority.
use crate::{
    ProjectId, RepositoryError, SqliteStoryRepository, multimedia_types::*,
    multimedia_validation as validate,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[path = "multimedia_operations.rs"]
mod operations;
#[path = "multimedia_sources.rs"]
mod source_reader;
#[path = "multimedia_patch.rs"]
mod document_patch;
#[path = "multimedia_text.rs"]
pub(crate) mod text;
#[path = "multimedia_canon.rs"]
mod canon;
#[path = "multimedia_catalog.rs"]
mod catalog;
#[path = "multimedia_adaptation.rs"]
pub(crate) mod adaptation;
pub use adaptation::RequestMediaAdaptation;
#[path = "multimedia_impact.rs"]
mod impact;
pub use canon::{CanonEntry, CanonCategory, CanonDocument, CanonWorkspace, SaveCanonDocument};
pub(crate) use operations::assert_no_pending_operations;
pub use operations::{
    MediaOperationOutputs, MediaOperationSeal, MediaTranscriptInput, PrepareMediaOperation,
};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_creative_scopes_v1 (
 scope_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('canon','edition')), UNIQUE(project_id,scope_id)
);
CREATE TABLE IF NOT EXISTS story_editions_v1 (
 edition_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, scope_id TEXT NOT NULL UNIQUE,
 medium TEXT NOT NULL CHECK(medium IN ('novel','comic','animation')), title TEXT NOT NULL,
 main_branch_id TEXT NOT NULL, canon_snapshot_id TEXT NOT NULL, sources_json TEXT NOT NULL,
 created_at INTEGER NOT NULL, FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE TABLE IF NOT EXISTS story_scope_branches_v1 (
 project_id TEXT NOT NULL, scope_id TEXT NOT NULL, branch_id TEXT NOT NULL, title TEXT NOT NULL,
 PRIMARY KEY(project_id,branch_id), FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id),
 FOREIGN KEY(project_id,branch_id) REFERENCES story_branches_v2(project_id,branch_id)
);
CREATE TABLE IF NOT EXISTS story_creative_project_receipts_v1 (
 idempotency_key TEXT PRIMARY KEY, request_digest TEXT NOT NULL, result_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_media_revision_scopes_v1 (
 revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id),
 project_id TEXT NOT NULL, scope_id TEXT NOT NULL,
 FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE TABLE IF NOT EXISTS story_media_revision_canon_v1 (
 revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id),
 canon_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id)
);
CREATE TABLE IF NOT EXISTS story_media_revision_sources_v1 (
 revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id),
 sources_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_media_revision_libraries_v1 (
 revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id),
 asset_revision_ids_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_media_source_links_v1 (
 revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id), ordinal INTEGER NOT NULL,
 source_json TEXT NOT NULL, PRIMARY KEY(revision_id,ordinal)
);
CREATE TABLE IF NOT EXISTS story_media_read_dependencies_v1 (
 revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id), ordinal INTEGER NOT NULL,
 dependency_json TEXT NOT NULL, PRIMARY KEY(revision_id,ordinal)
);
CREATE TABLE IF NOT EXISTS story_media_imports_v1 (
 operation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, scope_id TEXT NOT NULL,
 name TEXT NOT NULL, request_digest TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('receiving','registered','abandoned')),
 lease_expires_at INTEGER NOT NULL, asset_revision_id TEXT REFERENCES story_revisions_v2(revision_id),
 FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE TABLE IF NOT EXISTS story_media_assets_v1 (
 asset_revision_id TEXT PRIMARY KEY REFERENCES story_revisions_v2(revision_id), asset_id TEXT NOT NULL,
 project_id TEXT NOT NULL, scope_id TEXT NOT NULL, digest TEXT NOT NULL, metadata_json TEXT NOT NULL,
 created_at INTEGER NOT NULL, FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE INDEX IF NOT EXISTS story_media_assets_scope_v1 ON story_media_assets_v1(project_id,scope_id,created_at);
CREATE TABLE IF NOT EXISTS story_media_adaptations_v1 (
 step_id TEXT PRIMARY KEY REFERENCES story_work_steps_v1(id),
 source_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 edition_id TEXT NOT NULL REFERENCES story_editions_v1(edition_id),
 intent_revision INTEGER NOT NULL, instruction TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, request_digest TEXT NOT NULL,
 UNIQUE(source_task_id,idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_media_task_bindings_v1 (
 task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id), project_id TEXT NOT NULL,
 edition_id TEXT NOT NULL REFERENCES story_editions_v1(edition_id), scope_id TEXT NOT NULL,
 branch_id TEXT NOT NULL, snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
 asset_revisions_json TEXT NOT NULL, FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE TABLE IF NOT EXISTS story_media_operation_intents_v1 (
 operation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id),
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id), work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 edition_id TEXT NOT NULL REFERENCES story_editions_v1(edition_id), scope_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 request_digest TEXT NOT NULL, operation_json TEXT NOT NULL, grant_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('prepared','ready','failed','cancelled')),
 expected_output_count INTEGER, transcript_revision_id TEXT REFERENCES story_revisions_v2(revision_id),
 seal_json TEXT, created_at INTEGER NOT NULL,
 FOREIGN KEY(project_id,scope_id) REFERENCES story_creative_scopes_v1(project_id,scope_id)
);
CREATE INDEX IF NOT EXISTS story_media_operations_work_v1 ON story_media_operation_intents_v1(work_task_id,state);
"#;

fn storage(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}
fn encode<T: Serialize>(v: &T) -> Result<String, RepositoryError> {
    serde_json::to_string(v).map_err(|_| RepositoryError::CorruptState)
}
fn decode<T: DeserializeOwned>(v: &str) -> Result<T, RepositoryError> {
    serde_json::from_str(v).map_err(|_| RepositoryError::CorruptState)
}
fn id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}
fn digest<T: Serialize>(v: &T) -> Result<String, RepositoryError> {
    Ok(format!("{:x}", Sha256::digest(encode(v)?.as_bytes())))
}
fn fail(code: &str) -> RepositoryError {
    validate::invalid(code)
}
enum DocumentChange<'a> {
    Save(&'a SaveMultimediaDocument),
    Patch(&'a PatchMultimediaDocument),
}
fn project(db: &Connection, p: &str) -> Result<MediaProject, RepositoryError> {
    db.query_row("SELECT project_id,title,logline,created_at,updated_at FROM story_projects_v2 WHERE project_id=?1",[p],|r|Ok(MediaProject{id:r.get(0)?,title:r.get(1)?,logline:r.get(2)?,created_at:r.get(3)?,updated_at:r.get(4)?})).optional().map_err(storage)?.ok_or(RepositoryError::ProjectNotFound)
}
fn edition(
    db: &Connection,
    p: &str,
    e: &str,
    branch: Option<&str>,
) -> Result<CreativeEdition, RepositoryError> {
    let row=db.query_row("SELECT scope_id,medium,title,main_branch_id,canon_snapshot_id,sources_json FROM story_editions_v1 WHERE project_id=?1 AND edition_id=?2",params![p,e],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).optional().map_err(storage)?.ok_or_else(||fail("media_edition_not_found"))?;
    let branch = branch.unwrap_or(&row.3);
    let owned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_scope_branches_v1 WHERE project_id=?1 AND scope_id=?2 AND branch_id=?3)",params![p,row.0,branch],|r|r.get(0)).map_err(storage)?;
    if !owned {
        return Err(fail("media_branch_scope_mismatch"));
    }
    Ok(CreativeEdition {
        id: e.into(),
        edition_id: e.into(),
        project_id: p.into(),
        scope_id: row.0,
        branch_id: branch.into(),
        medium: decode(&format!("\"{}\"", row.1))?,
        title: row.2,
        canon_snapshot_id: Some(row.4),
        sources: decode(&row.5)?,
    })
}
fn editions(db: &Connection, p: &str) -> Result<Vec<CreativeEdition>, RepositoryError> {
    project(db, p)?;
    let mut stmt=db.prepare("SELECT edition_id FROM story_editions_v1 WHERE project_id=?1 ORDER BY (main_branch_id='main') DESC,created_at,edition_id").map_err(storage)?;
    let ids = stmt
        .query_map([p], |r| r.get::<_, String>(0))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    ids.iter().map(|e| {
        let edition = edition(db, p, e, None)?;
        if edition.medium == CreativeMedium::Novel { return Ok(edition); }
        let snapshot = head(db, &edition)?;
        edition_at_snapshot(db, p, edition, &snapshot)
    }).collect()
}
fn head(db: &Connection, e: &CreativeEdition) -> Result<String, RepositoryError> {
    db.query_row(
        "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id=?2",
        params![e.project_id, e.branch_id],
        |r| r.get(0),
    )
    .map_err(storage)
}
fn member(
    db: &Connection,
    snapshot: &str,
    object: &str,
) -> Result<Option<String>, RepositoryError> {
    db.query_row(
        "SELECT revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?1 AND object_id=?2",
        params![snapshot, object],
        |r| r.get(0),
    )
    .optional()
    .map_err(storage)
}
fn edition_at_snapshot(
    db: &Connection,
    p: &str,
    mut edition: CreativeEdition,
    snapshot: &str,
) -> Result<CreativeEdition, RepositoryError> {
    if let Some(revision) = member(db, snapshot, &edition.id)? {
        // Every current media revision has its own mandatory pin. No HEAD or
        // edition-level replacement may supply a missing immutable attribute.
        let canon_snapshot: String = db.query_row(
            "SELECT canon_snapshot_id FROM story_media_revision_canon_v1 WHERE revision_id=?1",
            [&revision], |row| row.get(0),
        ).map_err(storage)?;
        canon::at(db, p, Some(&canon_snapshot))?;
        edition.canon_snapshot_id = Some(canon_snapshot);
        let sources: String = db.query_row("SELECT sources_json FROM story_media_revision_sources_v1 WHERE revision_id=?1", [&revision], |row| row.get(0)).map_err(storage)?;
        edition.sources = decode(&sources)?;
    }
    Ok(edition)
}
fn receipt<T: DeserializeOwned>(
    db: &Connection,
    p: &str,
    op: &str,
    key: &str,
    hash: &str,
) -> Result<Option<T>, RepositoryError> {
    let row=db.query_row("SELECT request_digest,result_json FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation=?2 AND idempotency_key=?3",params![p,op,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(storage)?;
    match row {
        None => Ok(None),
        Some((existing, json)) => {
            if existing != hash {
                return Err(RepositoryError::IdempotencyConflict);
            }
            Ok(Some(decode(&json)?))
        }
    }
}
fn save_receipt<T: Serialize>(
    db: &Connection,
    p: &str,
    op: &str,
    key: &str,
    hash: &str,
    v: &T,
) -> Result<(), RepositoryError> {
    db.execute("INSERT INTO story_domain_receipts_v2(project_id,operation,idempotency_key,request_digest,result_json) VALUES(?1,?2,?3,?4,?5)",params![p,op,key,hash,encode(v)?]).map_err(storage)?;
    Ok(())
}

pub(crate) fn initialize_project(
    db: &Connection,
    p: &str,
    title: &str,
    branch: &str,
    medium: CreativeMedium,
    now: i64,
) -> Result<(), RepositoryError> {
    let canon = id("canon");
    let canon_branch = id("branch");
    let canon_snapshot = id("snapshot");
    db.execute(
        "INSERT INTO story_creative_scopes_v1 VALUES(?1,?2,'canon')",
        params![canon, p],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_snapshots_v2 VALUES(?1,?2,?3,NULL)",
        params![canon_snapshot, p, canon_branch],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_branches_v2 VALUES(?1,?2,?3)",
        params![p, canon_branch, canon_snapshot],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_scope_branches_v1 VALUES(?1,?2,?3,'共同设定')",
        params![p, canon, canon_branch],
    )
    .map_err(storage)?;
    insert_edition(db, p, title, medium, branch, &canon_snapshot, &[], now)?;
    Ok(())
}
fn insert_edition(
    db: &Connection,
    p: &str,
    title: &str,
    medium: CreativeMedium,
    branch: &str,
    canon_snapshot: &str,
    sources: &[MediaSourceRef],
    now: i64,
) -> Result<CreativeEdition, RepositoryError> {
    let e = id("edition");
    let scope = id("scope");
    db.execute(
        "INSERT INTO story_creative_scopes_v1 VALUES(?1,?2,'edition')",
        params![scope, p],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_editions_v1 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            e,
            p,
            scope,
            medium.as_str(),
            title,
            branch,
            canon_snapshot,
            encode(&sources)?,
            now
        ],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_scope_branches_v1 VALUES(?1,?2,?3,?4)",
        params![p, scope, branch, title],
    )
    .map_err(storage)?;
    edition(db, p, &e, None)
}
fn workspace(
    db: &Connection,
    p: &str,
    e: &str,
    branch: Option<&str>,
) -> Result<MultimediaWorkspace, RepositoryError> {
    workspace_at(db, p, e, branch, None, None)
}
fn workspace_at(
    db: &Connection,
    p: &str,
    e: &str,
    branch: Option<&str>,
    snapshot: Option<&str>,
    asset_ids: Option<&[String]>,
) -> Result<MultimediaWorkspace, RepositoryError> {
    let edition = edition(db, p, e, branch)?;
    let fixed_snapshot = snapshot.is_some();
    let snapshot = match snapshot {
        Some(s) => s.to_owned(),
        None => head(db, &edition)?,
    };
    let edition = edition_at_snapshot(db, p, edition, &snapshot)?;
    let revision = member(db, &snapshot, e)?;
    let library_pin;
    let admitted_assets = match (asset_ids, fixed_snapshot, &revision) {
        (Some(ids), _, _) => Some(ids),
        (None, true, Some(revision)) => {
            let encoded: String = db.query_row("SELECT asset_revision_ids_json FROM story_media_revision_libraries_v1 WHERE revision_id=?1", [revision], |row| row.get(0)).map_err(storage)?;
            library_pin = decode::<Vec<String>>(&encoded)?;
            Some(library_pin.as_slice())
        },
        _ => None,
    };
    let (document, document_encoding) = match &revision {
        Some(r) => {
            let json:String=db.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",params![p,e,r],|r|r.get(0)).map_err(storage)?;
            (decode(&json)?, Some(json))
        }
        None => (MultimediaDocument::empty(edition.medium, edition.title.clone())
            .ok_or_else(|| fail("novel_uses_novel_workspace"))?, None),
    };
    let mut stmt=db.prepare("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND scope_id=?2 ORDER BY created_at,asset_revision_id").map_err(storage)?;
    let rows = stmt
        .query_map(params![p, edition.scope_id], |r| r.get::<_, String>(0))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    let mut assets = rows
        .iter()
        .map(|s| decode(s))
        .collect::<Result<Vec<MediaAsset>, _>>()?;
    let document_assets = document_asset_ids(&document);
    if admitted_assets.is_some_and(|ids| document_assets.iter().any(|reference| !ids.contains(reference))) {
        return Err(RepositoryError::CorruptState);
    }
    let mut required_assets = document_assets;
    if let Some(ids) = admitted_assets { for id in ids { if !required_assets.contains(id) { required_assets.push(id.clone()); } } }
    for reference in required_assets {
        if assets.iter().any(|a| a.asset_revision_id == reference) {
            continue;
        }
        let stored:Option<String>=db.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND asset_revision_id=?2",params![p,reference],|r|r.get(0)).optional().map_err(storage)?;
        assets.push(decode(&stored.ok_or(RepositoryError::CorruptState)?)?);
    }
    if let Some(ids) = admitted_assets {
        assets.retain(|a| ids.contains(&a.asset_revision_id));
    }
    let mut sources = vec![];
    if let Some(r) = &revision {
        let mut stmt=db.prepare("SELECT source_json FROM story_media_source_links_v1 WHERE revision_id=?1 ORDER BY ordinal").map_err(storage)?;
        for v in stmt
            .query_map([r], |r| r.get::<_, String>(0))
            .map_err(storage)?
        {
            sources.push(decode(&v.map_err(storage)?)?);
        }
    }
    let document_source=match &revision {
        Some(revision)=>Some(MediaSourceRef{project_id:p.into(),scope_id:edition.scope_id.clone(),branch_id:edition.branch_id.clone(),object_id:edition.id.clone(),revision_id:revision.clone(),digest:db.query_row("SELECT content_digest FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2",params![p,revision],|r|r.get(0)).map_err(storage)?,selector:MediaSelector::Whole}),
        None=>None,
    };
    let project_revision = db
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
            [p],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let source_digest = digest(
        &json!({"scopeId":edition.scope_id,"branchId":edition.branch_id,"snapshotId":snapshot,"canonSnapshotId":edition.canon_snapshot_id,"sources":edition.sources,"assets":assets.iter().map(|a|json!([a.asset_revision_id,a.digest])).collect::<Vec<_>>()}),
    )?;
    Ok(MultimediaWorkspace {
        project: project(db, p)?,
        edition,
        revision: project_revision,
        source_digest,
        revision_id: revision,
        document_source,
        document_encoding,
        head_snapshot_id: snapshot,
        document,
        assets,
        sources,
    })
}

impl SqliteStoryRepository {
    
    
    
    
    
    
    
    
    
    
    
    
    
    fn save_multimedia_document_bound(
        &mut self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        change: DocumentChange<'_>,
        bound: Option<&crate::TaskRecord>,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let (operation, key, expected, hash) = match &change {
            DocumentChange::Save(r) => {
                validate::key(&r.idempotency_key)?;
                validate::document(&r.document)?;
                ("media-save", &r.idempotency_key, &r.expected_revision_id, digest(&(e, branch, r, bound.map(|t| t.task_id.as_str())))?)
            }
            DocumentChange::Patch(r) => {
                document_patch::validate_request(r)?;
                ("media-patch", &r.idempotency_key, &r.expected_revision_id, digest(&(e, branch, r, bound.map(|t| t.task_id.as_str())))?)
            }
        };
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(task) = bound {
            assert_task_media_write(&tx,task)?;
            assert_task_scope(&tx, task, e, branch)?;
            crate::task_input::assert_task_inputs_current(&tx, task)?;
            crate::task_intents::assert_no_pending_intent(&tx, &task.task_id)?;
        }
        if let Some(v) = receipt(&tx, p.as_str(), operation, key, &hash)? {
            return Ok(v);
        }
        let current = workspace(&tx, p.as_str(), e, branch)?;
        if &current.revision_id != expected {
            return Err(fail("media_revision_changed"));
        }
        // An author selecting a future baseline is not regenerating content.
        // Preserve the old read conditions verbatim; enforce them again on content writes.
        let baseline_only = bound.is_none() && matches!(&change, DocumentChange::Patch(r)
            if r.read_set.is_empty() && r.sources.is_empty()
                && r.operations.iter().all(|op| matches!(op, MediaDocumentOperation::ReplaceEditionSource { .. })));
        let patched;
        let (r, canon_snapshot_id, edition_sources) = match change {
            DocumentChange::Save(r) => (r, current.edition.canon_snapshot_id.as_deref().ok_or(RepositoryError::CorruptState)?, &current.edition.sources),
            DocumentChange::Patch(r) => {
                patched = document_patch::apply(&tx, &current, r, bound.is_none())?;
                (&patched.request, patched.canon_snapshot_id.as_str(), &patched.edition_sources)
            }
        };
        validate::document(&r.document)?;
        if r.read_set.len() > 500 || r.sources.len() > 1000 {
            return Err(fail("media_source_limit"));
        }
        if let Some(task) = bound {
            assert_task_read_sources(&tx, task, &current.edition, r)?;
            let allowed = operations::allowed_input_assets(&tx, task, &current.edition)?;
            if document_asset_ids(&r.document)
                .iter()
                .any(|id| !allowed.contains(id))
            {
                return Err(RepositoryError::TaskOutputForbidden);
            }
        }
        if current.edition.medium != r.document.medium() {
            return Err(fail("media_kind_mismatch"));
        }
        validate::preserve_locks(&current.document, &r.document, bound.is_none())?;
        for dep in &r.read_set {
            validate_source(&tx, p.as_str(), &dep.source, if baseline_only { MediaFreshness::Pinned } else { dep.freshness })?;
        }
        for link in &r.sources {
            if let Some(decision) = &link.decision { validate::text(decision, 20000, true)?; }
            if link.target_object_id != e {
                return Err(fail("media_target_mismatch"));
            }
            validate::select_document(&r.document, &link.target_selector)?;
            validate_source(&tx, p.as_str(), &link.source, MediaFreshness::Pinned)?;
        }
        validate_assets(&tx, &current.edition, r)?;
        let revision = id("revision");
        let content = encode(&r.document)?;
        let content_digest = digest(&r.document)?;
        let snapshot = id("snapshot");
        let commit = id("commit");
        tx.execute(
            "INSERT INTO story_revisions_v2 VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                revision,
                p.as_str(),
                e,
                r.document.medium().as_str(),
                current.revision_id,
                content_digest,
                content
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_revision_scopes_v1 VALUES(?1,?2,?3)",
            params![revision, p.as_str(), current.edition.scope_id],
        )
        .map_err(storage)?;
        tx.execute("INSERT INTO story_media_revision_canon_v1 VALUES(?1,?2)", params![revision, canon_snapshot_id]).map_err(storage)?;
        tx.execute("INSERT INTO story_media_revision_sources_v1 VALUES(?1,?2)", params![revision, encode(edition_sources)?]).map_err(storage)?;
        let mut library_assets = current.assets.iter().filter(|asset| asset.scope_id == current.edition.scope_id).map(|asset|asset.asset_revision_id.clone()).collect::<Vec<_>>();
        for id in document_asset_ids(&r.document) { if !library_assets.contains(&id) { library_assets.push(id); } }
        tx.execute("INSERT INTO story_media_revision_libraries_v1 VALUES(?1,?2)", params![revision, encode(&library_assets)?]).map_err(storage)?;
        for (i, link) in r.sources.iter().enumerate() {
            tx.execute(
                "INSERT INTO story_media_source_links_v1 VALUES(?1,?2,?3)",
                params![revision, i as i64, encode(link)?],
            )
            .map_err(storage)?;
        }
        for (i, dep) in r.read_set.iter().enumerate() {
            tx.execute(
                "INSERT INTO story_media_read_dependencies_v1 VALUES(?1,?2,?3)",
                params![revision, i as i64, encode(dep)?],
            )
            .map_err(storage)?;
        }
        tx.execute(
            "INSERT INTO story_snapshots_v2 VALUES(?1,?2,?3,?4)",
            params![
                snapshot,
                p.as_str(),
                current.edition.branch_id,
                current.head_snapshot_id
            ],
        )
        .map_err(storage)?;
        tx.execute("INSERT INTO story_snapshot_members_v2 SELECT ?1,object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?2 AND object_id!=?3",params![snapshot,current.head_snapshot_id,e]).map_err(storage)?;
        tx.execute(
            "INSERT INTO story_snapshot_members_v2 VALUES(?1,?2,?3)",
            params![snapshot, e, revision],
        )
        .map_err(storage)?;
        let changed=tx.execute("UPDATE story_branches_v2 SET head_snapshot_id=?1 WHERE project_id=?2 AND branch_id=?3 AND head_snapshot_id=?4",params![snapshot,p.as_str(),current.edition.branch_id,current.head_snapshot_id]).map_err(storage)?;
        if changed != 1 {
            return Err(fail("media_revision_changed"));
        }
        tx.execute(
            "INSERT INTO story_commits_v2 VALUES(?1,?2,?3,?4,?5,'applied',NULL)",
            params![
                commit,
                p.as_str(),
                current.edition.branch_id,
                current.head_snapshot_id,
                snapshot
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_receipts_v2 VALUES(?1,?2,?3,?4,?5,?6,'applied')",
            params![
                p.as_str(),
                current.edition.branch_id,
                format!("{operation}:{}", r.idempotency_key),
                hash,
                commit,
                snapshot
            ],
        )
        .map_err(storage)?;
        if let Some(task) = bound {
            tx.execute("INSERT INTO story_task_outputs_v2(task_id,ordinal,output_kind,revision_id,working_base_revision_id,completeness) SELECT ?1,COALESCE(MAX(ordinal)+1,0),?2,?3,?4,'complete' FROM story_task_outputs_v2 WHERE task_id=?1",params![task.task_id,r.document.medium().as_str(),revision,current.revision_id]).map_err(storage)?;
            let refs: Vec<Value> = {
                let mut stmt=tx.prepare("SELECT output_kind,revision_id FROM story_task_outputs_v2 WHERE task_id=?1 ORDER BY ordinal").map_err(storage)?;
                stmt.query_map([&task.task_id], |row| {
                    Ok(json!({"kind":row.get::<_,String>(0)?,"revisionId":row.get::<_,String>(1)?}))
                })
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?
            };
            tx.execute(
                "UPDATE story_tasks_v2 SET output_refs_json=?1 WHERE task_id=?2",
                params![encode(&refs)?, task.task_id],
            )
            .map_err(storage)?;
        }
        let result = workspace(&tx, p.as_str(), e, branch)?;
        save_receipt(
            &tx,
            p.as_str(),
            operation,
            &r.idempotency_key,
            &hash,
            &result,
        )?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    
    
}

pub(crate) fn media_target(
    db: &Connection,
    p: &str,
    target: &Value,
) -> Result<Option<CreativeEdition>, RepositoryError> {
    if target["type"] != "edition" {
        return Ok(None);
    }
    let e = target["editionId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let scope = target["scopeId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let branch = target["branchId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let edition = edition(db, p, e, Some(branch))?;
    if edition.scope_id != scope || edition.medium == CreativeMedium::Novel {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(Some(edition))
}
pub(crate) fn media_budget_source(
    db: &Connection,
    p: &ProjectId,
    target: &Value,
) -> Result<Option<(String, String)>, RepositoryError> {
    let Some(e) = media_target(db, p.as_str(), target)? else {
        return Ok(None);
    };
    let w = workspace(db, p.as_str(), &e.id, Some(&e.branch_id))?;
    Ok(Some((w.head_snapshot_id, w.source_digest)))
}
pub(crate) fn validate_task_target(
    db: &Connection,
    request: &crate::CreateTaskRequest,
    target: &Value,
) -> Result<(), RepositoryError> {
    let Some(e) = media_target(db, request.project_id.as_str(), target)? else {
        let novel:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_editions_v1 WHERE project_id=?1 AND medium='novel' AND main_branch_id='main')",[request.project_id.as_str()],|r|r.get(0)).map_err(storage)?;
        if !novel {
            return Err(fail("media_task_requires_edition"));
        }
        return Ok(());
    };
    if request.operation != "advisory" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let manifest: Value = decode(&request.input_manifest_json)?;
    let author_text = manifest["authorIntent"].as_str().ok_or(RepositoryError::TaskBindingInvalid)?;
    crate::author_message::validate_text(db, request.project_id.as_str(), target, author_text)?;
    if target.get("revisionId").is_some() {
        let current = member(db, &head(db, &e)?, &e.id)?;
        if target["revisionId"].as_str() != current.as_deref() {
            return Err(RepositoryError::TaskInputStale);
        }
    }
    let thread: Option<Option<String>> = db
        .query_row(
            "SELECT scope_id FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2",
            params![request.project_id.as_str(), request.thread_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if thread.flatten().as_deref() != Some(e.scope_id.as_str())
        && !adaptation::scheduled_scope(db, request, &e)? {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
pub(crate) fn capture_media_task(
    db: &Connection,
    task_id: &str,
    p: &ProjectId,
) -> Result<bool, RepositoryError> {
    let task = crate::tasks::load_task(db, task_id)?;
    let target: Value = decode(&task.target_json)?;
    let Some(e) = media_target(db, p.as_str(), &target)? else {
        return Ok(false);
    };
    let w = workspace(db, p.as_str(), &e.id, Some(&e.branch_id))?;
    db.execute(
        "INSERT INTO story_task_source_origins_v2 VALUES(?1,?2)",
        params![task_id, w.head_snapshot_id],
    )
    .map_err(storage)?;
    db.execute(
        "INSERT INTO story_media_task_bindings_v1 VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            task_id,
            p.as_str(),
            e.id,
            e.scope_id,
            e.branch_id,
            w.head_snapshot_id,
            encode(
                &w.assets
                    .iter()
                    .map(|a| a.asset_revision_id.clone())
                    .collect::<Vec<_>>()
            )?
        ],
    )
    .map_err(storage)?;
    Ok(true)
}
fn assert_task_media_write(db: &Connection, task: &crate::TaskRecord) -> Result<(), RepositoryError> {
    let current=crate::tasks::load_task(db,&task.task_id)?;
    let target:Value=decode(&current.target_json)?;
    if target["readOnly"]==true || target["conversationFocus"]["readOnly"]==true || target.get("interludeId").is_some() {
        return Err(RepositoryError::TaskNotWritable);
    }
    Ok(())
}
fn assert_task_scope(
    db: &Connection,
    task: &crate::TaskRecord,
    e: &str,
    branch: Option<&str>,
) -> Result<CreativeEdition, RepositoryError> {
    let current = crate::tasks::load_task(db, &task.task_id)?;
    if current.project_id != task.project_id
        || current.thread_id != task.thread_id
        || current.operation != "advisory"
        || !matches!(current.status.as_str(), "queued" | "running" | "waiting")
        || current.capability_revoked_at.is_some()
    {
        return Err(RepositoryError::TaskNotWritable);
    }
    let target: Value = decode(&current.target_json)?;
    let scope = media_target(db, task.project_id.as_str(), &target)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if scope.id != e || branch != Some(scope.branch_id.as_str()) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let bound:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_media_task_bindings_v1 WHERE task_id=?1 AND project_id=?2 AND edition_id=?3 AND scope_id=?4 AND branch_id=?5)",params![task.task_id,task.project_id.as_str(),e,scope.scope_id,scope.branch_id],|r|r.get(0)).map_err(storage)?;
    if !bound {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let snapshot: String = db.query_row("SELECT snapshot_id FROM story_media_task_bindings_v1 WHERE task_id=?1",[&task.task_id],|row|row.get(0)).map_err(storage)?;
    edition_at_snapshot(db, task.project_id.as_str(), scope, &snapshot)
}
pub(crate) fn assert_media_inputs(
    db: &Connection,
    task: &crate::TaskRecord,
) -> Result<bool, RepositoryError> {
    let target: Value = decode(&task.target_json)?;
    let Some(e) = media_target(db, task.project_id.as_str(), &target)? else {
        return Ok(false);
    };
    let snapshot: String = db
        .query_row(
            "SELECT snapshot_id FROM story_media_task_bindings_v1 WHERE task_id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let output:Option<String>=db.query_row("SELECT o.revision_id FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id WHERE o.task_id=?1 AND r.object_id=?2 ORDER BY o.ordinal DESC LIMIT 1",params![task.task_id,e.id],|r|r.get(0)).optional().map_err(storage)?;
    let expected = match output {
        Some(r) => Some(r),
        None => member(db, &snapshot, &e.id)?,
    };
    if member(db, &head(db, &e)?, &e.id)? != expected {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(true)
}
fn assert_task_read_sources(
    db: &Connection,
    task: &crate::TaskRecord,
    edition: &CreativeEdition,
    request: &SaveMultimediaDocument,
) -> Result<(), RepositoryError> {
    let granted = source_reader::grants(db, task, edition)?;
    for source in request.read_set.iter().map(|d| &d.source).chain(request.sources.iter().map(|l| &l.source)) {
        if !granted.iter().any(|g| source_reader::exact_scope(g, source)) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    Ok(())
}



fn validate_source(
    db: &Connection,
    p: &str,
    s: &MediaSourceRef,
    freshness: MediaFreshness,
) -> Result<(), RepositoryError> {
    if s.project_id != p {
        return Err(RepositoryError::CrossProjectReference);
    }
    if matches!(&s.selector, MediaSelector::Text {quote,..} | MediaSelector::FieldText {quote,..} if quote.chars().count()>8000) {
        return Err(fail("media_source_quote_limit"));
    }
    for k in [&s.scope_id, &s.branch_id, &s.object_id, &s.revision_id] {
        validate::key(k)?;
    }
    let scope: Option<String> = db
        .query_row(
            "SELECT scope_id FROM story_scope_branches_v1 WHERE project_id=?1 AND branch_id=?2",
            params![p, s.branch_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if scope.as_deref() != Some(s.scope_id.as_str()) {
        return Err(fail("media_source_scope_mismatch"));
    }
    let row=db.query_row("SELECT object_kind,content_digest,content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",params![p,s.object_id,s.revision_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional().map_err(storage)?.ok_or_else(||fail("media_source_not_found"))?;
    if row.1 != s.digest {
        return Err(fail("media_source_digest_mismatch"));
    }
    let declared:Option<String>=db.query_row("SELECT scope_id FROM story_media_revision_scopes_v1 WHERE project_id=?1 AND revision_id=?2",params![p,s.revision_id],|r|r.get(0)).optional().map_err(storage)?;
    if row.0 == "mediaAsset" {
        if declared.as_deref() != Some(s.scope_id.as_str()) {
            return Err(fail("media_source_scope_mismatch"));
        }
        let asset: MediaAsset = decode(&row.2)?;
        select_asset(&asset, &s.selector)?;
        return Ok(());
    }
    if declared.as_deref().is_some_and(|scope| scope != s.scope_id) {
        return Err(fail("media_source_scope_mismatch"));
    }
    // A pinned revision must have belonged to the selected branch's ancestry, not merely share a project.
    let reachable:bool=db.query_row("WITH RECURSIVE chain(id) AS (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id=?2 UNION SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN chain c ON s.snapshot_id=c.id WHERE s.parent_snapshot_id IS NOT NULL) SELECT EXISTS(SELECT 1 FROM chain c JOIN story_snapshot_members_v2 m ON m.snapshot_id=c.id WHERE m.object_id=?3 AND m.revision_id=?4)",params![p,s.branch_id,s.object_id,s.revision_id],|r|r.get(0)).map_err(storage)?;
    if !reachable {
        return Err(fail("media_source_not_in_branch"));
    }
    if freshness == MediaFreshness::MustBeCurrent {
        let current:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id WHERE b.project_id=?1 AND b.branch_id=?2 AND m.object_id=?3 AND m.revision_id=?4)",params![p,s.branch_id,s.object_id,s.revision_id],|r|r.get(0)).map_err(storage)?;
        if !current {
            return Err(fail("media_source_changed"));
        }
    }
    if matches!(row.0.as_str(), "comic" | "animation") {
        select_document_source(db, p, &decode(&row.2)?, &s.selector)?;
    } else {
        match &s.selector {
            MediaSelector::FieldText { .. } if row.0 == "canon" => text::validate_selector(&decode::<Value>(&row.2)?, &s.selector)?,
            MediaSelector::Whole => {}
            MediaSelector::Text {
                start_byte,
                end_byte,
                quote,
            } => {
                let payload: Value = decode(&row.2)?;
                let body = payload
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("media_source_text_unavailable"))?;
                if body.get(*start_byte..*end_byte) != Some(quote.as_str()) || quote.is_empty() {
                    return Err(fail("media_source_text_mismatch"));
                }
            }
            _ => return Err(fail("invalid_media_selector")),
        }
    }
    Ok(())
}
fn select_document_source(
    db: &Connection,
    p: &str,
    document: &MultimediaDocument,
    selector: &MediaSelector,
) -> Result<(), RepositoryError> {
    validate::select_document(document, selector)?;
    if let (
        MultimediaDocument::Animation { shots, .. },
        MediaSelector::Shot {
            shot_id,
            range: Some(range),
        },
    ) = (document, selector)
    {
        let asset = shots
            .iter()
            .find(|s| s.id == *shot_id)
            .and_then(|s| s.asset_revision_id.as_deref())
            .ok_or_else(|| fail("media_shot_range_requires_asset"))?;
        let stored:Option<String>=db.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND asset_revision_id=?2",params![p,asset],|r|r.get(0)).optional().map_err(storage)?;
        select_asset(
            &decode::<MediaAsset>(&stored.ok_or_else(|| fail("media_asset_not_found"))?)?,
            &MediaSelector::Time {
                start: range.start.clone(),
                duration: range.duration.clone(),
            },
        )?;
    }
    Ok(())
}
fn select_asset(a: &MediaAsset, s: &MediaSelector) -> Result<(), RepositoryError> {
    match s {
        MediaSelector::Whole => Ok(()),
        MediaSelector::Region { region } => validate::rect(
            region,
            a.width.ok_or_else(|| fail("invalid_media_selector"))? as f64,
            a.height.ok_or_else(|| fail("invalid_media_selector"))? as f64,
        ),
        MediaSelector::Time { start, duration } => {
            validate::time(start, false)?;
            validate::time(duration, true)?;
            let total = a
                .duration
                .as_ref()
                .ok_or_else(|| fail("invalid_media_selector"))?;
            if !validate::within(
                start,
                duration,
                &Rational {
                    numerator: 1,
                    denominator: 1,
                },
                total,
            ) {
                return Err(fail("media_range_outside_source"));
            }
            Ok(())
        }
        _ => Err(fail("invalid_media_selector")),
    }
}
fn asset_ref(
    db: &Connection,
    e: &CreativeEdition,
    id: &str,
    r: &SaveMultimediaDocument,
) -> Result<MediaAsset, RepositoryError> {
    let raw:Option<String>=db.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND asset_revision_id=?2",params![e.project_id,id],|r|r.get(0)).optional().map_err(storage)?;
    let a: MediaAsset = decode(&raw.ok_or_else(|| fail("media_asset_not_found"))?)?;
    if a.scope_id != e.scope_id
        && !r.read_set.iter().any(|d| d.source.revision_id == id)
        && !r.sources.iter().any(|l| l.source.revision_id == id)
    {
        return Err(fail("media_asset_scope_mismatch"));
    }
    Ok(a)
}
fn validate_assets(
    db: &Connection,
    e: &CreativeEdition,
    r: &SaveMultimediaDocument,
) -> Result<(), RepositoryError> {
    match &r.document {
        MultimediaDocument::Comic { pages, .. } => {
            for p in pages {
                for panel in &p.panels {
                    for reference in &panel.reference_asset_revision_ids {
                        if asset_ref(db, e, reference, r)?.kind != MediaAssetKind::Image {
                            return Err(fail("media_image_required"));
                        }
                    }
                    if let Some(reference) = &panel.asset_revision_id {
                        if asset_ref(db, e, reference, r)?.kind != MediaAssetKind::Image {
                            return Err(fail("media_image_required"));
                        }
                    }
                }
            }
        }
        MultimediaDocument::Animation {
            shots, timeline, ..
        } => {
            for shot in shots {
                for reference in &shot.reference_asset_revision_ids {
                    if asset_ref(db, e, reference, r)?.kind != MediaAssetKind::Image {
                        return Err(fail("media_image_required"));
                    }
                }
                if let Some(reference) = &shot.asset_revision_id {
                    if asset_ref(db, e, reference, r)?.kind == MediaAssetKind::Audio {
                        return Err(fail("media_visual_required"));
                    }
                }
            }
            for track in &timeline.tracks {
                for clip in &track.clips {
                    let reference = clip.asset_revision_id.as_ref().or_else(|| {
                        clip.shot_id
                            .as_ref()
                            .and_then(|s| shots.iter().find(|x| &x.id == s))
                            .and_then(|s| s.asset_revision_id.as_ref())
                    });
                    if let Some(reference) = reference {
                        let a = asset_ref(db, e, reference, r)?;
                        if (track.kind == TrackKind::Audio && a.kind == MediaAssetKind::Image)
                            || (track.kind == TrackKind::Video && a.kind == MediaAssetKind::Audio)
                        {
                            return Err(fail("media_track_asset_mismatch"));
                        }
                        if let Some(total) = &a.duration {
                            if !validate::within(
                                &clip.source_start,
                                &clip.duration,
                                &clip.playback_rate,
                                total,
                            ) {
                                return Err(fail("media_range_outside_source"));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn document_asset_ids(document: &MultimediaDocument) -> Vec<String> {
    let mut ids = std::collections::BTreeSet::new();
    match document {
        MultimediaDocument::Comic { pages, .. } => {
            for p in pages {
                for panel in &p.panels {
                    ids.extend(panel.asset_revision_id.iter().cloned());
                    ids.extend(panel.reference_asset_revision_ids.iter().cloned());
                }
            }
        }
        MultimediaDocument::Animation {
            shots, timeline, ..
        } => {
            for shot in shots {
                ids.extend(shot.asset_revision_id.iter().cloned());
                ids.extend(shot.reference_asset_revision_ids.iter().cloned());
            }
            for track in &timeline.tracks {
                for clip in &track.clips {
                    ids.extend(clip.asset_revision_id.iter().cloned());
                }
            }
        }
    }
    ids.into_iter().collect()
}

/// Author message references are durable, while bytes remain in the asset store.
/// This validates reception and binding; it does not grant any editing authority.
pub(crate) fn message_target(db:&Connection,p:&ProjectId,thread:&str,input:&Value)->Result<Value,RepositoryError> {
    let object=input.as_object().ok_or(RepositoryError::TaskBindingInvalid)?;
    if object.len()!=4 || object.keys().any(|k|!matches!(k.as_str(),"scope"|"expectedRevisionId"|"selection"|"imageAssetRevisionIds")) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let scope=input["scope"].as_object().ok_or(RepositoryError::TaskBindingInvalid)?;
    if scope.len()!=4 || scope.get("projectId")!=Some(&json!(p.as_str())) {return Err(RepositoryError::TaskBindingInvalid);}
    let target=json!({"type":"edition","editionId":input["scope"]["editionId"],"scopeId":input["scope"]["scopeId"],
        "branchId":input["scope"]["branchId"],"revisionId":input["expectedRevisionId"],"selection":input["selection"],"imageAssetRevisionIds":input["imageAssetRevisionIds"]});
    let e=media_target(db,p.as_str(),&target)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let owner:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2 AND scope_id=?3)",params![p.as_str(),thread,e.scope_id],|r|r.get(0)).map_err(storage)?;
    if !owner {return Err(RepositoryError::TaskBindingInvalid);}
    let w=workspace(db,p.as_str(),&e.id,Some(&e.branch_id))?;
    if input["expectedRevisionId"]!=json!(w.revision_id) {return Err(RepositoryError::TaskInputStale);}
    let images=input["imageAssetRevisionIds"].as_array().filter(|ids|ids.len()<=5).ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut seen=std::collections::BTreeSet::new();
    for value in images {
        let id=value.as_str().ok_or(RepositoryError::TaskBindingInvalid)?;
        if !seen.insert(id) || !w.assets.iter().any(|asset|asset.asset_revision_id==id && asset.kind==MediaAssetKind::Image && asset.scope_id==e.scope_id) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    let selection=&input["selection"];
    if !selection.is_null() {
        if selection["scope"]!=input["scope"] || selection["revisionId"]!=input["expectedRevisionId"]
            || selection["label"].as_str().is_none_or(|s|s.chars().count()>500) || selection.to_string().len()>8000 {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        serde_json::from_value::<MediaSelector>(selection["selector"].clone()).map_err(|_|RepositoryError::TaskBindingInvalid)?;
    }
    Ok(target)
}

fn create_edition(db: &Connection, p: &ProjectId, r: &CreateEditionRequest, now: i64) -> Result<CreativeEdition, RepositoryError> {
        validate::key(&r.idempotency_key)?;
        validate::text(&r.title, 160, true)?;
        if r.sources.len() > 100 {
            return Err(fail("media_source_limit"));
        }
        let hash = digest(r)?;
        project(db, p.as_str())?;
        if let Some(v) = receipt(db, p.as_str(), "create-edition", &r.idempotency_key, &hash)? {
            return Ok(v);
        }
        // The chapter APIs own the project's explicit main novel edition. Other media never manufacture one.
        if r.medium == CreativeMedium::Novel {
            return Err(fail("additional_novel_edition_not_supported"));
        }
        for source in &r.sources {
            validate_source(db, p.as_str(), source, MediaFreshness::Pinned)?;
        }
        let canon:String=db.query_row("SELECT b.head_snapshot_id FROM story_creative_scopes_v1 s JOIN story_scope_branches_v1 x ON x.project_id=s.project_id AND x.scope_id=s.scope_id JOIN story_branches_v2 b ON b.project_id=x.project_id AND b.branch_id=x.branch_id WHERE s.project_id=?1 AND s.kind='canon' ORDER BY b.branch_id LIMIT 1",[p.as_str()],|r|r.get(0)).map_err(storage)?;
        let branch = id("branch");
        let snapshot = id("snapshot");
        db.execute(
            "INSERT INTO story_snapshots_v2 VALUES(?1,?2,?3,NULL)",
            params![snapshot, p.as_str(), branch],
        )
        .map_err(storage)?;
        db.execute(
            "INSERT INTO story_branches_v2 VALUES(?1,?2,?3)",
            params![p.as_str(), branch, snapshot],
        )
        .map_err(storage)?;
        let e = insert_edition(
            db,
            p.as_str(),
            &r.title,
            r.medium,
            &branch,
            &canon,
            &r.sources,
            now,
        )?;
        save_receipt(
            db,
            p.as_str(),
            "create-edition",
            &r.idempotency_key,
            &hash,
            &e,
        )?;
        Ok(e)
}

/// Public multimedia capability; SQLite retains transaction ownership.
pub trait MultimediaRepository {
fn media_asset_revision(
        &self,
        p: &ProjectId,
        revision: &str,
    ) -> Result<MediaAsset, RepositoryError>;
fn creative_project(
        &self,
        p: &ProjectId,
    ) -> Result<CreativeProjectCreated, RepositoryError>;
fn creative_editions(
        &self,
        p: &ProjectId,
    ) -> Result<Vec<CreativeEdition>, RepositoryError>;
fn require_novel_project(&self, p: &ProjectId) -> Result<(), RepositoryError>;
fn create_creative_project(
        &mut self,
        r: &CreateCreativeProjectRequest,
        now: i64,
    ) -> Result<CreativeProjectCreated, RepositoryError>;
fn create_creative_edition(
        &mut self, p: &ProjectId, r: &CreateEditionRequest, now: i64,
    ) -> Result<CreativeEdition, RepositoryError>;
fn multimedia_workspace(
        &self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn multimedia_revision_workspace(
        &self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        revision: &str,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn create_media_branch(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &CreateMediaBranch,
    ) -> Result<CreativeEdition, RepositoryError>;
fn save_multimedia_document(
        &mut self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        r: &SaveMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn save_task_multimedia_document(
        &mut self,
        task: &crate::TaskRecord,
        r: &SaveMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn patch_multimedia_document(
        &mut self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        r: &PatchMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn patch_task_multimedia_document(
        &mut self,
        task: &crate::TaskRecord,
        r: &PatchMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn begin_media_import(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &BeginMediaImport,
        now: i64,
    ) -> Result<Value, RepositoryError>;
fn register_media_asset(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &RegisterMediaAsset,
        now: i64,
    ) -> Result<MediaAsset, RepositoryError>;
fn media_task_scope(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<CreativeEdition, RepositoryError>;
fn media_task_working_workspace(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
fn media_task_workspace(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<MultimediaWorkspace, RepositoryError>;
}

impl MultimediaRepository for SqliteStoryRepository {
fn media_asset_revision(
        &self,
        p: &ProjectId,
        revision: &str,
    ) -> Result<MediaAsset, RepositoryError> {
        validate::key(revision)?;
        let stored:Option<String>=self.connection.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND asset_revision_id=?2",params![p.as_str(),revision],|r|r.get(0)).optional().map_err(storage)?;
        decode(&stored.ok_or_else(|| fail("media_asset_not_found"))?)
    }

fn creative_project(
        &self,
        p: &ProjectId,
    ) -> Result<CreativeProjectCreated, RepositoryError> {
        let editions = editions(&self.connection, p.as_str())?;
        let active = editions
            .first()
            .ok_or(RepositoryError::CorruptState)?
            .id
            .clone();
        Ok(CreativeProjectCreated {
            project: project(&self.connection, p.as_str())?,
            editions,
            active_edition_id: active,
        })
    }

fn creative_editions(
        &self,
        p: &ProjectId,
    ) -> Result<Vec<CreativeEdition>, RepositoryError> {
        editions(&self.connection, p.as_str())
    }

fn require_novel_project(&self, p: &ProjectId) -> Result<(), RepositoryError> {
        let exists:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_editions_v1 WHERE project_id=?1 AND medium='novel' AND main_branch_id='main')",[p.as_str()],|r|r.get(0)).map_err(storage)?;
        if !exists {
            return Err(fail("novel_scope_required"));
        }
        Ok(())
    }

fn create_creative_project(
        &mut self,
        r: &CreateCreativeProjectRequest,
        now: i64,
    ) -> Result<CreativeProjectCreated, RepositoryError> {
        validate::key(&r.idempotency_key)?;
        validate::text(&r.title, 120, true)?;
        validate::text(&r.logline, 500, false)?;
        let hash = digest(r)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let replay=tx.query_row("SELECT request_digest,result_json FROM story_creative_project_receipts_v1 WHERE idempotency_key=?1",[&r.idempotency_key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(storage)?;
        if let Some((h, v)) = replay {
            if h != hash {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return decode(&v);
        }
        let p = id("project");
        let snapshot = id("snapshot");
        tx.execute(
            "INSERT INTO story_projects_v2 VALUES(?1,?2,?3,?4,?4)",
            params![p, r.title.trim(), r.logline.trim(), now],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_workspaces_v2(project_id,revision) VALUES(?1,0)",
            [&p],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_snapshots_v2 VALUES(?1,?2,'main',NULL)",
            params![snapshot, p],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_branches_v2 VALUES(?1,'main',?2)",
            params![p, snapshot],
        )
        .map_err(storage)?;
        initialize_project(&tx, &p, r.title.trim(), "main", r.medium, now)?;
        let es = editions(&tx, &p)?;
        let result = CreativeProjectCreated {
            project: project(&tx, &p)?,
            active_edition_id: es[0].id.clone(),
            editions: es,
        };
        tx.execute(
            "INSERT INTO story_creative_project_receipts_v1 VALUES(?1,?2,?3)",
            params![r.idempotency_key, hash, encode(&result)?],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

fn create_creative_edition(
        &mut self, p: &ProjectId, r: &CreateEditionRequest, now: i64,
    ) -> Result<CreativeEdition, RepositoryError> {
        let tx = self.connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(storage)?;
        let result = create_edition(&tx, p, r, now)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

fn multimedia_workspace(
        &self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let w = workspace(&tx, p.as_str(), e, branch)?;
        tx.commit().map_err(storage)?;
        Ok(w)
    }

fn multimedia_revision_workspace(
        &self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        revision: &str,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let edition = edition(&tx, p.as_str(), e, branch)?;
        let snapshot:Option<String>=tx.query_row("WITH RECURSIVE chain(id) AS (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id=?2 UNION SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN chain c ON c.id=s.snapshot_id WHERE s.parent_snapshot_id IS NOT NULL) SELECT m.snapshot_id FROM chain c JOIN story_snapshot_members_v2 m ON m.snapshot_id=c.id WHERE m.object_id=?3 AND m.revision_id=?4 LIMIT 1",params![p.as_str(),edition.branch_id,e,revision],|r|r.get(0)).optional().map_err(storage)?;
        let snapshot = snapshot.ok_or_else(|| fail("media_revision_not_found"))?;
        let w = workspace_at(
            &tx,
            p.as_str(),
            e,
            Some(&edition.branch_id),
            Some(&snapshot),
            None,
        )?;
        tx.commit().map_err(storage)?;
        Ok(w)
    }

fn create_media_branch(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &CreateMediaBranch,
    ) -> Result<CreativeEdition, RepositoryError> {
        validate::key(&r.idempotency_key)?;
        validate::text(&r.title, 160, true)?;
        let hash = digest(&(e, r))?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(v) = receipt(&tx, p.as_str(), "media-branch", &r.idempotency_key, &hash)? {
            return Ok(v);
        }
        let source = edition(&tx, p.as_str(), e, Some(&r.source_branch_id))?;
        if source.medium == CreativeMedium::Novel {
            return Err(fail("novel_uses_novel_workspace"));
        }
        let old = head(&tx, &source)?;
        let b = id("branch");
        let snapshot = id("snapshot");
        tx.execute(
            "INSERT INTO story_snapshots_v2 VALUES(?1,?2,?3,?4)",
            params![snapshot, p.as_str(), b, old],
        )
        .map_err(storage)?;
        tx.execute("INSERT INTO story_snapshot_members_v2 SELECT ?1,object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?2",params![snapshot,old]).map_err(storage)?;
        tx.execute(
            "INSERT INTO story_branches_v2 VALUES(?1,?2,?3)",
            params![p.as_str(), b, snapshot],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_scope_branches_v1 VALUES(?1,?2,?3,?4)",
            params![p.as_str(), source.scope_id, b, r.title],
        )
        .map_err(storage)?;
        let result = edition_at_snapshot(&tx, p.as_str(), edition(&tx, p.as_str(), e, Some(&b))?, &snapshot)?;
        save_receipt(
            &tx,
            p.as_str(),
            "media-branch",
            &r.idempotency_key,
            &hash,
            &result,
        )?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

fn save_multimedia_document(
        &mut self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        r: &SaveMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        self.save_multimedia_document_bound(p, e, branch, DocumentChange::Save(r), None)
    }

fn save_task_multimedia_document(
        &mut self,
        task: &crate::TaskRecord,
        r: &SaveMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let scope = self.media_task_scope(task)?;
        self.save_multimedia_document_bound(
            &task.project_id,
            &scope.id,
            Some(&scope.branch_id),
            DocumentChange::Save(r),
            Some(task),
        )
    }

fn patch_multimedia_document(
        &mut self,
        p: &ProjectId,
        e: &str,
        branch: Option<&str>,
        r: &PatchMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        self.save_multimedia_document_bound(p, e, branch, DocumentChange::Patch(r), None)
    }

fn patch_task_multimedia_document(
        &mut self,
        task: &crate::TaskRecord,
        r: &PatchMultimediaDocument,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let scope = self.media_task_scope(task)?;
        self.save_multimedia_document_bound(
            &task.project_id,
            &scope.id,
            Some(&scope.branch_id),
            DocumentChange::Patch(r),
            Some(task),
        )
    }

fn begin_media_import(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &BeginMediaImport,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        validate::key(&r.operation_id)?;
        validate::key(&r.idempotency_key)?;
        validate::text(&r.name, 255, true)?;
        let hash = digest(&(e, r))?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(v) = receipt(&tx, p.as_str(), "media-import", &r.idempotency_key, &hash)? {
            return Ok(v);
        }
        let edition = edition(&tx, p.as_str(), e, None)?;
        tx.execute(
            "INSERT INTO story_media_imports_v1 VALUES(?1,?2,?3,?4,?5,'receiving',?6,NULL)",
            params![
                r.operation_id,
                p.as_str(),
                edition.scope_id,
                r.name,
                hash,
                now + 86400
            ],
        )
        .map_err(storage)?;
        let result = json!({"operationId":r.operation_id,"projectId":p.as_str(),"scopeId":edition.scope_id,"status":"receiving","leaseExpiresAt":now+86400});
        save_receipt(
            &tx,
            p.as_str(),
            "media-import",
            &r.idempotency_key,
            &hash,
            &result,
        )?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

fn register_media_asset(
        &mut self,
        p: &ProjectId,
        e: &str,
        r: &RegisterMediaAsset,
        now: i64,
    ) -> Result<MediaAsset, RepositoryError> {
        validate::key(&r.operation_id)?;
        validate::key(&r.idempotency_key)?;
        validate::asset(&r.asset)?;
        if r.sources.len() > 100 {
            return Err(fail("media_source_limit"));
        }
        let hash = digest(&(e, r))?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(v) = receipt(&tx, p.as_str(), "media-asset", &r.idempotency_key, &hash)? {
            return Ok(v);
        }
        let edition = edition(&tx, p.as_str(), e, None)?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_media_imports_v1 WHERE operation_id=?1 AND project_id=?2 AND scope_id=?3 AND status='receiving' AND lease_expires_at>=?4)",params![r.operation_id,p.as_str(),edition.scope_id,now],|r|r.get(0)).map_err(storage)?;
        if !valid {
            return Err(fail("media_import_not_active"));
        }
        for source in &r.sources {
            validate_source(&tx, p.as_str(), source, MediaFreshness::Pinned)?;
        }
        let a = &r.asset;
        let asset = MediaAsset {
            id: id("asset"),
            asset_revision_id: id("revision"),
            project_id: p.to_string(),
            scope_id: edition.scope_id.clone(),
            name: a.name.clone(),
            kind: a.kind.clone(),
            mime_type: a.mime_type.clone(),
            digest: a.digest.clone(),
            bytes: a.bytes,
            width: a.width,
            height: a.height,
            duration: a.duration.clone(),
            sample_rate: a.sample_rate,
            channels: a.channels,
            preview_url: String::new(),
            created_at: now,
        };
        tx.execute(
            "INSERT INTO story_revisions_v2 VALUES(?1,?2,?3,'mediaAsset',NULL,?4,?5)",
            params![
                asset.asset_revision_id,
                p.as_str(),
                asset.id,
                asset.digest,
                encode(&asset)?
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_revision_scopes_v1 VALUES(?1,?2,?3)",
            params![asset.asset_revision_id, p.as_str(), asset.scope_id],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_assets_v1 VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                asset.asset_revision_id,
                asset.id,
                p.as_str(),
                asset.scope_id,
                asset.digest,
                encode(&asset)?,
                now
            ],
        )
        .map_err(storage)?;
        for (i, source) in r.sources.iter().enumerate() {
            let link = MediaSourceLink {
                purpose: None, decision: None,
                relation: MediaRelation::DerivedFrom,
                source: source.clone(),
                target_object_id: asset.id.clone(),
                target_selector: MediaSelector::Whole,
            };
            tx.execute(
                "INSERT INTO story_media_source_links_v1 VALUES(?1,?2,?3)",
                params![asset.asset_revision_id, i as i64, encode(&link)?],
            )
            .map_err(storage)?;
        }
        tx.execute("UPDATE story_media_imports_v1 SET status='registered',asset_revision_id=?1 WHERE operation_id=?2",params![asset.asset_revision_id,r.operation_id]).map_err(storage)?;
        save_receipt(
            &tx,
            p.as_str(),
            "media-asset",
            &r.idempotency_key,
            &hash,
            &asset,
        )?;
        tx.commit().map_err(storage)?;
        Ok(asset)
    }

fn media_task_scope(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<CreativeEdition, RepositoryError> {
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&self.connection, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        assert_task_scope(&self.connection, task, &e.id, Some(&e.branch_id))
    }

fn media_task_working_workspace(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        crate::task_input::assert_task_inputs_current(&tx, task)?;
        let assets = operations::allowed_input_assets(&tx, task, &e)?
            .into_iter()
            .collect::<Vec<_>>();
        let mut w = workspace_at(
            &tx,
            task.project_id.as_str(),
            &e.id,
            Some(&e.branch_id),
            None,
            Some(&assets),
        )?;
        w.revision = task.input_revision;
        tx.commit().map_err(storage)?;
        Ok(w)
    }

fn media_task_workspace(
        &self,
        task: &crate::TaskRecord,
    ) -> Result<MultimediaWorkspace, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        let (snapshot,assets):(String,String)=tx.query_row("SELECT snapshot_id,asset_revisions_json FROM story_media_task_bindings_v1 WHERE task_id=?1",[&task.task_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(storage)?;
        let mut w = workspace_at(
            &tx,
            task.project_id.as_str(),
            &e.id,
            Some(&e.branch_id),
            Some(&snapshot),
            Some(&decode::<Vec<String>>(&assets)?),
        )?;
        w.revision = task.input_revision;
        tx.commit().map_err(storage)?;
        Ok(w)
    }
}

pub use operations::MultimediaOperationsRepository;

pub use source_reader::MultimediaSourcesRepository;

pub use adaptation::MultimediaAdaptationRepository;

pub use catalog::MultimediaCatalogRepository;

pub use canon::MultimediaCanonRepository;

pub use impact::MultimediaImpactRepository;
