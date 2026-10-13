//! Single source for every table, index and trigger owned by Story Core.
use std::sync::OnceLock;

pub(crate) fn ddl() -> &'static str {
    static DDL: OnceLock<String> = OnceLock::new();
    DDL.get_or_init(|| {
        let mut ddl = [
            BASE_SCHEMA,
            crate::work_control_commands::SCHEMA,
            crate::conversation_messages::SCHEMA,
            crate::work_pause_barrier::SCHEMA,
            crate::work_interludes::SCHEMA,
            crate::multimedia::SCHEMA,
            crate::work_decision::SCHEMA,
            crate::direct_verification::SCHEMA,
            crate::model_work_plan::SCHEMA,
            crate::work_execution_events::SCHEMA,
            crate::generation_handoff_execution::SCHEMA,
            crate::generation_model_control::SCHEMA,
            crate::task_sources::SCHEMA,
            crate::quality_notebook::SCHEMA,
            crate::delegated_candidates::SCHEMA,
            crate::work_issues::SCHEMA,
            crate::task_workspace::SCHEMA,
            crate::chapter_deletion::SCHEMA,
            crate::chapter_structure::SCHEMA,
            crate::manuscript_catalog::SCHEMA,
            crate::condition_evaluation_jobs::SCHEMA,
        ].join("\n");
        ddl.push_str(&crate::model_dependencies::schema());
        ddl.push_str(crate::source_payload_audit_schema::SCHEMA);
        ddl.push_str(&crate::source_payload_audit_schema::index_fences());
        ddl.push_str(crate::modeling_outbox_schema::SCHEMA);
        ddl.push_str(crate::recovery_operations::SCHEMA);
        ddl.push_str(crate::reading_checkpoint::SCHEMA);
        ddl
    })
}

pub(crate) const BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_branches_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  head_snapshot_id TEXT NOT NULL,
  PRIMARY KEY(project_id, branch_id)
);
CREATE TABLE IF NOT EXISTS story_projects_v2 (
  project_id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  logline TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_workspaces_v2 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  revision INTEGER NOT NULL DEFAULT 0,
  plan_window INTEGER NOT NULL DEFAULT 3 CHECK(plan_window BETWEEN 1 AND 5)
);
CREATE TABLE IF NOT EXISTS story_conversations_v1 (
  thread_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  is_pinned INTEGER NOT NULL DEFAULT 0 CHECK(is_pinned IN (0,1)),
  is_archived INTEGER NOT NULL DEFAULT 0 CHECK(is_archived IN (0,1)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  forked_from_id TEXT REFERENCES story_conversations_v1(thread_id),
  last_turn_id TEXT,
  registration_json TEXT NOT NULL,
  scope_id TEXT,
  version INTEGER NOT NULL DEFAULT 0 CHECK(version >= 0)
);
CREATE INDEX IF NOT EXISTS story_conversations_project_v1
  ON story_conversations_v1(project_id, is_archived, is_pinned DESC, updated_at DESC, thread_id);
CREATE TABLE IF NOT EXISTS story_object_heads_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  candidate_revision_id TEXT,
  accepted_revision_id TEXT,
  locked INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(project_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_chapters_v2 (
  chapter_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  UNIQUE(project_id, number)
);
CREATE TABLE IF NOT EXISTS story_revisions_v2 (
  revision_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL CHECK(length(object_kind) > 0),
  parent_revision_id TEXT,
  content_digest TEXT NOT NULL,
  content_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_search_documents_v2 (
  doc_id INTEGER PRIMARY KEY,
  project_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id) ON DELETE CASCADE,
  field_path TEXT NOT NULL,
  text TEXT NOT NULL,
  UNIQUE(revision_id, field_path)
);
CREATE TABLE IF NOT EXISTS story_search_grams_v2 (
  project_id TEXT NOT NULL,
  gram TEXT NOT NULL,
  doc_id INTEGER NOT NULL REFERENCES story_search_documents_v2(doc_id) ON DELETE CASCADE,
  PRIMARY KEY(project_id, gram, doc_id)
);
CREATE INDEX IF NOT EXISTS story_search_documents_revision_v2
  ON story_search_documents_v2(project_id, revision_id);
CREATE INDEX IF NOT EXISTS story_search_grams_document_v2
  ON story_search_grams_v2(doc_id, project_id, gram);
CREATE TABLE IF NOT EXISTS story_search_index_state_v1 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_search_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','failed','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  index_generation INTEGER NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL,
  error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_search_audit_active_v1
  ON story_search_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
CREATE TABLE IF NOT EXISTS story_snapshots_v2 (
  snapshot_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  parent_snapshot_id TEXT
);
CREATE TABLE IF NOT EXISTS story_hypotheses_v1 (
  hypothesis_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  base_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
  intent TEXT NOT NULL,
  assumptions_json TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(project_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS story_hypotheses_by_project_v1
  ON story_hypotheses_v1(project_id, created_at DESC, hypothesis_id);
CREATE TABLE IF NOT EXISTS story_proposal_sets_v1 (
  proposal_set_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL,
  base_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
  project_revision INTEGER NOT NULL,
  intent TEXT NOT NULL,
  operations_json TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(project_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS story_proposal_sets_by_project_v1
  ON story_proposal_sets_v1(project_id, created_at DESC, proposal_set_id);
CREATE TABLE IF NOT EXISTS story_proposal_applications_v1 (
  application_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  proposal_set_id TEXT NOT NULL REFERENCES story_proposal_sets_v1(proposal_set_id),
  base_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
  result_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
  base_revision INTEGER NOT NULL,
  result_revision INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('applied', 'undone')),
  undone_by_application_id TEXT,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_proposal_application_members_v1 (
  application_id TEXT NOT NULL REFERENCES story_proposal_applications_v1(application_id),
  operation_id TEXT NOT NULL,
  acceptance_commit_id TEXT NOT NULL UNIQUE REFERENCES story_acceptances_v2(commit_id),
  PRIMARY KEY(application_id, operation_id)
);
CREATE TABLE IF NOT EXISTS story_proposal_undo_receipts_v1 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  application_id TEXT NOT NULL REFERENCES story_proposal_applications_v1(application_id),
  undo_application_id TEXT NOT NULL,
  result_revision INTEGER NOT NULL,
  result_snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id),
  PRIMARY KEY(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_snapshot_members_v2 (
  snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id),
  PRIMARY KEY(snapshot_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_snapshot_chapters_v1 (
  snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id) ON DELETE CASCADE,
  chapter_id TEXT NOT NULL,
  number INTEGER NOT NULL,
  PRIMARY KEY(snapshot_id, chapter_id)
);
CREATE TRIGGER IF NOT EXISTS story_snapshot_chapters_capture_v1
AFTER INSERT ON story_snapshots_v2
BEGIN
  INSERT INTO story_snapshot_chapters_v1(snapshot_id, chapter_id, number)
  SELECT NEW.snapshot_id, chapter_id, number FROM story_chapters_v2 WHERE project_id = NEW.project_id
    AND EXISTS(SELECT 1 FROM story_editions_v1 e WHERE e.project_id=NEW.project_id AND e.medium='novel' AND e.main_branch_id=NEW.branch_id);
END;
CREATE TABLE IF NOT EXISTS story_commits_v2 (
  commit_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  base_snapshot_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('applied', 'undone')),
  target_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_receipts_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  commit_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('applied', 'undone')),
  PRIMARY KEY(project_id, branch_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_candidate_receipts_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  PRIMARY KEY(project_id, branch_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_reviews_v2 (
  review_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  checks_json TEXT NOT NULL,
  proposals_json TEXT NOT NULL,
  selections_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('ready', 'accepted', 'stale', 'rejected'))
);
CREATE TABLE IF NOT EXISTS story_facts_v2 (
  fact_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  payload_json TEXT NOT NULL,
  accepted_by_commit_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  reverted_by_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_review_receipts_v2 (
  project_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  review_id TEXT NOT NULL,
  PRIMARY KEY(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_acceptances_v2 (
  commit_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  review_id TEXT,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  accepted_revision_id TEXT NOT NULL,
  previous_accepted_revision_id TEXT,
  base_revision INTEGER NOT NULL,
  result_revision INTEGER NOT NULL,
  base_snapshot_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('accept', 'undo')),
  target_commit_id TEXT,
  undone_by_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_fact_audit_state_v1 (
  project_id TEXT PRIMARY KEY,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_fact_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  source_generation INTEGER NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS story_fact_audit_active_v1
  ON story_fact_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
CREATE TRIGGER IF NOT EXISTS story_fact_audit_project_delete_v1
AFTER DELETE ON story_projects_v2
BEGIN
  DELETE FROM story_fact_audit_state_v1 WHERE project_id = OLD.project_id;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_fact_insert_v1
AFTER INSERT ON story_facts_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_fact_update_v1
AFTER UPDATE ON story_facts_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_fact_delete_v1
AFTER DELETE ON story_facts_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_acceptance_insert_v1
AFTER INSERT ON story_acceptances_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_acceptance_update_v1
AFTER UPDATE ON story_acceptances_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_acceptance_delete_v1
AFTER DELETE ON story_acceptances_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_snapshot_insert_v1
AFTER INSERT ON story_snapshots_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_snapshot_update_v1
AFTER UPDATE ON story_snapshots_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_fact_audit_snapshot_delete_v1
BEFORE DELETE ON story_snapshots_v2
BEGIN
  INSERT INTO story_fact_audit_state_v1(project_id, generation) VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TABLE IF NOT EXISTS story_acceptance_receipts_v2 (
  project_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  commit_id TEXT NOT NULL,
  result_revision INTEGER NOT NULL,
  PRIMARY KEY(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_tasks_v2 (
  task_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  task_kind TEXT NOT NULL,
  purpose TEXT NOT NULL,
  required_output_kind TEXT,
  target_json TEXT NOT NULL,
  input_manifest_json TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  model TEXT,
  turn_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('queued','running','waiting','completed','failed','cancelled')),
  stage TEXT NOT NULL,
  delivery_status TEXT NOT NULL CHECK(delivery_status IN ('pending','ready_for_review','incomplete')),
  output_refs_json TEXT NOT NULL DEFAULT '[]',
  result_summary TEXT NOT NULL DEFAULT '',
  error TEXT,
  capability_hash TEXT NOT NULL,
  capability_expires_at INTEGER NOT NULL,
  capability_revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS story_task_outputs_v2 (
  task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  output_kind TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  working_base_revision_id TEXT REFERENCES story_revisions_v2(revision_id),
  completeness TEXT,
  PRIMARY KEY(task_id, ordinal)
);
CREATE TABLE IF NOT EXISTS story_task_final_replies_v1 (
  task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  receipt_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_author_messages_v1 (
  message_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id),
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
  task_id TEXT REFERENCES story_tasks_v2(task_id),
  generation_run_id TEXT REFERENCES story_generation_runs_v1(run_id),
  review_run_id TEXT REFERENCES story_planning_review_runs_v2(run_id),
  intent_revision INTEGER NOT NULL CHECK(intent_revision BETWEEN 0 AND 16),
  thread_id TEXT,
  author_text TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  CHECK((task_id IS NOT NULL AND generation_run_id IS NULL AND review_run_id IS NULL AND thread_id IS NOT NULL)
     OR (task_id IS NULL AND generation_run_id IS NOT NULL AND review_run_id IS NULL AND thread_id IS NULL)
     OR (task_id IS NULL AND generation_run_id IS NULL AND review_run_id IS NOT NULL AND thread_id IS NULL AND intent_revision=0)),
  UNIQUE(task_id,intent_revision),
  UNIQUE(generation_run_id,intent_revision),
  UNIQUE(review_run_id,intent_revision)
);
CREATE UNIQUE INDEX IF NOT EXISTS story_author_message_root_v1
  ON story_author_messages_v1(work_task_id) WHERE intent_revision=0;
CREATE TABLE IF NOT EXISTS story_task_intents_v1 (
  task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  intent_revision INTEGER NOT NULL CHECK(intent_revision > 0),
  author_message_id TEXT NOT NULL UNIQUE REFERENCES story_author_messages_v1(message_id),
  author_text TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending','recorded','delivered','notSent')),
  created_at INTEGER NOT NULL,
  delivered_at INTEGER,
  PRIMARY KEY(task_id, intent_revision)
);
CREATE UNIQUE INDEX IF NOT EXISTS story_task_intents_pending_v1
  ON story_task_intents_v1(task_id) WHERE status='pending';
CREATE TABLE IF NOT EXISTS story_work_tasks_v1 (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  thread_id TEXT NOT NULL,
  goal TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('conversation','direct','generation')),
  status TEXT NOT NULL CHECK(status IN ('running','paused','blocked','completed','cancelled')),
  version INTEGER NOT NULL DEFAULT 0,
  control_revision INTEGER NOT NULL DEFAULT 0 CHECK(control_revision >= 0),
  input_revision INTEGER NOT NULL,
  error TEXT
);
CREATE TABLE IF NOT EXISTS story_work_budgets_v1 (
  work_task_id TEXT PRIMARY KEY REFERENCES story_work_tasks_v1(id) ON DELETE CASCADE,
  ledger_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_task_provider_budgets_v1 (
  task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  work_task_id TEXT NOT NULL REFERENCES story_work_budgets_v1(work_task_id) ON DELETE CASCADE,
  grant_json TEXT NOT NULL,
  settled INTEGER NOT NULL DEFAULT 0 CHECK(settled IN (0,1))
);
CREATE TABLE IF NOT EXISTS story_work_budget_authorizations_v1 (
  work_task_id TEXT PRIMARY KEY REFERENCES story_work_budgets_v1(work_task_id) ON DELETE CASCADE,
  authorization_json TEXT NOT NULL,
  after_attempt_sequence INTEGER NOT NULL,
  initial_request_digest TEXT,
  initial_authorization_id TEXT,
  granted_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS story_task_provider_budget_active_v1
  ON story_task_provider_budgets_v1(work_task_id) WHERE settled=0;
CREATE TABLE IF NOT EXISTS story_work_steps_v1 (
  id TEXT PRIMARY KEY,
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  operation TEXT NOT NULL,
  target_json TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  depends_on_step_id TEXT REFERENCES story_work_steps_v1(id),
  status TEXT NOT NULL CHECK(status IN ('queued','running','completed','blocked','cancelled','superseded')),
  current_attempt_id TEXT REFERENCES story_tasks_v2(task_id),
  UNIQUE(work_task_id, ordinal)
);

CREATE TABLE IF NOT EXISTS story_work_attempts_v1 (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL UNIQUE REFERENCES story_tasks_v2(task_id),
  step_id TEXT NOT NULL REFERENCES story_work_steps_v1(id),
  attempt_number INTEGER NOT NULL,
  started_at INTEGER NOT NULL DEFAULT (unixepoch()),
  completed_at INTEGER,
  UNIQUE(step_id, attempt_number)
);
CREATE INDEX IF NOT EXISTS story_work_steps_task_v1 ON story_work_steps_v1(work_task_id, ordinal);
CREATE TABLE IF NOT EXISTS story_task_action_requests_v1 (
  action_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  source_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  idempotency_key TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('generateBrief','generateOutline','generatePlan','generateChapter','modelKnowledge','extractCommitments','reviseCommitment','generatePropertyRules')),
  target_json TEXT NOT NULL,
  requested_target_json TEXT NOT NULL,
  intent_revision INTEGER NOT NULL CHECK(intent_revision >= 0),
  instruction TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'proposed' CHECK(status IN ('proposed','declined','queued','running','completed','failed','superseded')),
  result_task_id TEXT,
  error TEXT,
  UNIQUE(source_task_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_execution_scopes_v1 (
  permit_id TEXT PRIMARY KEY,
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
  kind TEXT NOT NULL CHECK(kind IN ('advisory','generation')),
  author_message_id TEXT NOT NULL REFERENCES story_author_messages_v1(message_id),
  source_task_id TEXT REFERENCES story_tasks_v2(task_id),
  generation_run_id TEXT REFERENCES story_generation_runs_v1(run_id),
  scope_json TEXT NOT NULL,
  preview_digest TEXT NOT NULL UNIQUE,
  confirmed_at INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('active','superseded')),
  superseded_by_author_message_id TEXT REFERENCES story_author_messages_v1(message_id),
  CHECK((status='active' AND superseded_by_author_message_id IS NULL)
     OR (status='superseded' AND superseded_by_author_message_id IS NOT NULL)),
  CHECK((kind='advisory' AND source_task_id IS NOT NULL AND generation_run_id IS NULL)
     OR (kind='generation' AND source_task_id IS NULL AND generation_run_id IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS story_execution_scope_active_v1
  ON story_execution_scopes_v1(work_task_id) WHERE status='active';
CREATE UNIQUE INDEX IF NOT EXISTS story_execution_scope_generation_v1
  ON story_execution_scopes_v1(generation_run_id) WHERE kind='generation';
CREATE TABLE IF NOT EXISTS story_action_permits_v1 (
  action_id TEXT PRIMARY KEY REFERENCES story_task_action_requests_v1(action_id) ON DELETE CASCADE,
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id) ON DELETE CASCADE,
  scope_permit_id TEXT REFERENCES story_execution_scopes_v1(permit_id),
  evidence_json TEXT NOT NULL,
  granted_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_task_input_heads_v2 (
  task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  candidate_revision_id TEXT,
  accepted_revision_id TEXT,
  working_revision_id TEXT,
  working_selection_revision INTEGER,
  PRIMARY KEY(task_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_task_input_knowledge_v2 (
  task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  batch_id TEXT NOT NULL,
  PRIMARY KEY(task_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_generation_runs_v1 (
  run_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  state_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_revision_decisions_v1 (
  ordinal INTEGER PRIMARY KEY AUTOINCREMENT,
  decision_id TEXT NOT NULL UNIQUE,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  writer_task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
  assessment_revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id),
  request_json TEXT NOT NULL,
  record_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS story_revision_decisions_writer_v1
  ON story_revision_decisions_v1(project_id, writer_task_id, ordinal DESC);
CREATE TABLE IF NOT EXISTS story_generation_revision_sources_v1 (
  run_id TEXT PRIMARY KEY REFERENCES story_generation_runs_v1(run_id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  decision_id TEXT NOT NULL REFERENCES story_revision_decisions_v1(decision_id),
  source_json TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS story_generation_one_open_run_v1
  ON story_generation_runs_v1(project_id)
  WHERE json_extract(state_json, '$.status') IN ('running', 'paused', 'blocked');
CREATE TABLE IF NOT EXISTS story_working_baselines_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_kind TEXT NOT NULL,
  object_id TEXT NOT NULL,
  revision_id TEXT,
  selection_revision INTEGER NOT NULL,
  selected_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, object_kind, object_id)
);
CREATE TABLE IF NOT EXISTS story_continuation_intents_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  working_revision_id TEXT NOT NULL,
  author_intent TEXT NOT NULL,
  status TEXT NOT NULL,
  task_id TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, target_kind, target_id)
);
CREATE TABLE IF NOT EXISTS story_chapter_plan_bindings_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  plan_revision_id TEXT NOT NULL,
  chapter_plan_id TEXT NOT NULL,
  chapter_id TEXT NOT NULL REFERENCES story_chapters_v2(chapter_id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, chapter_plan_id),
  UNIQUE(project_id, chapter_id)
);
CREATE TABLE IF NOT EXISTS story_domain_receipts_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  operation TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  result_json TEXT NOT NULL,
  PRIMARY KEY(project_id, operation, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_planning_impacts_v2 (
  impact_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  source_fact_id TEXT NOT NULL,
  source_commit_id TEXT NOT NULL,
  target_artifact_kind TEXT NOT NULL CHECK(target_artifact_kind IN ('outline','plan')),
  target_revision_id TEXT NOT NULL,
  target_node_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  dependency_key TEXT NOT NULL,
  predicate TEXT NOT NULL,
  evidence TEXT NOT NULL,
  severity TEXT NOT NULL CHECK(severity IN ('affected','locked-conflict')),
  state TEXT NOT NULL CHECK(state IN ('open','resolved-kept','resolved-false-positive','resolved-updated','superseded')),
  resolution_note TEXT,
  created_revision INTEGER NOT NULL,
  resolved_revision INTEGER,
  reverted_by_commit_id TEXT,
  UNIQUE(project_id, source_fact_id, target_revision_id, target_node_id, dependency_key)
);
CREATE INDEX IF NOT EXISTS story_planning_impacts_project_v2
  ON story_planning_impacts_v2(project_id, state, severity, created_revision DESC);
CREATE TABLE IF NOT EXISTS story_planning_impact_links_v2 (
  impact_id TEXT PRIMARY KEY REFERENCES story_planning_impacts_v2(impact_id) ON DELETE CASCADE,
  candidate_revision_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_planning_review_runs_v2 (
  run_id TEXT PRIMARY KEY,
  work_task_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
  source_task_id TEXT REFERENCES story_tasks_v2(task_id),
  source_intent_revision INTEGER,
  focus TEXT NOT NULL,
  request_key TEXT,
  request_hash TEXT,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('outline','plan')),
  revision_id TEXT NOT NULL,
  target_node_id TEXT,
  author_intent TEXT NOT NULL,
  author_message_id TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  roles_json TEXT NOT NULL,
  source_packet_json TEXT NOT NULL,
  source_packet_hash TEXT NOT NULL,
  checks_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('open','complete','partial','cancelled')),
  synthesis_json TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(source_task_id, request_key),
  CHECK((source_task_id IS NULL AND source_intent_revision IS NULL AND request_key IS NULL AND request_hash IS NULL) OR
        (source_task_id IS NOT NULL AND source_intent_revision >= 0 AND request_key IS NOT NULL AND request_hash IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS story_planning_review_seats_v2 (
  run_id TEXT NOT NULL REFERENCES story_planning_review_runs_v2(run_id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  task_id TEXT UNIQUE REFERENCES story_tasks_v2(task_id),
  PRIMARY KEY(run_id, role)
);
CREATE TABLE IF NOT EXISTS story_planning_review_reports_v2 (
  report_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES story_planning_review_runs_v2(run_id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  task_id TEXT NOT NULL UNIQUE REFERENCES story_tasks_v2(task_id),
  report_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('complete','partial')),
  created_at INTEGER NOT NULL,
  UNIQUE(run_id, role)
);
CREATE TABLE IF NOT EXISTS story_local_metrics_v2 (
  metric TEXT PRIMARY KEY,
  count INTEGER NOT NULL DEFAULT 0,
  total_seconds INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_local_metric_receipts_v2 (
  idempotency_key TEXT PRIMARY KEY,
  metric TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_modeling_batches_v2 (
  batch_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  source_revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id),
  input_project_revision INTEGER NOT NULL,
  extractor_version TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('queued','ready','stale')),
  assertions_json TEXT NOT NULL DEFAULT '[]',
  entities_json TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  published_at INTEGER,
  publisher_task_id TEXT REFERENCES story_tasks_v2(task_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS story_modeling_batches_live_v2
  ON story_modeling_batches_v2(project_id, object_id, source_revision_id, input_project_revision, extractor_version)
  WHERE status IN ('queued','ready');
CREATE INDEX IF NOT EXISTS story_modeling_batches_project_v2
  ON story_modeling_batches_v2(project_id, object_id, created_at DESC);
CREATE TABLE IF NOT EXISTS story_knowledge_heads_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id),
  PRIMARY KEY(project_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_snapshot_knowledge_v2 (
  snapshot_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  batch_id TEXT REFERENCES story_modeling_batches_v2(batch_id),
  PRIMARY KEY(snapshot_id, object_id),
  FOREIGN KEY(snapshot_id, object_id) REFERENCES story_snapshot_members_v2(snapshot_id, object_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS story_snapshot_knowledge_batch_v2
  ON story_snapshot_knowledge_v2(batch_id, snapshot_id);
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_capture_v2
AFTER INSERT ON story_snapshot_members_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_v2(snapshot_id, object_id, batch_id)
  SELECT NEW.snapshot_id, NEW.object_id,
    (SELECT h.batch_id FROM story_knowledge_heads_v2 h
     JOIN story_modeling_batches_v2 b ON b.batch_id = h.batch_id
       AND b.project_id = s.project_id AND b.object_id = NEW.object_id
     WHERE h.project_id = s.project_id AND h.object_id = NEW.object_id
       AND b.status = 'ready' AND b.published_at IS NOT NULL
       AND b.source_revision_id = NEW.revision_id)
  FROM story_snapshots_v2 s WHERE s.snapshot_id = NEW.snapshot_id;
END;
CREATE TABLE IF NOT EXISTS story_snapshot_knowledge_audit_state_v1 (
  project_id TEXT PRIMARY KEY,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_project_delete_v1
AFTER DELETE ON story_projects_v2
BEGIN
  DELETE FROM story_snapshot_knowledge_audit_state_v1 WHERE project_id = OLD.project_id;
END;
CREATE TABLE IF NOT EXISTS story_snapshot_knowledge_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','failed','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  source_generation INTEGER NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL,
  error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_snapshot_knowledge_audit_active_v1
  ON story_snapshot_knowledge_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_capture_insert_v1
AFTER INSERT ON story_snapshot_knowledge_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = NEW.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_capture_update_v1
AFTER UPDATE ON story_snapshot_knowledge_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = OLD.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = NEW.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_capture_delete_v1
AFTER DELETE ON story_snapshot_knowledge_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = OLD.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_member_update_v1
AFTER UPDATE ON story_snapshot_members_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = OLD.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = NEW.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_member_delete_v1
AFTER DELETE ON story_snapshot_members_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  SELECT project_id, 1 FROM story_snapshots_v2 WHERE snapshot_id = OLD.snapshot_id
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_batch_update_v1
AFTER UPDATE OF project_id, object_id, source_revision_id, status, published_at ON story_modeling_batches_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_revision_update_v1
AFTER UPDATE OF project_id, object_id, object_kind ON story_revisions_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_snapshot_delete_v1
BEFORE DELETE ON story_snapshots_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TRIGGER IF NOT EXISTS story_snapshot_knowledge_audit_snapshot_update_v1
BEFORE UPDATE OF project_id, snapshot_id ON story_snapshots_v2
BEGIN
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (OLD.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
  INSERT INTO story_snapshot_knowledge_audit_state_v1(project_id, generation)
  VALUES (NEW.project_id, 1)
  ON CONFLICT(project_id) DO UPDATE SET generation = generation + 1;
END;
CREATE TABLE IF NOT EXISTS story_entity_claims_v2 (
  project_id TEXT NOT NULL,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  aliases_json TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  PRIMARY KEY(batch_id, entity_id)
);
CREATE TABLE IF NOT EXISTS story_entity_names_v2 (
  project_id TEXT NOT NULL,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY(batch_id, entity_id, name)
);
CREATE INDEX IF NOT EXISTS story_entity_names_lookup_v2
  ON story_entity_names_v2(project_id, name, batch_id);
CREATE INDEX IF NOT EXISTS story_entity_claims_lookup_v2
  ON story_entity_claims_v2(project_id, entity_id);
CREATE TABLE IF NOT EXISTS story_assertion_claims_v1 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id) ON DELETE CASCADE,
  assertion_index INTEGER NOT NULL,
  subject_id TEXT,
  target_id TEXT,
  owner_id TEXT,
  assertion_json TEXT NOT NULL,
  PRIMARY KEY(batch_id, assertion_index)
);
CREATE INDEX IF NOT EXISTS story_assertion_subject_v1
  ON story_assertion_claims_v1(project_id, subject_id, batch_id);
CREATE INDEX IF NOT EXISTS story_assertion_target_v1
  ON story_assertion_claims_v1(project_id, target_id, batch_id);
CREATE INDEX IF NOT EXISTS story_assertion_owner_v1
  ON story_assertion_claims_v1(project_id, owner_id, batch_id);
CREATE TABLE IF NOT EXISTS story_relation_claims_v1 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id) ON DELETE CASCADE,
  assertion_index INTEGER NOT NULL,
  subject_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  relation_type TEXT NOT NULL,
  assertion_json TEXT NOT NULL,
  PRIMARY KEY(batch_id, assertion_index)
);
CREATE INDEX IF NOT EXISTS story_relation_subject_v1
  ON story_relation_claims_v1(project_id, subject_id, relation_type, batch_id);
CREATE INDEX IF NOT EXISTS story_relation_target_v1
  ON story_relation_claims_v1(project_id, target_id, relation_type, batch_id);
CREATE TABLE IF NOT EXISTS story_epistemic_claims_v1 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  batch_id TEXT NOT NULL REFERENCES story_modeling_batches_v2(batch_id) ON DELETE CASCADE,
  assertion_index INTEGER NOT NULL,
  owner_id TEXT NOT NULL,
  proposition_id TEXT NOT NULL,
  proposition_source_batch_id TEXT NOT NULL,
  epistemic_state TEXT NOT NULL,
  epistemic_time INTEGER,
  assertion_json TEXT NOT NULL,
  PRIMARY KEY(batch_id, assertion_index)
);
CREATE INDEX IF NOT EXISTS story_epistemic_owner_proposition_v1
  ON story_epistemic_claims_v1(project_id, owner_id, proposition_id, proposition_source_batch_id, batch_id);
CREATE INDEX IF NOT EXISTS story_epistemic_source_batch_v1
  ON story_epistemic_claims_v1(project_id, proposition_source_batch_id, batch_id);
CREATE TABLE IF NOT EXISTS story_knowledge_index_state_v1 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_knowledge_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','failed','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  index_generation INTEGER NOT NULL,
  entity_generation INTEGER NOT NULL,
  validation_version TEXT NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL,
  error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_knowledge_audit_active_v1
  ON story_knowledge_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
CREATE TABLE IF NOT EXISTS story_relation_index_state_v1 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_relation_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','failed','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  index_generation INTEGER NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL,
  error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_relation_audit_active_v1
  ON story_relation_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
CREATE TABLE IF NOT EXISTS story_entity_index_state_v1 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  generation INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_entity_audit_jobs_v1 (
  job_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK(status IN ('queued','running','paused','cancelled','stale','failed','completed')),
  version INTEGER NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL,
  source_max_rowid INTEGER NOT NULL,
  index_generation INTEGER NOT NULL,
  next_rowid INTEGER NOT NULL DEFAULT 0,
  audit_json TEXT NOT NULL,
  error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_entity_audit_active_v1
  ON story_entity_audit_jobs_v1(project_id)
  WHERE status IN ('queued','running','paused');
"#;

