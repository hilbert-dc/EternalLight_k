//! Author-owned shared canon. Editions read their frozen snapshot, never an implicit latest head.
use super::*;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonEntry {
    pub id: String,
    pub category: CanonCategory,
    pub statement: String,
    pub sources: Vec<MediaSourceRef>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CanonCategory {
    Character,
    World,
    Rule,
    Timeline,
    Style,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonDocument {
    pub title: String,
    pub entries: Vec<CanonEntry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonWorkspace {
    pub project_id: String,
    pub scope_id: String,
    pub branch_id: String,
    pub snapshot_id: String,
    pub revision_id: Option<String>,
    pub document: CanonDocument,
    pub document_source: Option<MediaSourceRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCanonDocument {
    pub expected_revision_id: Option<String>,
    pub document: CanonDocument,
    pub idempotency_key: String,
}
pub(super) fn page_document(
    document: &CanonDocument,
    source: Option<&MediaSourceRef>,
    r: &crate::MediaWorkspacePageRequest,
) -> Result<Value, RepositoryError> {
    r.validate_section_parameters()?;
    let offset = r.offset.unwrap_or(0);
    let limit = r.limit.unwrap_or(20);
    if limit == 0 || limit > 100 {
        return Err(fail("media_page_limit_invalid"));
    }
    let all = if r.section.as_deref() == Some("canonSources") {
        let id = r
            .entry_id
            .as_deref()
            .ok_or_else(|| fail("media_canon_source_parameters_invalid"))?;
        document
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| fail("media_canon_entry_not_found"))?
            .sources
            .iter()
            .map(|s| serde_json::to_value(s).map_err(|_| RepositoryError::CorruptState))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        document.entries.iter().map(|e|json!({"id":e.id,"category":e.category,"sourceCount":e.sources.len(),"textFields":[text::descriptor(MediaTextField::CanonStatement{entry_id:e.id.clone()},&e.statement)]})).collect::<Vec<_>>()
    };
    if offset > all.len() {
        return Err(fail("media_page_offset_invalid"));
    }
    let mut response = json!({"section":if r.section.as_deref()==Some("canonSources"){"canonSources"}else{"canon"},"version":"source","partialView":true,"source":source,"title":document.title,"entryId":r.entry_id,"total":all.len(),"offset":offset,"items":[],"nextOffset":null});
    let mut items = vec![];
    for item in all.iter().skip(offset).take(limit) {
        items.push(item.clone());
        response["items"] = json!(items);
        if encode(&response)?.len() > 60 * 1024 {
            items.pop();
            break;
        }
    }
    if items.is_empty() && offset < all.len() {
        return Err(fail("media_entity_too_large_select_smaller_scope"));
    }
    let next = offset + items.len();
    response["items"] = json!(items);
    response["nextOffset"] = json!(if next < all.len() { Some(next) } else { None });
    Ok(response)
}
fn identity(db: &Connection, p: &str) -> Result<(String, String, String), RepositoryError> {
    project(db, p)?;
    db.query_row("SELECT s.scope_id,x.branch_id,b.head_snapshot_id FROM story_creative_scopes_v1 s JOIN story_scope_branches_v1 x ON x.project_id=s.project_id AND x.scope_id=s.scope_id JOIN story_branches_v2 b ON b.project_id=x.project_id AND b.branch_id=x.branch_id WHERE s.project_id=?1 AND s.kind='canon'", [p], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(storage)
}
pub(super) fn at(
    db: &Connection,
    p: &str,
    snapshot: Option<&str>,
) -> Result<CanonWorkspace, RepositoryError> {
    let (scope, branch, current) = identity(db, p)?;
    let snapshot = snapshot.unwrap_or(&current);
    let reachable: bool = db.query_row("WITH RECURSIVE chain(id) AS (SELECT ?1 UNION SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN chain c ON c.id=s.snapshot_id WHERE s.project_id=?2 AND s.branch_id=?3 AND s.parent_snapshot_id IS NOT NULL) SELECT EXISTS(SELECT 1 FROM chain c JOIN story_snapshots_v2 s ON s.snapshot_id=c.id WHERE s.snapshot_id=?4 AND s.project_id=?2 AND s.branch_id=?3)", params![current,p,branch,snapshot], |r|r.get(0)).map_err(storage)?;
    if !reachable {
        return Err(fail("canon_snapshot_scope_mismatch"));
    }
    let revision = member(db, snapshot, &scope)?;
    let (document, source) = if let Some(revision) = &revision {
        let (content, hash): (String,String) = db.query_row("SELECT content_json,content_digest FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3 AND object_kind='canon'", params![p,scope,revision], |r|Ok((r.get(0)?,r.get(1)?))).map_err(storage)?;
        (
            decode(&content)?,
            Some(MediaSourceRef {
                project_id: p.into(),
                scope_id: scope.clone(),
                branch_id: branch.clone(),
                object_id: scope.clone(),
                revision_id: revision.clone(),
                digest: hash,
                selector: MediaSelector::Whole,
            }),
        )
    } else {
        (
            CanonDocument {
                title: "共同设定".into(),
                entries: vec![],
            },
            None,
        )
    };
    Ok(CanonWorkspace {
        project_id: p.into(),
        scope_id: scope,
        branch_id: branch,
        snapshot_id: snapshot.into(),
        revision_id: revision,
        document,
        document_source: source,
    })
}
impl SqliteStoryRepository {
    pub fn canon_workspace(
        &self,
        p: &ProjectId,
        snapshot: Option<&str>,
    ) -> Result<CanonWorkspace, RepositoryError> {
        if let Some(s) = snapshot {
            validate::key(s)?;
        }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let result = at(&tx, p.as_str(), snapshot)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    /// This method is exposed only on the author route. Model statements are proposals until explicitly saved by the author.
    pub fn save_canon_document(
        &mut self,
        p: &ProjectId,
        r: &SaveCanonDocument,
    ) -> Result<CanonWorkspace, RepositoryError> {
        validate::key(&r.idempotency_key)?;
        validate::text(&r.document.title, 160, true)?;
        if r.document.entries.len() > 500 || encode(r)?.len() > 2 * 1024 * 1024 {
            return Err(fail("canon_document_limit"));
        }
        let mut ids = HashSet::new();
        for entry in &r.document.entries {
            validate::key(&entry.id)?;
            validate::text(&entry.statement, 4000, true)?;
            if !ids.insert(&entry.id) || entry.sources.len() > 10 {
                return Err(fail("canon_entry_invalid"));
            }
        }
        let hash = digest(r)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(v) = receipt(&tx, p.as_str(), "canon-save", &r.idempotency_key, &hash)? {
            return Ok(v);
        }
        let current = at(&tx, p.as_str(), None)?;
        if current.revision_id != r.expected_revision_id {
            return Err(fail("media_revision_changed"));
        }
        for entry in &r.document.entries {
            for source in &entry.sources {
                validate_source(&tx, p.as_str(), source, MediaFreshness::Pinned)?;
            }
        }
        let revision = id("revision");
        let snapshot = id("snapshot");
        let commit = id("commit");
        tx.execute(
            "INSERT INTO story_revisions_v2 VALUES(?1,?2,?3,'canon',?4,?5,?6)",
            params![
                revision,
                p.as_str(),
                current.scope_id,
                current.revision_id,
                digest(&r.document)?,
                encode(&r.document)?
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_revision_scopes_v1 VALUES(?1,?2,?3)",
            params![revision, p.as_str(), current.scope_id],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_snapshots_v2 VALUES(?1,?2,?3,?4)",
            params![snapshot, p.as_str(), current.branch_id, current.snapshot_id],
        )
        .map_err(storage)?;
        tx.execute("INSERT INTO story_snapshot_members_v2 SELECT ?1,object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?2 AND object_id!=?3",params![snapshot,current.snapshot_id,current.scope_id]).map_err(storage)?;
        tx.execute(
            "INSERT INTO story_snapshot_members_v2 VALUES(?1,?2,?3)",
            params![snapshot, current.scope_id, revision],
        )
        .map_err(storage)?;
        if tx.execute("UPDATE story_branches_v2 SET head_snapshot_id=?1 WHERE project_id=?2 AND branch_id=?3 AND head_snapshot_id=?4",params![snapshot,p.as_str(),current.branch_id,current.snapshot_id]).map_err(storage)?!=1 { return Err(fail("media_revision_changed")); }
        tx.execute(
            "INSERT INTO story_commits_v2 VALUES(?1,?2,?3,?4,?5,'applied',NULL)",
            params![
                commit,
                p.as_str(),
                current.branch_id,
                current.snapshot_id,
                snapshot
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_receipts_v2 VALUES(?1,?2,?3,?4,?5,?6,'applied')",
            params![
                p.as_str(),
                current.branch_id,
                format!("canon:{}", r.idempotency_key),
                hash,
                commit,
                snapshot
            ],
        )
        .map_err(storage)?;
        let result = at(&tx, p.as_str(), None)?;
        save_receipt(
            &tx,
            p.as_str(),
            "canon-save",
            &r.idempotency_key,
            &hash,
            &result,
        )?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    pub fn media_task_canon(
        &self,
        task: &crate::TaskRecord,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if limit == 0 || limit > 100 {
            return Err(fail("media_page_limit_invalid"));
        }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let e = assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        let canon = at(
            &tx,
            task.project_id.as_str(),
            e.canon_snapshot_id.as_deref(),
        )?;
        if offset > canon.document.entries.len() {
            return Err(fail("media_page_offset_invalid"));
        }
        let mut items = vec![];
        let mut bytes = 1500;
        for entry in canon.document.entries.iter().skip(offset).take(limit) {
            let v = json!({"id":entry.id,"category":entry.category,"sourceCount":entry.sources.len(),"textFields":[text::descriptor(MediaTextField::CanonStatement{entry_id:entry.id.clone()},&entry.statement)]});
            let n = v.to_string().len();
            if bytes + n > 60 * 1024 {
                if items.is_empty() {
                    return Err(fail("media_entity_too_large_select_smaller_scope"));
                }
                break;
            }
            bytes += n;
            items.push(v);
        }
        let next = offset + items.len();
        let result = json!({"section":"canon","version":"source","partialView":true,"projectId":canon.project_id,"scopeId":canon.scope_id,"branchId":canon.branch_id,"snapshotId":canon.snapshot_id,"revisionId":canon.revision_id,"documentSource":canon.document_source,"title":canon.document.title,"items":items,"total":canon.document.entries.len(),"offset":offset,"nextOffset":if next<canon.document.entries.len(){Some(next)}else{None}});
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    pub fn media_task_canon_text(
        &self,
        task: &crate::TaskRecord,
        r: &crate::MediaWorkspacePageRequest,
    ) -> Result<Value, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let e = assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        let canon = at(
            &tx,
            task.project_id.as_str(),
            e.canon_snapshot_id.as_deref(),
        )?;
        let source = canon
            .document_source
            .ok_or_else(|| fail("media_text_unsaved"))?;
        let mut result = text::page(
            &source,
            &serde_json::to_value(&canon.document).map_err(|_| RepositoryError::CorruptState)?,
            r,
        )?;
        result["version"] = json!("source");
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    pub fn media_task_canon_sources(
        &self,
        task: &crate::TaskRecord,
        r: &crate::MediaWorkspacePageRequest,
    ) -> Result<Value, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let e = assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        let canon = at(
            &tx,
            task.project_id.as_str(),
            e.canon_snapshot_id.as_deref(),
        )?;
        let mut result = page_document(&canon.document, canon.document_source.as_ref(), r)?;
        result["snapshotId"] = json!(canon.snapshot_id);
        result["revisionId"] = json!(canon.revision_id);
        result["projectId"] = json!(canon.project_id);
        result["scopeId"] = json!(canon.scope_id);
        result["branchId"] = json!(canon.branch_id);
        if encode(&result)?.len() > 64 * 1024 {
            return Err(fail("media_entity_too_large_select_smaller_scope"));
        }
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}
