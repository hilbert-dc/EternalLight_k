//! Author-selected adaptation inputs, paged at one immutable branch snapshot.
use super::*;



/// Public multimedia catalog capability; SQLite retains transaction ownership.
pub trait MultimediaCatalogRepository {
fn media_source_catalog(
        &self, p: &ProjectId, edition_id: &str, snapshot: Option<&str>, offset: usize, limit: usize,
    ) -> Result<Value, RepositoryError>;
}

impl MultimediaCatalogRepository for SqliteStoryRepository {
fn media_source_catalog(
        &self, p: &ProjectId, edition_id: &str, snapshot: Option<&str>, offset: usize, limit: usize,
    ) -> Result<Value, RepositoryError> {
        if limit == 0 || limit > 100 { return Err(fail("media_page_limit_invalid")); }
        if offset > 0 && snapshot.is_none() { return Err(fail("media_source_snapshot_required")); }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let e = edition(&tx, p.as_str(), edition_id, None)?;
        let snapshot = match snapshot { Some(s) => { validate::key(s)?; s.to_owned() }, None => head(&tx, &e)? };
        let reachable: bool = tx.query_row(
            "WITH RECURSIVE chain(id) AS (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id=?2 UNION SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN chain c ON s.snapshot_id=c.id WHERE s.parent_snapshot_id IS NOT NULL) SELECT EXISTS(SELECT 1 FROM chain WHERE id=?3)",
            params![p.as_str(), e.branch_id, snapshot], |r| r.get(0)).map_err(storage)?;
        if !reachable { return Err(fail("media_source_snapshot_not_in_branch")); }
        // Only actual saved production/design objects, never task reports or inferred assets.
        let filter = "FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.project_id=?1 WHERE m.snapshot_id=?2 AND ((?3='novel' AND r.object_kind IN ('draft','brief','outline','plan')) OR (r.object_id=?4 AND r.object_kind IN ('comic','animation')))";
        let medium = if e.medium == CreativeMedium::Novel { "novel" } else { "media" };
        let total: usize = tx.query_row(&format!("SELECT COUNT(*) {filter}"), params![p.as_str(), snapshot, medium, e.id], |r| r.get(0)).map_err(storage)?;
        if offset > total { return Err(fail("media_page_offset_invalid")); }
        let mut items = vec![];
        {
            let mut q = tx.prepare(&format!("SELECT r.object_id,r.revision_id,r.content_digest,r.object_kind,substr(COALESCE(json_extract(r.content_json,'$.title'),''),1,160) {filter} ORDER BY r.object_kind,r.object_id LIMIT ?5 OFFSET ?6")).map_err(storage)?;
            let rows = q.query_map(params![p.as_str(), snapshot, medium, e.id, limit, offset], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?))).map_err(storage)?;
            for row in rows {
                let (object, revision, hash, kind, title) = row.map_err(storage)?;
                let source = MediaSourceRef { project_id:p.as_str().into(), scope_id:e.scope_id.clone(), branch_id:e.branch_id.clone(), object_id:object, revision_id:revision, digest:hash, selector:MediaSelector::Whole };
                items.push(json!({"source":source,"kind":kind,"title":title}));
            }
        }
        let next = offset + items.len();
        let result = json!({"projectId":p.as_str(),"editionId":e.id,"branchId":e.branch_id,"snapshotId":snapshot,"items":items,"offset":offset,"total":total,"nextOffset":if next<total {Some(next)} else {None}});
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}
