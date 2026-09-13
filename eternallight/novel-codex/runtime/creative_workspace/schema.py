import sqlite3

from .metrics import create_metrics_schema

WORKSPACE_SCHEMA = """
CREATE TABLE IF NOT EXISTS project_workspace (
  project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL DEFAULT 0,
  index_target_revision INTEGER NOT NULL DEFAULT 0,
  indexed_revision INTEGER NOT NULL DEFAULT 0,
  plan_window INTEGER NOT NULL DEFAULT 3 CHECK(plan_window BETWEEN 1 AND 5)
);
CREATE TABLE IF NOT EXISTS artifact_revisions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('brief','outline','plan')),
  version INTEGER NOT NULL,
  base_project_revision INTEGER NOT NULL,
  parent_revision_id TEXT,
  payload_json TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('author','agent')),
  created_at TEXT NOT NULL,
  UNIQUE(project_id, artifact_kind, version)
);
CREATE INDEX IF NOT EXISTS idx_artifact_revisions_project ON artifact_revisions(project_id, artifact_kind, version DESC);
CREATE TABLE IF NOT EXISTS artifact_pointers (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('brief','outline','plan')),
  candidate_revision_id TEXT,
  accepted_revision_id TEXT,
  locked INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(project_id, artifact_kind)
);
CREATE TABLE IF NOT EXISTS chapter_workspace (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
  candidate_revision_id TEXT,
  accepted_revision_id TEXT,
  PRIMARY KEY(project_id, chapter_id)
);
CREATE TABLE IF NOT EXISTS working_baseline_selections (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  object_kind TEXT NOT NULL CHECK(object_kind IN ('brief','outline','plan','draft')),
  object_id TEXT NOT NULL,
  revision_id TEXT,
  selection_revision INTEGER NOT NULL CHECK(selection_revision > 0),
  selected_at TEXT NOT NULL,
  PRIMARY KEY(project_id, object_kind, object_id)
);
CREATE INDEX IF NOT EXISTS idx_working_baseline_project ON working_baseline_selections(project_id, object_kind, object_id);
CREATE TABLE IF NOT EXISTS continuation_intents (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('brief','outline','plan','draft')),
  target_id TEXT NOT NULL,
  working_revision_id TEXT NOT NULL,
  author_intent TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('ready','launched','failed')),
  task_id TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, target_kind, target_id)
);
CREATE TABLE IF NOT EXISTS draft_revisions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  parent_revision_id TEXT,
  source_plan_refs_json TEXT NOT NULL,
  base_project_revision INTEGER NOT NULL,
  body TEXT NOT NULL,
  scene_groups_json TEXT NOT NULL DEFAULT '[]',
  source TEXT NOT NULL CHECK(source IN ('author','agent')),
  created_at TEXT NOT NULL,
  UNIQUE(project_id, chapter_id, version)
);
CREATE INDEX IF NOT EXISTS idx_draft_revisions_chapter ON draft_revisions(project_id, chapter_id, version DESC);
CREATE TABLE IF NOT EXISTS fact_proposals (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  draft_revision_id TEXT NOT NULL REFERENCES draft_revisions(id) ON DELETE CASCADE,
  scene_group_id TEXT,
  scene_id TEXT,
  fact_kind TEXT NOT NULL CHECK(fact_kind IN ('world','reader','character','author-secret')),
  subject_id TEXT,
  predicate TEXT NOT NULL,
  value_json TEXT NOT NULL,
  epistemic_owner_id TEXT,
  epistemic_state TEXT NOT NULL CHECK(epistemic_state IN ('unknown','suspected','known','false')),
  story_time INTEGER,
  reading_chapter INTEGER NOT NULL,
  scene_order INTEGER NOT NULL DEFAULT 0,
  evidence TEXT NOT NULL,
  review_decision TEXT NOT NULL DEFAULT 'candidate' CHECK(review_decision IN ('candidate','confirmed','rejected')),
  source TEXT NOT NULL CHECK(source IN ('author','agent')),
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fact_proposals_draft ON fact_proposals(project_id, draft_revision_id);
CREATE TABLE IF NOT EXISTS review_bundles (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('brief','outline','plan','draft')),
  artifact_revision_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  selections_json TEXT NOT NULL DEFAULT '{}',
  checks_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ready' CHECK(status IN ('ready','stale','accepted','rejected')),
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_review_bundles_project ON review_bundles(project_id, created_at DESC);
CREATE TABLE IF NOT EXISTS acceptance_commits (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  operation_kind TEXT NOT NULL CHECK(operation_kind IN ('accept','materialize-chapter','materialize-standalone','associate-plan','resolve-impact','undo')),
  idempotency_key TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  base_revision INTEGER NOT NULL,
  result_revision INTEGER NOT NULL,
  review_id TEXT,
  undo_of_commit_id TEXT,
  receipt_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(project_id, operation_kind, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_acceptance_commits_project ON acceptance_commits(project_id, result_revision DESC);
CREATE TABLE IF NOT EXISTS temporal_state_events (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  project_revision INTEGER NOT NULL,
  fact_kind TEXT NOT NULL,
  subject_id TEXT,
  predicate TEXT NOT NULL,
  value_json TEXT NOT NULL,
  epistemic_owner_id TEXT,
  epistemic_state TEXT NOT NULL,
  story_time INTEGER,
  reading_chapter INTEGER NOT NULL,
  scene_order INTEGER NOT NULL DEFAULT 0,
  source_draft_revision_id TEXT,
  evidence TEXT NOT NULL,
  supersedes_id TEXT,
  reverted_by_commit_id TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_temporal_state_asof ON temporal_state_events(project_id, project_revision, reading_chapter, scene_order);
CREATE TABLE IF NOT EXISTS story_branches (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  base_project_revision INTEGER NOT NULL,
  head_version INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','ready','incomplete','accepted','archived')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_story_branches_project ON story_branches(project_id, updated_at DESC);
CREATE TABLE IF NOT EXISTS generation_sessions (
  id TEXT PRIMARY KEY,
  branch_id TEXT NOT NULL REFERENCES story_branches(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  targets_json TEXT NOT NULL,
  intent TEXT NOT NULL,
  next_index INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','ready','incomplete','cancelled')),
  max_model_turns INTEGER NOT NULL CHECK(max_model_turns BETWEEN 1 AND 5),
  turns_used INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_generation_sessions_project ON generation_sessions(project_id, updated_at DESC);
CREATE TABLE IF NOT EXISTS branch_working_commits (
  id TEXT PRIMARY KEY,
  branch_id TEXT NOT NULL REFERENCES story_branches(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  head_version INTEGER NOT NULL,
  parent_commit_id TEXT REFERENCES branch_working_commits(id),
  chapter_id TEXT NOT NULL REFERENCES chapters(id),
  draft_revision_id TEXT NOT NULL REFERENCES draft_revisions(id),
  created_at TEXT NOT NULL,
  UNIQUE(branch_id, head_version)
);
CREATE INDEX IF NOT EXISTS idx_branch_commits_project ON branch_working_commits(project_id, branch_id, head_version);
CREATE TABLE IF NOT EXISTS branch_reviews (
  id TEXT PRIMARY KEY,
  branch_id TEXT NOT NULL REFERENCES story_branches(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  head_version INTEGER NOT NULL,
  project_revision INTEGER NOT NULL,
  prefix_count INTEGER NOT NULL CHECK(prefix_count BETWEEN 1 AND 5),
  manifest_json TEXT NOT NULL,
  checks_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ready' CHECK(status IN ('ready','stale','accepted','rejected')),
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_branch_reviews_project ON branch_reviews(project_id, created_at DESC);
CREATE TABLE IF NOT EXISTS task_runs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  thread_id TEXT NOT NULL,
  task_kind TEXT NOT NULL,
  purpose TEXT NOT NULL DEFAULT 'planner' CHECK(purpose IN ('planner','writer','critic')),
  target_json TEXT NOT NULL DEFAULT '{}',
  required_output_kind TEXT CHECK(required_output_kind IS NULL OR required_output_kind IN ('brief','outline','plan','draft','critique')),
  stage TEXT NOT NULL DEFAULT 'preparing',
  delivery_status TEXT NOT NULL DEFAULT 'pending' CHECK(delivery_status IN ('pending','ready_for_review','incomplete')),
  status TEXT NOT NULL CHECK(status IN ('queued','running','waiting','completed','failed','cancelled')),
  input_revision INTEGER NOT NULL,
  model TEXT,
  output_refs_json TEXT NOT NULL DEFAULT '[]',
  result_summary TEXT NOT NULL DEFAULT '',
  branch_id TEXT REFERENCES story_branches(id),
  session_id TEXT REFERENCES generation_sessions(id),
  expected_working_head INTEGER,
  operation TEXT,
  input_manifest_json TEXT NOT NULL DEFAULT '{}',
  input_snapshot_id TEXT,
  candidate_head_revision_id TEXT,
  capability_hash TEXT,
  capability_expires_at INTEGER,
  capability_revoked_at TEXT,
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_runs_project ON task_runs(project_id, updated_at DESC);
CREATE TABLE IF NOT EXISTS task_input_snapshots (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL UNIQUE REFERENCES task_runs(id) ON DELETE CASCADE,
  purpose TEXT NOT NULL CHECK(purpose IN ('planner','writer','critic')),
  content_json TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  byte_size INTEGER NOT NULL CHECK(byte_size > 0),
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS task_checkpoints (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
  draft_revision_id TEXT REFERENCES draft_revisions(id),
  scene_id TEXT,
  completeness TEXT NOT NULL DEFAULT 'sceneComplete' CHECK(completeness IN ('partial','sceneComplete','chapterComplete')),
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS chapter_plan_bindings (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  plan_revision_id TEXT NOT NULL REFERENCES artifact_revisions(id) ON DELETE RESTRICT,
  chapter_plan_id TEXT NOT NULL,
  chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE RESTRICT,
  materialized_by_commit_id TEXT REFERENCES acceptance_commits(id),
  created_at TEXT NOT NULL,
  PRIMARY KEY(project_id, chapter_plan_id),
  UNIQUE(project_id, chapter_id)
);
CREATE TABLE IF NOT EXISTS task_outputs (
  task_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  output_kind TEXT NOT NULL CHECK(output_kind IN ('brief','outline','plan','draft','critique')),
  revision_id TEXT NOT NULL,
  completeness TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY(task_id, ordinal)
);
CREATE TABLE IF NOT EXISTS candidate_write_requests (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  owner_key TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(project_id, owner_key, idempotency_key)
);
CREATE TABLE IF NOT EXISTS planning_impacts (
  id TEXT PRIMARY KEY,
  impact_chain_id TEXT NOT NULL,
  supersedes_impact_id TEXT UNIQUE REFERENCES planning_impacts(id),
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  source_commit_id TEXT REFERENCES acceptance_commits(id),
  source_draft_revision_id TEXT REFERENCES draft_revisions(id),
  source_state_event_id TEXT REFERENCES temporal_state_events(id),
  source_planning_revision_id TEXT,
  target_artifact_kind TEXT NOT NULL CHECK(target_artifact_kind IN ('outline','plan')),
  target_revision_id TEXT,
  target_node_id TEXT,
  target_label TEXT NOT NULL,
  dependency_key TEXT,
  severity TEXT NOT NULL CHECK(severity IN ('affected','locked-conflict')),
  state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','superseded','resolved-kept','resolved-updated','resolved-false-positive','withdrawn')),
  resolution_note TEXT,
  resolved_by_commit_id TEXT REFERENCES acceptance_commits(id),
  created_by_commit_id TEXT REFERENCES acceptance_commits(id),
  created_at TEXT NOT NULL,
  resolved_at TEXT,
  UNIQUE(impact_chain_id, target_revision_id)
);
CREATE INDEX IF NOT EXISTS idx_planning_impacts_open ON planning_impacts(project_id, state, target_artifact_kind, target_node_id, created_at DESC);
CREATE TABLE IF NOT EXISTS planning_impact_links (
  impact_id TEXT PRIMARY KEY REFERENCES planning_impacts(id) ON DELETE CASCADE,
  candidate_revision_id TEXT NOT NULL,
  linked_by_author_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS author_fact_dependency_links (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  fact_proposal_id TEXT NOT NULL REFERENCES fact_proposals(id) ON DELETE CASCADE,
  target_artifact_kind TEXT NOT NULL CHECK(target_artifact_kind IN ('outline','plan')),
  target_revision_id TEXT NOT NULL,
  target_node_id TEXT NOT NULL,
  created_by_commit_id TEXT NOT NULL REFERENCES acceptance_commits(id),
  created_at TEXT NOT NULL,
  UNIQUE(project_id, fact_proposal_id, target_artifact_kind, target_revision_id, target_node_id)
);
"""


def create_workspace_schema(connection: sqlite3.Connection) -> None:
    connection.executescript(WORKSPACE_SCHEMA)
    create_metrics_schema(connection)
