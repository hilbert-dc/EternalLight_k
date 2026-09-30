/// Published source payloads participate in all dependent assurance epochs.
/// This is a fault-detection fence, not permission to mutate immutable source authority.
pub(crate) const SCHEMA: &str = r#"
CREATE TRIGGER IF NOT EXISTS story_source_payload_audit_update_v1
AFTER UPDATE OF assertions_json, entities_json ON story_modeling_batches_v2
WHEN (OLD.published_at IS NOT NULL OR NEW.published_at IS NOT NULL)
 AND (OLD.assertions_json IS NOT NEW.assertions_json OR OLD.entities_json IS NOT NEW.entities_json)
BEGIN
  INSERT INTO story_knowledge_index_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'knowledge assurance epoch exhausted') END;
  INSERT INTO story_entity_index_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'entity assurance epoch exhausted') END;
  INSERT INTO story_relation_index_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'relation assurance epoch exhausted') END;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'historical knowledge assurance epoch exhausted') END;
END;

-- Evidence anchors depend on the exact published draft, not just extracted JSON.
-- Normal edits create a new revision; this detects corruption of an existing one.
CREATE TRIGGER IF NOT EXISTS story_source_revision_audit_update_v1
AFTER UPDATE OF content_json, content_digest ON story_revisions_v2
WHEN (OLD.content_json IS NOT NEW.content_json OR OLD.content_digest IS NOT NEW.content_digest)
 AND EXISTS (
   SELECT 1 FROM story_modeling_batches_v2 b
   WHERE b.project_id=OLD.project_id AND b.object_id=OLD.object_id
     AND b.source_revision_id=OLD.revision_id AND b.published_at IS NOT NULL
 )
BEGIN
  INSERT INTO story_knowledge_index_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'knowledge assurance epoch exhausted') END;
  INSERT INTO story_entity_index_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'entity assurance epoch exhausted') END;
  INSERT INTO story_relation_index_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'relation assurance epoch exhausted') END;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation =
    CASE WHEN generation < 9223372036854775807 THEN generation + 1
         ELSE RAISE(ABORT, 'historical knowledge assurance epoch exhausted') END;
END;
"#;

pub(crate) fn index_fences() -> String {
    let mut schema = String::new();
    let indexes: &[(&str, &str, &[&str])] = &[
        (
            "story_entity_claims_v2",
            "entity",
            &[
                "project_id",
                "batch_id",
                "entity_id",
                "kind",
                "display_name",
                "aliases_json",
                "evidence_json",
            ],
        ),
        (
            "story_entity_names_v2",
            "entity",
            &["project_id", "batch_id", "entity_id", "name"],
        ),
        (
            "story_assertion_claims_v1",
            "knowledge",
            &[
                "project_id",
                "batch_id",
                "assertion_index",
                "subject_id",
                "target_id",
                "owner_id",
                "assertion_json",
            ],
        ),
        (
            "story_epistemic_claims_v1",
            "knowledge",
            &[
                "project_id",
                "batch_id",
                "assertion_index",
                "owner_id",
                "proposition_id",
                "proposition_source_batch_id",
                "epistemic_state",
                "epistemic_time",
                "assertion_json",
            ],
        ),
        (
            "story_knowledge_heads_v2",
            "knowledge",
            &["project_id", "object_id", "batch_id"],
        ),
        (
            "story_model_dependencies_v1",
            "knowledge",
            &["project_id", "batch_id", "source_batch_id"],
        ),
        (
            "story_object_heads_v2",
            "knowledge",
            &[
                "project_id",
                "object_id",
                "object_kind",
                "candidate_revision_id",
                "accepted_revision_id",
            ],
        ),
        (
            "story_modeling_batches_v2",
            "knowledge",
            &[
                "project_id",
                "object_id",
                "source_revision_id",
                "status",
                "published_at",
            ],
        ),
    ];
    for (table, index, fields) in indexes {
        let changed = fields
            .iter()
            .map(|field| format!("OLD.{field} IS NOT NEW.{field}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        for (name, action, row, condition) in [
            ("insert", "INSERT", "NEW", "1"),
            ("delete", "DELETE", "OLD", "1"),
            ("update_old", "UPDATE", "OLD", changed.as_str()),
            (
                "update_new",
                "UPDATE",
                "NEW",
                "NEW.project_id IS NOT OLD.project_id",
            ),
        ] {
            let published = match (table, name) {
                (&"story_modeling_batches_v2", "insert") => "NEW.published_at IS NOT NULL",
                (&"story_modeling_batches_v2", "delete") => "OLD.published_at IS NOT NULL",
                (&"story_modeling_batches_v2", "update_old" | "update_new") => {
                    "(OLD.published_at IS NOT NULL OR NEW.published_at IS NOT NULL)"
                }
                _ => "1",
            };
            schema.push_str(&format!(
                "CREATE TRIGGER IF NOT EXISTS {table}_{name}_assurance AFTER {action} ON {table} WHEN ({condition}) AND {published} AND EXISTS(SELECT 1 FROM story_projects_v2 WHERE project_id={row}.project_id) BEGIN \
                 INSERT INTO story_{index}_index_state_v1(project_id,generation) VALUES({row}.project_id,1) \
                 ON CONFLICT(project_id) DO UPDATE SET generation=CASE WHEN generation<9223372036854775807 THEN generation+1 ELSE RAISE(ABORT,'{index} assurance epoch exhausted') END; END;"
            ));
        }
    }
    schema
}
