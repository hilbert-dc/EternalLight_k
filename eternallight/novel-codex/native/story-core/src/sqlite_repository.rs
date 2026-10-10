use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;

use crate::BranchId;
use crate::CommitId;
use crate::CommitReceipt;
use crate::CommitRequest;
use crate::CommitStatus;
use crate::IdempotencyKey;
use crate::ObjectId;
use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::RevisionRecord;
use crate::Snapshot;
use crate::SnapshotId;
use crate::UndoRequest;

const SCHEMA: &str = r#"
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
  status TEXT NOT NULL CHECK(status IN ('pending','delivered','notSent')),
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

/// Durable Story Core repository. Every state transition is committed in one
/// SQLite transaction; callers never need to mirror an in-memory authority.
pub struct SqliteStoryRepository {
    pub(crate) connection: Connection,
    pub(crate) recovery_operation: Option<crate::RecoveryOperationBinding>,
    pub(crate) generation_work_control_guard: Option<(String, i64)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    pub project_id: ProjectId,
    pub title: String,
    pub logline: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectHead {
    pub project_id: ProjectId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub candidate_revision_id: Option<RevisionId>,
    pub accepted_revision_id: Option<RevisionId>,
    pub locked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveCandidateRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub revision_id: RevisionId,
    pub commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub expected_project_revision: i64,
    pub expected_snapshot_id: SnapshotId,
    pub next_snapshot_id: SnapshotId,
    pub expected_candidate_revision_id: Option<RevisionId>,
    pub content_digest: String,
    pub content_json: String,
    pub chapter_projection: Option<ChapterProjection>,
    pub task_output: Option<TaskOutputBinding>,
    pub linked_impact_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateReceipt {
    pub revision_id: RevisionId,
    pub result_snapshot_id: SnapshotId,
    pub project_revision: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChapterProjection {
    pub chapter_id: String,
    pub number: i64,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChapterRecord {
    pub chapter_id: String,
    pub project_id: ProjectId,
    pub number: i64,
    pub title: String,
    pub status: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskOutputBinding {
    pub task_id: String,
    pub thread_id: String,
    pub output_kind: String,
    pub completeness: Option<String>,
}

impl SqliteStoryRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(storage_error)?;
        let repository = Self::from_connection(connection)?;
        // The file store serves foreground writes and background audits together.
        // Re-selecting WAL on every read connection would itself contend for locks.
        let mode: String = repository
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(storage_error)?;
        if mode != "wal" {
            let selected: String = repository
                .connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(storage_error)?;
            if selected != "wal" {
                return Err(RepositoryError::Storage(
                    "file story storage requires WAL journal mode".into(),
                ));
            }
        }
        Ok(repository)
    }

    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, RepositoryError> {
        crate::current_schema::validate(&connection, SCHEMA)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; PRAGMA synchronous = FULL;",
            )
            .map_err(storage_error)?;
        connection.execute_batch(SCHEMA).map_err(storage_error)?;
        connection.execute_batch(crate::work_control_commands::SCHEMA).map_err(storage_error)?;
        connection.execute_batch(crate::conversation_messages::SCHEMA).map_err(storage_error)?;
        connection.execute_batch(crate::work_pause_barrier::SCHEMA).map_err(storage_error)?;
        connection.execute_batch(crate::work_interludes::SCHEMA).map_err(storage_error)?;
        connection.execute_batch(crate::multimedia::SCHEMA).map_err(storage_error)?;
        connection
            .execute_batch(&format!(
                "{}{}{}",
                crate::work_decision::SCHEMA,
                crate::direct_verification::SCHEMA,
                crate::model_work_plan::SCHEMA
            ))
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::work_execution_events::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::generation_handoff_execution::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::generation_model_control::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::task_sources::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::quality_notebook::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::delegated_candidates::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::work_issues::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::task_workspace::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::chapter_deletion::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::chapter_structure::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::manuscript_catalog::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::condition_evaluation_jobs::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(&crate::model_dependencies::schema())
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::source_payload_audit_schema::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(&crate::source_payload_audit_schema::index_fences())
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::modeling_outbox_schema::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::recovery_operations::SCHEMA)
            .map_err(storage_error)?;
        connection
            .execute_batch(crate::reading_checkpoint::SCHEMA)
            .map_err(storage_error)?;
        Ok(Self {
            connection,
            recovery_operation: None,
            generation_work_control_guard: None,
        })
    }

    pub fn create_project(
        &mut self,
        project: ProjectRecord,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<ProjectRecord, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if exists(
            &transaction,
            "SELECT 1 FROM story_projects_v2 WHERE project_id = ?1",
            [project.project_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "project",
                project.project_id.to_string(),
            ));
        }
        require_unused_snapshot(&transaction, &snapshot_id)?;
        transaction
            .execute(
                "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project.project_id.as_str(), project.title, project.logline, project.created_at, project.updated_at],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_workspaces_v2(project_id, revision) VALUES (?1, 0)",
                [project.project_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, NULL)",
                params![snapshot_id.as_str(), project.project_id.as_str(), branch_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_branches_v2(project_id, branch_id, head_snapshot_id) VALUES (?1, ?2, ?3)",
                params![project.project_id.as_str(), branch_id.as_str(), snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        crate::multimedia::initialize_project(&transaction, project.project_id.as_str(), &project.title, branch_id.as_str(), crate::CreativeMedium::Novel, project.created_at)?;
        transaction.commit().map_err(storage_error)?;
        Ok(project)
    }

    pub fn project_revision(&self, project_id: &ProjectId) -> Result<i64, RepositoryError> {
        self.connection
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)
    }

    pub fn object_heads(&self, project_id: &ProjectId) -> Result<Vec<ObjectHead>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT project_id, object_id, object_kind, candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 ORDER BY object_kind, object_id",
            )
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], object_head_from_row)
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }

    pub fn set_object_lock(
        &mut self,
        project_id: &ProjectId,
        object_id: &ObjectId,
        locked: bool,
        expected_project_revision: i64,
    ) -> Result<i64, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let revision = current_project_revision(&transaction, project_id)?;
        if revision != expected_project_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: expected_project_revision,
                actual: revision,
            });
        }
        let accepted = transaction
            .query_row(
                "SELECT accepted_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![project_id.as_str(), object_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(storage_error)?
            .flatten()
            .ok_or(RepositoryError::CandidateNotFound)?;
        let current = transaction
            .query_row(
                "SELECT locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2 AND accepted_revision_id = ?3",
                params![project_id.as_str(), object_id.as_str(), accepted],
                |row| row.get::<_, bool>(0),
            )
            .map_err(storage_error)?;
        if current == locked {
            return Ok(revision);
        }
        let next = revision + 1;
        transaction
            .execute(
                "UPDATE story_object_heads_v2 SET locked = ?1 WHERE project_id = ?2 AND object_id = ?3",
                params![locked, project_id.as_str(), object_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2",
                params![next, project_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(next)
    }

    pub fn save_candidate(
        &mut self,
        request: SaveCandidateRequest,
    ) -> Result<CandidateReceipt, RepositoryError> {
        serde_json::from_str::<serde_json::Value>(&request.content_json)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        if request.object_kind.trim().is_empty() {
            return Err(RepositoryError::InvalidObjectKind);
        }
        if request.object_kind == "sceneAssessment" && request.task_output.is_none() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "commitmentExtraction"
            && (request.branch_id.as_str() != "main"
                || request.chapter_projection.is_some()
                || request
                    .task_output
                    .as_ref()
                    .is_none_or(|binding| binding.output_kind != "commitmentExtraction"))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "commitmentExtraction")
            && request.object_kind != "commitmentExtraction"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "sceneAssessment")
            && request.object_kind != "sceneAssessment"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "revisionOutcome"
            && (request.branch_id.as_str() != "main"
                || request.chapter_projection.is_some()
                || request
                    .task_output
                    .as_ref()
                    .is_none_or(|binding| binding.output_kind != "revisionOutcome"))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "revisionOutcome")
            && request.object_kind != "revisionOutcome"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "identity" {
            if request.object_id.as_str() != "artifact-identity" {
                return Err(RepositoryError::InvalidObjectKind);
            }
            crate::identity_decisions::validate_identity_content(&request.content_json)?;
        }
        if request.object_kind == "propertyRules" {
            if request.branch_id.as_str() != "main" || request.chapter_projection.is_some() {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            crate::property_rules::validate_write(&request.object_id, &request.content_json)?;
            let content: serde_json::Value = serde_json::from_str(&request.content_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            let source_valid = match (content["source"].as_str(), request.task_output.as_ref()) {
                (Some("author"), None) => true,
                (Some("agent"), Some(binding)) => binding.output_kind == "propertyRules",
                _ => false,
            };
            if !source_valid {
                return Err(RepositoryError::TaskOutputForbidden);
            }
        }
        if request.object_kind == "commitment"
            && (request.branch_id.as_str() != "main"
                || request
                    .task_output
                    .as_ref()
                    .is_some_and(|binding| binding.output_kind != "commitment")
                || request.chapter_projection.is_some())
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "commitment")
            && request.object_kind != "commitment"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_candidate_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            crate::recovery_operations::commit_candidate(
                &transaction,
                self.recovery_operation.as_ref(),
                &request.project_id,
                request.task_output.as_ref(),
                &request.object_kind,
                receipt.revision_id.as_str(),
            )?;
            transaction.commit().map_err(storage_error)?;
            return Ok(receipt);
        }
        let project_revision = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [request.project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        if project_revision != request.expected_project_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_project_revision,
                actual: project_revision,
            });
        }
        if let Some(binding) = &request.task_output
            && let Err(error) = crate::tasks::validate_task_write(
                &transaction,
                &request.project_id,
                binding,
                project_revision,
                &request.object_id,
                &request.expected_candidate_revision_id,
                &request.content_json,
            )
        {
            // Task validation precedes every candidate domain write. Only these
            // constructor-owned rejections prove this exact call has no receipt.
            if matches!(&error,
                RepositoryError::InvalidContent(_)
                    | RepositoryError::PayloadValidation { .. }
                    | RepositoryError::TaskBindingInvalid
                    | RepositoryError::TaskInputStale
                    | RepositoryError::TaskNotWritable
                    | RepositoryError::TaskOutputForbidden
                    | RepositoryError::TaskOutputMissing
                    | RepositoryError::WorkTaskStale
                    | RepositoryError::AuthoringRunStale
                    | RepositoryError::StaleProjectRevision { .. }
            )
            {
                crate::recovery_operations::reject_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    &request.project_id,
                    &binding.task_id,
                    crate::recovery_operations::candidate_tool(&request.object_kind)?,
                )?;
                transaction.commit().map_err(storage_error)?;
            }
            return Err(error);
        }
        let head = require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.expected_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;
        if let Some(chapter) = &request.chapter_projection {
            crate::chapter_deletion::assert_not_deleted(&transaction, &chapter.chapter_id)?;
            if chapter.number < 1 || chapter.title.chars().count() > 160 {
                return Err(RepositoryError::InvalidChapter);
            }
            if exists(
                &transaction,
                "SELECT 1 FROM story_chapters_v2 WHERE project_id = ?1 AND number = ?2 AND chapter_id <> ?3",
                params![
                    request.project_id.as_str(),
                    chapter.number,
                    chapter.chapter_id
                ],
            )? {
                return Err(RepositoryError::InvalidChapter);
            }
            let existing_project = transaction
                .query_row(
                    "SELECT project_id FROM story_chapters_v2 WHERE chapter_id = ?1",
                    [&chapter.chapter_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?;
            match existing_project {
                Some(existing_project) if existing_project != request.project_id.as_str() => {
                    return Err(RepositoryError::CrossProjectReference);
                }
                Some(_) => {
                    transaction
                        .execute(
                            "UPDATE story_chapters_v2 SET number = ?1, title = ?2 WHERE project_id = ?3 AND chapter_id = ?4",
                            params![chapter.number, chapter.title, request.project_id.as_str(), chapter.chapter_id],
                        )
                        .map_err(storage_error)?;
                }
                None => {
                    transaction
                        .execute(
                            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
                            params![chapter.chapter_id, request.project_id.as_str(), chapter.number, chapter.title],
                        )
                        .map_err(storage_error)?;
                }
            }
        }
        if exists(
            &transaction,
            "SELECT 1 FROM story_revisions_v2 WHERE revision_id = ?1",
            [request.revision_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "revision",
                request.revision_id.to_string(),
            ));
        }
        let existing_head = transaction
            .query_row(
                "SELECT object_kind, candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![request.project_id.as_str(), request.object_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, bool>(3)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let (candidate, accepted) = if let Some((kind, candidate, accepted, locked)) = existing_head
        {
            if kind != request.object_kind {
                return Err(RepositoryError::InvalidObjectKind);
            }
            if locked {
                return Err(RepositoryError::ObjectLocked);
            }
            (candidate, accepted)
        } else {
            (None, None)
        };
        let current_candidate = candidate
            .map(RevisionId::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?;
        if current_candidate != request.expected_candidate_revision_id {
            return Err(RepositoryError::StaleCandidate {
                expected: request.expected_candidate_revision_id,
                actual: current_candidate,
            });
        }
        // Extraction forbids chapter projection, so this phase has no domain writes.
        // Only an explicit validation rejection can be settled without a receipt.
        if request.object_kind == "commitmentExtraction"
            && let Err(error) = crate::commitment_extraction::validate_write(
                &transaction,
                &request.project_id,
                &request.object_id,
                &request.content_json,
            )
        {
            if matches!(&error, RepositoryError::InvalidContent(_)) {
                let binding = request
                    .task_output
                    .as_ref()
                    .ok_or(RepositoryError::TaskOutputForbidden)?;
                crate::recovery_operations::reject_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    &request.project_id,
                    &binding.task_id,
                    "submit_commitment_extraction",
                )?;
                transaction.commit().map_err(storage_error)?;
            }
            return Err(error);
        }
        let parent_revision_id =
            current_candidate.or_else(|| accepted.and_then(|value| RevisionId::parse(value).ok()));
        let working_source_revision_id = parent_revision_id.clone();
        let revision = RevisionRecord {
            project_id: request.project_id.clone(),
            object_id: request.object_id.clone(),
            object_kind: request.object_kind.clone(),
            revision_id: request.revision_id.clone(),
            parent_revision_id,
            content_digest: request.content_digest,
            content_json: request.content_json,
        };
        insert_revision(&transaction, &revision)?;
        crate::impacts::link_candidate_impacts(
            &transaction,
            &request.project_id,
            &request.object_kind,
            &request.revision_id,
            &revision.content_json,
            &request.linked_impact_ids,
        )?;
        let mut members = head.members;
        members.insert(request.object_id.clone(), request.revision_id.clone());
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.expected_snapshot_id),
            &members,
        )?;
        let changed = transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.expected_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        if changed != 1 {
            return Err(RepositoryError::CorruptState);
        }
        transaction
            .execute(
                "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(project_id, object_id) DO UPDATE SET candidate_revision_id = excluded.candidate_revision_id",
                params![request.project_id.as_str(), request.object_id.as_str(), request.object_kind, request.revision_id.as_str()],
            )
            .map_err(storage_error)?;
        if request.object_kind == "draft" {
            crate::modeling::invalidate_source_modeling(
                &transaction,
                &request.project_id,
                &request.object_id,
            )?;
        }
        if let Some(binding) = &request.task_output {
            let working_base_revision_id = crate::workspace_selection::task_output_working_base(
                &transaction,
                &binding.task_id,
                request.object_id.as_str(),
                working_source_revision_id.as_ref(),
            )?;
            crate::tasks::record_task_output(
                &transaction,
                binding,
                &request.revision_id,
                &request.object_id,
                working_base_revision_id.as_ref(),
            )?;
            crate::workspace_selection::advance_task_working_source(
                &transaction,
                &request.project_id,
                &binding.task_id,
                &request.object_kind,
                request.object_id.as_str(),
                &request.revision_id,
            )?;
        } else {
            crate::workspace_selection::advance_author_working_source(
                &transaction,
                &request.project_id,
                &request.object_kind,
                request.object_id.as_str(),
                working_source_revision_id.as_ref(),
                &request.revision_id,
            )?;
        }
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'applied', NULL)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.expected_snapshot_id.as_str(), request.next_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        let commit_receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id.clone(),
            status: CommitStatus::Applied,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &commit_receipt,
        )?;
        transaction
            .execute(
                "INSERT INTO story_candidate_receipts_v2(project_id, branch_id, idempotency_key, request_digest, revision_id, result_snapshot_id, project_revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![request.project_id.as_str(), request.branch_id.as_str(), request.idempotency_key.as_str(), request.request_digest, request.revision_id.as_str(), request.next_snapshot_id.as_str(), project_revision],
            )
            .map_err(storage_error)?;
        crate::recovery_operations::commit_candidate(
            &transaction,
            self.recovery_operation.as_ref(),
            &request.project_id,
            request.task_output.as_ref(),
            &request.object_kind,
            request.revision_id.as_str(),
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(CandidateReceipt {
            revision_id: request.revision_id,
            result_snapshot_id: request.next_snapshot_id,
            project_revision,
        })
    }

    pub fn chapters(&self, project_id: &ProjectId) -> Result<Vec<ChapterRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT chapter_id, project_id, number, title, status, summary FROM story_chapters_v2 WHERE project_id = ?1 ORDER BY number")
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage_error)?
            .map(|row| {
                let row = row.map_err(storage_error)?;
                Ok(ChapterRecord {
                    chapter_id: row.0,
                    project_id: ProjectId::parse(row.1)
                        .map_err(|_| RepositoryError::CorruptState)?,
                    number: row.2,
                    title: row.3,
                    status: row.4,
                    summary: row.5,
                })
            })
            .collect()
    }

    pub fn project(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<ProjectRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT project_id, title, logline, created_at, updated_at FROM story_projects_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                project_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT project_id, title, logline, created_at, updated_at FROM story_projects_v2 ORDER BY updated_at DESC, created_at DESC")
            .map_err(storage_error)?;
        statement
            .query_map([], project_from_row)
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }

    pub fn create_branch(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if exists(
            &transaction,
            "SELECT 1 FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
            params![project_id.as_str(), branch_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "branch",
                branch_id.to_string(),
            ));
        }
        require_unused_snapshot(&transaction, &snapshot_id)?;
        transaction
            .execute(
                "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, NULL)",
                params![snapshot_id.as_str(), project_id.as_str(), branch_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_branches_v2(project_id, branch_id, head_snapshot_id) VALUES (?1, ?2, ?3)",
                params![project_id.as_str(), branch_id.as_str(), snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }

    pub fn branch_head(
        &self,
        project_id: &ProjectId,
        branch_id: &BranchId,
    ) -> Result<Option<Snapshot>, RepositoryError> {
        let snapshot_id = self
            .connection
            .query_row(
                "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
                params![project_id.as_str(), branch_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?;
        snapshot_id
            .map(|snapshot_id| load_snapshot(&self.connection, &snapshot_id))
            .transpose()
    }

    pub fn revision_content(
        &self,
        revision_id: &RevisionId,
    ) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE revision_id = ?1",
                [revision_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn apply_commit(
        &mut self,
        request: CommitRequest,
    ) -> Result<CommitReceipt, RepositoryError> {
        if request.revisions.is_empty() {
            return Err(RepositoryError::EmptyCommit);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        let head = require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;

        let mut members = head.members;
        let mut staged = BTreeMap::new();
        for revision in &request.revisions {
            if revision.project_id != request.project_id {
                return Err(RepositoryError::CrossProjectReference);
            }
            if staged
                .insert(revision.object_id.clone(), revision.revision_id.clone())
                .is_some()
            {
                return Err(RepositoryError::DuplicateObjectRevision(
                    revision.object_id.clone(),
                ));
            }
            if exists(
                &transaction,
                "SELECT 1 FROM story_revisions_v2 WHERE revision_id = ?1",
                [revision.revision_id.as_str()],
            )? {
                return Err(RepositoryError::AlreadyExists(
                    "revision",
                    revision.revision_id.to_string(),
                ));
            }
            let expected = members.get(&revision.object_id).cloned();
            if revision.parent_revision_id != expected {
                return Err(RepositoryError::InvalidParent {
                    object_id: revision.object_id.clone(),
                    expected,
                    actual: revision.parent_revision_id.clone(),
                });
            }
            members.insert(revision.object_id.clone(), revision.revision_id.clone());
        }

        for revision in &request.revisions {
            insert_revision(&transaction, revision)?;
        }
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.base_snapshot_id),
            &members,
        )?;
        transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'applied', NULL)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str(), request.next_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        let receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id,
            status: CommitStatus::Applied,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &receipt,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(receipt)
    }

    pub fn undo_latest(&mut self, request: UndoRequest) -> Result<CommitReceipt, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;
        let target = transaction
            .query_row(
                "SELECT project_id, branch_id, base_snapshot_id, result_snapshot_id FROM story_commits_v2 WHERE commit_id = ?1",
                [request.target_commit_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CommitNotFound)?;
        if target.0 != request.project_id.as_str() || target.1 != request.branch_id.as_str() {
            return Err(RepositoryError::CrossProjectReference);
        }
        if target.3 != request.base_snapshot_id.as_str() {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let restored = load_members(&transaction, &target.2)?;
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.base_snapshot_id),
            &restored,
        )?;
        transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'undone', ?6)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str(), request.next_snapshot_id.as_str(), request.target_commit_id.as_str()],
            )
            .map_err(storage_error)?;
        let receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id,
            status: CommitStatus::Undone,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &receipt,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

fn replay_candidate_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
) -> Result<Option<CandidateReceipt>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT request_digest, revision_id, result_snapshot_id, project_revision FROM story_candidate_receipts_v2 WHERE project_id = ?1 AND branch_id = ?2 AND idempotency_key = ?3",
            params![project_id.as_str(), branch_id.as_str(), key.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored_digest, revision_id, snapshot_id, project_revision)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    Ok(Some(CandidateReceipt {
        revision_id: RevisionId::parse(revision_id).map_err(|_| RepositoryError::CorruptState)?,
        result_snapshot_id: SnapshotId::parse(snapshot_id)
            .map_err(|_| RepositoryError::CorruptState)?,
        project_revision,
    }))
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let project_id = row.get::<_, String>(0)?;
    Ok(ProjectRecord {
        project_id: ProjectId::parse(project_id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        title: row.get(1)?,
        logline: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn object_head_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectHead> {
    let parse_id = |index, value: String| {
        ProjectId::parse(value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    };
    let project_id = parse_id(0, row.get(0)?)?;
    let object_id = ObjectId::parse(row.get::<_, String>(1)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let candidate = row
        .get::<_, Option<String>>(3)?
        .map(RevisionId::parse)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let accepted = row
        .get::<_, Option<String>>(4)?
        .map(RevisionId::parse)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(ObjectHead {
        project_id,
        object_id,
        object_kind: row.get(2)?,
        candidate_revision_id: candidate,
        accepted_revision_id: accepted,
        locked: row.get(5)?,
    })
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn exists<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    parameters: P,
) -> Result<bool, RepositoryError> {
    connection
        .query_row(sql, parameters, |_| Ok(()))
        .optional()
        .map(|row| row.is_some())
        .map_err(storage_error)
}

fn require_unused_commit(
    connection: &Connection,
    commit_id: &CommitId,
) -> Result<(), RepositoryError> {
    if exists(
        connection,
        "SELECT 1 FROM story_commits_v2 WHERE commit_id = ?1",
        [commit_id.as_str()],
    )? {
        return Err(RepositoryError::AlreadyExists(
            "commit",
            commit_id.to_string(),
        ));
    }
    Ok(())
}

fn require_unused_snapshot(
    connection: &Connection,
    snapshot_id: &SnapshotId,
) -> Result<(), RepositoryError> {
    if exists(
        connection,
        "SELECT 1 FROM story_snapshots_v2 WHERE snapshot_id = ?1",
        [snapshot_id.as_str()],
    )? {
        return Err(RepositoryError::AlreadyExists(
            "snapshot",
            snapshot_id.to_string(),
        ));
    }
    Ok(())
}

fn require_head(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    expected: &SnapshotId,
) -> Result<Snapshot, RepositoryError> {
    let actual = connection
        .query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
            params![project_id.as_str(), branch_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::BranchNotFound)?;
    if actual != expected.as_str() {
        return Err(RepositoryError::StaleSnapshot {
            expected: expected.clone(),
            actual: SnapshotId::parse(actual).map_err(|_| RepositoryError::CorruptState)?,
        });
    }
    load_snapshot(connection, expected.as_str())
}

fn load_snapshot(connection: &Connection, snapshot_id: &str) -> Result<Snapshot, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT project_id, branch_id, parent_snapshot_id FROM story_snapshots_v2 WHERE snapshot_id = ?1",
            [snapshot_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::CorruptState)?;
    Ok(Snapshot {
        project_id: ProjectId::parse(row.0).map_err(|_| RepositoryError::CorruptState)?,
        branch_id: BranchId::parse(row.1).map_err(|_| RepositoryError::CorruptState)?,
        snapshot_id: SnapshotId::parse(snapshot_id).map_err(|_| RepositoryError::CorruptState)?,
        parent_snapshot_id: row
            .2
            .map(SnapshotId::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?,
        members: load_members(connection, snapshot_id)?,
    })
}

fn load_members(
    connection: &Connection,
    snapshot_id: &str,
) -> Result<BTreeMap<ObjectId, RevisionId>, RepositoryError> {
    let mut statement = connection
        .prepare("SELECT object_id, revision_id FROM story_snapshot_members_v2 WHERE snapshot_id = ?1 ORDER BY object_id")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    rows.map(|row| {
        let (object_id, revision_id) = row.map_err(storage_error)?;
        Ok((
            ObjectId::parse(object_id).map_err(|_| RepositoryError::CorruptState)?,
            RevisionId::parse(revision_id).map_err(|_| RepositoryError::CorruptState)?,
        ))
    })
    .collect()
}

fn insert_revision(
    connection: &Connection,
    revision: &RevisionRecord,
) -> Result<(), RepositoryError> {
    if revision.object_kind.trim().is_empty() {
        return Err(RepositoryError::InvalidObjectKind);
    }
    if revision.object_kind == "propertyRules" {
        crate::property_rules::validate_write(&revision.object_id, &revision.content_json)?;
    }
    if revision.object_kind == "commitment" {
        crate::commitment_establishment::validate_revision(connection, revision)?;
        crate::narrative_commitments::validate_write(
            connection,
            &revision.project_id,
            &revision.object_id,
            &revision.content_json,
        )?;
    }
    if revision.object_kind == "commitmentExtraction" {
        crate::commitment_extraction::validate_write(
            connection,
            &revision.project_id,
            &revision.object_id,
            &revision.content_json,
        )?;
    }
    if let Some(parent_revision_id) = &revision.parent_revision_id {
        let parent_kind: String = connection
            .query_row(
                "SELECT object_kind FROM story_revisions_v2 WHERE revision_id = ?1",
                [parent_revision_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if parent_kind != revision.object_kind {
            return Err(RepositoryError::InvalidObjectKind);
        }
    }
    serde_json::from_str::<serde_json::Value>(&revision.content_json)
        .map_err(|error| RepositoryError::Storage(format!("invalid revision JSON: {error}")))?;
    connection
        .execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, parent_revision_id, content_digest, content_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![revision.revision_id.as_str(), revision.project_id.as_str(), revision.object_id.as_str(), revision.object_kind, revision.parent_revision_id.as_ref().map(RevisionId::as_str), revision.content_digest, revision.content_json],
        )
        .map_err(storage_error)?;
    crate::search_index::index_revision(connection, revision)?;
    Ok(())
}

fn current_project_revision(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

fn insert_snapshot(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    snapshot_id: &SnapshotId,
    parent_snapshot_id: Option<&SnapshotId>,
    members: &BTreeMap<ObjectId, RevisionId>,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_id.as_str(), project_id.as_str(), branch_id.as_str(), parent_snapshot_id.map(SnapshotId::as_str)],
        )
        .map_err(storage_error)?;
    for (object_id, revision_id) in members {
        connection
            .execute(
                "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
                params![snapshot_id.as_str(), object_id.as_str(), revision_id.as_str()],
            )
            .map_err(storage_error)?;
    }
    Ok(())
}

fn replay_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
) -> Result<Option<CommitReceipt>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT request_digest, commit_id, result_snapshot_id, status FROM story_receipts_v2 WHERE project_id = ?1 AND branch_id = ?2 AND idempotency_key = ?3",
            params![project_id.as_str(), branch_id.as_str(), key.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored_digest, commit_id, snapshot_id, status)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    Ok(Some(CommitReceipt {
        commit_id: CommitId::parse(commit_id).map_err(|_| RepositoryError::CorruptState)?,
        result_snapshot_id: SnapshotId::parse(snapshot_id)
            .map_err(|_| RepositoryError::CorruptState)?,
        status: match status.as_str() {
            "applied" => CommitStatus::Applied,
            "undone" => CommitStatus::Undone,
            _ => return Err(RepositoryError::CorruptState),
        },
    }))
}

fn insert_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
    receipt: &CommitReceipt,
) -> Result<(), RepositoryError> {
    let status = match receipt.status {
        CommitStatus::Applied => "applied",
        CommitStatus::Undone => "undone",
    };
    connection
        .execute(
            "INSERT INTO story_receipts_v2(project_id, branch_id, idempotency_key, request_digest, commit_id, result_snapshot_id, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![project_id.as_str(), branch_id.as_str(), key.as_str(), digest, receipt.commit_id.as_str(), receipt.result_snapshot_id.as_str(), status],
        )
        .map_err(storage_error)?;
    Ok(())
}
