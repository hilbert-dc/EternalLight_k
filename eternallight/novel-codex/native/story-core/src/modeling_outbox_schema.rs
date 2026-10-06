pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_modeling_outbox_v1 (
 project_id TEXT NOT NULL, object_id TEXT NOT NULL, source_revision_id TEXT NOT NULL,
 origin_snapshot_id TEXT NOT NULL, generation INTEGER NOT NULL DEFAULT 1,
 status TEXT NOT NULL CHECK(status IN ('pending','satisfied')),
 first_dirty_ms INTEGER NOT NULL, last_dirty_ms INTEGER NOT NULL, completed_batch_id TEXT,
 PRIMARY KEY(project_id,object_id),
 FOREIGN KEY(project_id,object_id) REFERENCES story_object_heads_v2(project_id,object_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS story_modeling_outbox_pending_v1 ON story_modeling_outbox_v1(project_id,status);
CREATE TABLE IF NOT EXISTS story_modeling_outbox_state_v1 (
 project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 generation INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_watermark_insert_v1 AFTER INSERT ON story_modeling_outbox_v1
BEGIN
 INSERT INTO story_modeling_outbox_state_v1(project_id,generation) VALUES(NEW.project_id,1)
 ON CONFLICT(project_id) DO UPDATE SET generation=generation+1;
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_watermark_update_v1 AFTER UPDATE ON story_modeling_outbox_v1
BEGIN
 UPDATE story_modeling_outbox_state_v1 SET generation=generation+1 WHERE project_id=NEW.project_id;
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_watermark_delete_v1 AFTER DELETE ON story_modeling_outbox_v1
BEGIN
 UPDATE story_modeling_outbox_state_v1 SET generation=generation+1 WHERE project_id=OLD.project_id;
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_insert_v1 AFTER INSERT ON story_object_heads_v2
WHEN NEW.object_kind='draft' AND COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id) IS NOT NULL
BEGIN
 INSERT INTO story_modeling_outbox_v1(project_id,object_id,source_revision_id,origin_snapshot_id,status,first_dirty_ms,last_dirty_ms)
 VALUES(NEW.project_id,NEW.object_id,COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id),
 (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=NEW.project_id AND branch_id='main'),
 'pending',CAST((julianday('now')-2440587.5)*86400000 AS INTEGER),CAST((julianday('now')-2440587.5)*86400000 AS INTEGER));
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_update_v1 AFTER UPDATE OF candidate_revision_id,accepted_revision_id ON story_object_heads_v2
WHEN NEW.object_kind='draft' AND COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id) IS NOT NULL
 AND COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id) IS NOT COALESCE(OLD.candidate_revision_id,OLD.accepted_revision_id)
BEGIN
 INSERT INTO story_modeling_outbox_v1(project_id,object_id,source_revision_id,origin_snapshot_id,status,first_dirty_ms,last_dirty_ms)
 VALUES(NEW.project_id,NEW.object_id,COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id),
 (SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=NEW.project_id AND branch_id='main'),
 'pending',CAST((julianday('now')-2440587.5)*86400000 AS INTEGER),CAST((julianday('now')-2440587.5)*86400000 AS INTEGER))
 ON CONFLICT(project_id,object_id) DO UPDATE SET source_revision_id=excluded.source_revision_id,
 origin_snapshot_id=excluded.origin_snapshot_id,generation=generation+1,status='pending',completed_batch_id=NULL,
 first_dirty_ms=CASE WHEN status='pending' THEN first_dirty_ms ELSE excluded.first_dirty_ms END,last_dirty_ms=excluded.last_dirty_ms;
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_source_removed_v1 AFTER UPDATE OF candidate_revision_id,accepted_revision_id ON story_object_heads_v2
WHEN NEW.object_kind='draft' AND COALESCE(NEW.candidate_revision_id,NEW.accepted_revision_id) IS NULL
BEGIN
 DELETE FROM story_modeling_outbox_v1 WHERE project_id=NEW.project_id AND object_id=NEW.object_id;
END;
CREATE TRIGGER IF NOT EXISTS story_modeling_outbox_snapshot_v1 AFTER UPDATE OF head_snapshot_id ON story_branches_v2
WHEN NEW.branch_id='main'
BEGIN
 UPDATE story_modeling_outbox_v1 SET origin_snapshot_id=NEW.head_snapshot_id
 WHERE project_id=NEW.project_id AND status='pending'
 AND EXISTS(SELECT 1 FROM story_snapshot_members_v2 m WHERE m.snapshot_id=NEW.head_snapshot_id AND m.object_id=story_modeling_outbox_v1.object_id AND m.revision_id=story_modeling_outbox_v1.source_revision_id)
 AND NOT EXISTS(SELECT 1 FROM story_snapshot_members_v2 m WHERE m.snapshot_id=origin_snapshot_id AND m.object_id=story_modeling_outbox_v1.object_id AND m.revision_id=story_modeling_outbox_v1.source_revision_id);
END;
"#;
