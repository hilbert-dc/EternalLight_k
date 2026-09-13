"""Anonymous aggregates and bounded random receipts for manual action retries."""

import sqlite3
import re
from pathlib import Path

from .errors import WorkspaceError

STAGES = ("brief", "outline", "plan", "draft")
METRICS = tuple(
    f"{stage}.{event}"
    for stage in STAGES
    for event in ("saved", "changedSave", "staleResave", "adopted", "abandoned")
) + (
    "feedback.autoMatch",
    "feedback.manualLink",
    "feedback.kept",
    "feedback.updated",
    "feedback.falsePositive",
    "localOnly.created",
    "localOnly.associated",
    "localOnly.adopted",
    "generation.completed",
    "generation.failed",
    "generation.cancelled",
)


def record(connection, metric: str, seconds: int = 0) -> None:
    if metric not in METRICS:
        raise ValueError("Unknown local metric")
    connection.execute(
        "UPDATE local_product_metrics SET count = count + 1, total_seconds = total_seconds + ? WHERE metric = ?",
        (max(0, seconds), metric),
    )


def create_metrics_schema(connection: sqlite3.Connection) -> None:
    # Fixed keys bound storage size, including when authors work for years.
    keys = ",".join(f"'{key}'" for key in METRICS)
    connection.executescript(f"""
        CREATE TABLE IF NOT EXISTS local_product_metrics (
          metric TEXT PRIMARY KEY CHECK(metric IN ({keys})),
          count INTEGER NOT NULL DEFAULT 0 CHECK(count >= 0),
          total_seconds INTEGER NOT NULL DEFAULT 0 CHECK(total_seconds >= 0)
        );
        CREATE TABLE IF NOT EXISTS local_metric_action_receipts (
          idempotency_key TEXT PRIMARY KEY,
          stage TEXT NOT NULL CHECK(stage IN ('brief','outline','plan','draft'))
        );
    """)
    connection.executemany(
        "INSERT OR IGNORE INTO local_product_metrics(metric) VALUES (?)",
        [(key,) for key in METRICS],
    )
    for table, pointer, stage, content in (
        (
            "artifact_revisions",
            "artifact_pointers",
            "NEW.artifact_kind",
            "payload_json",
        ),
        ("draft_revisions", "chapter_workspace", "'draft'", "body"),
    ):
        connection.executescript(f"""
          CREATE TRIGGER IF NOT EXISTS metrics_{pointer}_saved AFTER UPDATE OF candidate_revision_id ON {pointer}
          WHEN NEW.candidate_revision_id IS NOT NULL AND NEW.candidate_revision_id IS NOT OLD.candidate_revision_id
          BEGIN
            UPDATE local_product_metrics SET count = count + 1
              WHERE metric = {stage} || '.saved';
            UPDATE local_product_metrics SET count = count + 1
              WHERE metric = {stage} || '.changedSave'
              AND EXISTS (SELECT 1 FROM {table} revision JOIN {table} parent ON parent.id = revision.parent_revision_id
                WHERE revision.id = NEW.candidate_revision_id AND revision.source = 'author' AND parent.{content} != revision.{content});
            UPDATE local_product_metrics SET count = count + 1
              WHERE metric = {stage} || '.staleResave'
              AND EXISTS (SELECT 1 FROM {table} revision JOIN {table} parent ON parent.id = revision.parent_revision_id
                WHERE revision.id = NEW.candidate_revision_id AND revision.source = 'author'
                AND parent.base_project_revision < revision.base_project_revision);
          END;
        """)
    connection.executescript("""
      CREATE TRIGGER IF NOT EXISTS metrics_acceptance AFTER INSERT ON acceptance_commits
      BEGIN
        UPDATE local_product_metrics SET count = count + 1, total_seconds = total_seconds +
          COALESCE((SELECT MAX(0, CAST(ROUND((julianday(NEW.created_at) - julianday(revision.created_at)) * 86400) AS INTEGER))
            FROM review_bundles review JOIN (
              SELECT id, created_at FROM artifact_revisions UNION ALL SELECT id, created_at FROM draft_revisions
            ) revision ON revision.id = review.artifact_revision_id WHERE review.id = NEW.review_id), 0)
          WHERE NEW.operation_kind = 'accept' AND metric =
            (SELECT artifact_kind || '.adopted' FROM review_bundles WHERE id = NEW.review_id);
        UPDATE local_product_metrics SET count = count + 1
          WHERE metric = 'localOnly.created' AND NEW.operation_kind = 'materialize-standalone';
        UPDATE local_product_metrics SET count = count + 1
          WHERE metric = 'localOnly.associated' AND NEW.operation_kind = 'associate-plan';
        UPDATE local_product_metrics SET count = count + 1
          WHERE metric = 'localOnly.adopted' AND NEW.operation_kind = 'accept' AND EXISTS (
            SELECT 1 FROM review_bundles review JOIN draft_revisions draft ON draft.id = review.artifact_revision_id
            WHERE review.id = NEW.review_id AND json_extract(draft.source_plan_refs_json, '$.localOnly') = 1);
      END;
      CREATE TRIGGER IF NOT EXISTS metrics_feedback_resolved AFTER UPDATE OF state ON planning_impacts
      WHEN OLD.state = 'open' AND NEW.state IN ('resolved-kept', 'resolved-updated', 'resolved-false-positive')
      BEGIN
        UPDATE local_product_metrics SET count = count + 1, total_seconds = total_seconds +
          MAX(0, COALESCE(CAST(ROUND((julianday(NEW.resolved_at) - julianday(OLD.created_at)) * 86400) AS INTEGER), 0))
          WHERE metric = CASE NEW.state WHEN 'resolved-kept' THEN 'feedback.kept'
            WHEN 'resolved-updated' THEN 'feedback.updated' ELSE 'feedback.falsePositive' END;
      END;
      CREATE TRIGGER IF NOT EXISTS metrics_feedback_initial_resolution AFTER INSERT ON planning_impacts
      WHEN NEW.state = 'resolved-false-positive'
      BEGIN
        UPDATE local_product_metrics SET count = count + 1 WHERE metric = 'feedback.falsePositive';
      END;
      CREATE TRIGGER IF NOT EXISTS metrics_generation_finished AFTER UPDATE OF status ON task_runs
      WHEN OLD.status IN ('queued', 'running', 'waiting') AND NEW.status IN ('completed', 'failed', 'cancelled')
      BEGIN
        UPDATE local_product_metrics SET count = count + 1, total_seconds = total_seconds +
          MAX(0, COALESCE(CAST(ROUND((julianday(NEW.updated_at) - julianday(OLD.created_at)) * 86400) AS INTEGER), 0))
          WHERE metric = 'generation.' || NEW.status;
      END;
    """)


def local_metrics(db_path: str | Path) -> dict:
    try:
        from ..storage import connect, initialize
    except ImportError:
        from storage import connect, initialize
    initialize(db_path)
    with connect(db_path) as connection:
        rows = connection.execute(
            "SELECT metric, count, total_seconds FROM local_product_metrics ORDER BY metric"
        ).fetchall()
    return {
        "scope": "local-device",
        "metrics": [
            {
                "metric": row["metric"],
                "count": row["count"],
                "totalSeconds": row["total_seconds"],
            }
            for row in rows
        ],
    }


def record_abandonment(db_path: str | Path, stage: str, idempotency_key: str) -> dict:
    if stage not in STAGES:
        raise WorkspaceError(
            "invalid_metric_stage", "请选择构想、层级大纲、近章规划或正文阶段"
        )
    if not isinstance(idempotency_key, str) or not re.fullmatch(
        r"[a-f0-9-]{36}", idempotency_key
    ):
        raise WorkspaceError("invalid_metric_action", "统计操作凭证无效")
    try:
        from ..storage import connect, initialize
    except ImportError:
        from storage import connect, initialize
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        previous = connection.execute(
            "SELECT stage FROM local_metric_action_receipts WHERE idempotency_key = ?",
            (idempotency_key,),
        ).fetchone()
        if previous and previous["stage"] != stage:
            raise WorkspaceError(
                "metric_action_conflict", "统计操作凭证已用于其他阶段", 409
            )
        if not previous:
            record(connection, f"{stage}.abandoned")
            connection.execute(
                "INSERT INTO local_metric_action_receipts(idempotency_key, stage) VALUES (?, ?)",
                (idempotency_key, stage),
            )
            connection.execute(
                "DELETE FROM local_metric_action_receipts WHERE rowid NOT IN (SELECT rowid FROM local_metric_action_receipts ORDER BY rowid DESC LIMIT 1000)"
            )
    return local_metrics(db_path)
