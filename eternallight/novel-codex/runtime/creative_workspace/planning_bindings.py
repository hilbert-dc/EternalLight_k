import hashlib
import json
from pathlib import Path
from typing import Any
from uuid import uuid4

from .errors import ConflictError, NotFoundError, WorkspaceError

try:
    from ..storage import connect, initialize, now
except ImportError:
    from storage import connect, initialize, now


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _hash(value: Any) -> str:
    return hashlib.sha256(_json(value).encode()).hexdigest()


def _id(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex}"


def _existing(connection, project_id: str, operation: str, key: str, digest: str):
    row = connection.execute(
        "SELECT request_hash, receipt_json FROM acceptance_commits WHERE project_id = ? AND operation_kind = ? AND idempotency_key = ?",
        (project_id, operation, key),
    ).fetchone()
    if row is None:
        return None
    if row["request_hash"] != digest:
        raise ConflictError("idempotency_conflict", "幂等键已用于不同操作")
    return json.loads(row["receipt_json"])


def _workspace(connection, project_id: str, expected_revision: int):
    row = connection.execute(
        "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
    ).fetchone()
    if row is None:
        raise NotFoundError("作品工作区不存在")
    if row["revision"] != expected_revision:
        raise ConflictError(
            "stale_project_revision",
            "作品版本已变化",
            {"currentRevision": row["revision"]},
        )
    return row


def materialize_chapter_from_plan(
    db_path: str | Path,
    project_id: str,
    chapter_plan_id: str,
    expected_revision: int,
    idempotency_key: str,
) -> dict[str, Any]:
    request = {"chapterPlanId": chapter_plan_id, "expectedRevision": expected_revision}
    digest = _hash(request)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        replay = _existing(
            connection, project_id, "materialize-chapter", idempotency_key, digest
        )
        if replay is not None:
            return replay
        _workspace(connection, project_id, expected_revision)
        existing = connection.execute(
            "SELECT chapter_id, materialized_by_commit_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_plan_id = ?",
            (project_id, chapter_plan_id),
        ).fetchone()
        if existing:
            raise ConflictError(
                "chapter_plan_already_materialized",
                "该章纲已物化",
                {"chapterId": existing["chapter_id"]},
            )
        pointer = connection.execute(
            "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'plan'",
            (project_id,),
        ).fetchone()
        if not pointer or not pointer["accepted_revision_id"]:
            raise ConflictError("planning_baseline_required", "请先采用近期章纲")
        plan_row = connection.execute(
            "SELECT payload_json FROM artifact_revisions WHERE id = ?",
            (pointer["accepted_revision_id"],),
        ).fetchone()
        plan = json.loads(plan_row["payload_json"])
        outline_pointer = connection.execute(
            "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'outline'",
            (project_id,),
        ).fetchone()
        if (
            not outline_pointer
            or plan.get("sourceOutlineRevisionId")
            != outline_pointer["accepted_revision_id"]
        ):
            raise ConflictError(
                "stale_planning_baseline",
                "正式章纲已不再基于当前正式大纲",
                recovery="先基于新版大纲修订并采用近章规划",
            )
        entry = next(
            (
                item
                for item in plan.get("chapters", [])
                if item.get("chapterPlanId") == chapter_plan_id
            ),
            None,
        )
        if entry is None:
            raise NotFoundError("章纲条目不存在", {"chapterPlanId": chapter_plan_id})
        number = entry["chapterNumber"]
        if connection.execute(
            "SELECT 1 FROM chapters WHERE project_id = ? AND number = ?",
            (project_id, number),
        ).fetchone():
            raise WorkspaceError("duplicate_chapter_number", "该章节序号已存在", 409)
        commit_id = _id("commit")
        chapter_id = _id("chapter")
        result_revision = expected_revision + 1
        connection.execute(
            "INSERT INTO chapters(id, project_id, number, title, status, summary, updated_at) VALUES (?, ?, ?, ?, 'draft', '', ?)",
            (
                chapter_id,
                project_id,
                number,
                entry.get("publicTitle") or f"第 {number} 章",
                now(),
            ),
        )
        connection.execute(
            "INSERT INTO chapter_workspace(project_id, chapter_id) VALUES (?, ?)",
            (project_id, chapter_id),
        )
        receipt = {
            "commitId": commit_id,
            "operation": "materialize-chapter",
            "projectId": project_id,
            "chapterPlanId": chapter_plan_id,
            "chapterId": chapter_id,
            "baseRevision": expected_revision,
            "resultRevision": result_revision,
        }
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, receipt_json, created_at) VALUES (?, ?, 'materialize-chapter', ?, ?, ?, ?, ?, ?)",
            (
                commit_id,
                project_id,
                idempotency_key,
                digest,
                expected_revision,
                result_revision,
                _json(receipt),
                now(),
            ),
        )
        connection.execute(
            "INSERT INTO chapter_plan_bindings(project_id, plan_revision_id, chapter_plan_id, chapter_id, materialized_by_commit_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            (
                project_id,
                pointer["accepted_revision_id"],
                chapter_plan_id,
                chapter_id,
                commit_id,
                now(),
            ),
        )
        connection.execute(
            "UPDATE task_runs SET status = 'cancelled', delivery_status = 'incomplete', error = '章节结构已变化', capability_revoked_at = ?, updated_at = ? WHERE project_id = ? AND status IN ('queued','running','waiting')",
            (now(), now(), project_id),
        )
        connection.execute(
            "UPDATE review_bundles SET status = 'stale' WHERE project_id = ? AND status = 'ready'",
            (project_id,),
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, project_id),
        )
    return receipt


def materialize_standalone_chapter(
    db_path: str | Path,
    project_id: str,
    title: str,
    expected_revision: int,
    idempotency_key: str,
) -> dict[str, Any]:
    request = {"title": title, "expectedRevision": expected_revision}
    digest = _hash(request)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        replay = _existing(
            connection, project_id, "materialize-standalone", idempotency_key, digest
        )
        if replay is not None:
            return replay
        _workspace(connection, project_id, expected_revision)
        number = connection.execute(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM chapters WHERE project_id = ?",
            (project_id,),
        ).fetchone()[0]
        chapter_id = _id("chapter")
        commit_id = _id("commit")
        result_revision = expected_revision + 1
        connection.execute(
            "INSERT INTO chapters(id, project_id, number, title, status, summary, updated_at) VALUES (?, ?, ?, ?, 'draft', '', ?)",
            (
                chapter_id,
                project_id,
                number,
                (title.strip() or f"第 {number} 章")[:160],
                now(),
            ),
        )
        connection.execute(
            "INSERT INTO chapter_workspace(project_id, chapter_id) VALUES (?, ?)",
            (project_id, chapter_id),
        )
        receipt = {
            "commitId": commit_id,
            "operation": "materialize-standalone",
            "projectId": project_id,
            "chapterId": chapter_id,
            "chapterNumber": number,
            "baseRevision": expected_revision,
            "resultRevision": result_revision,
        }
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, receipt_json, created_at) VALUES (?, ?, 'materialize-standalone', ?, ?, ?, ?, ?, ?)",
            (
                commit_id,
                project_id,
                idempotency_key,
                digest,
                expected_revision,
                result_revision,
                _json(receipt),
                now(),
            ),
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, project_id),
        )
    return receipt


def associate_standalone_chapter_to_plan(
    db_path: str | Path,
    project_id: str,
    chapter_id: str,
    chapter_plan_id: str,
    expected_revision: int,
    idempotency_key: str,
) -> dict[str, Any]:
    request = {
        "chapterId": chapter_id,
        "chapterPlanId": chapter_plan_id,
        "expectedRevision": expected_revision,
    }
    digest = _hash(request)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        replay = _existing(
            connection, project_id, "associate-plan", idempotency_key, digest
        )
        if replay is not None:
            return replay
        _workspace(connection, project_id, expected_revision)
        chapter = connection.execute(
            "SELECT id, number FROM chapters WHERE id = ? AND project_id = ?",
            (chapter_id, project_id),
        ).fetchone()
        if chapter is None:
            raise NotFoundError("独立章节不存在")
        existing_chapter = connection.execute(
            "SELECT chapter_plan_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ?",
            (project_id, chapter_id),
        ).fetchone()
        if existing_chapter:
            raise ConflictError(
                "chapter_already_bound",
                "该章节已经关联章纲",
                {"chapterPlanId": existing_chapter["chapter_plan_id"]},
            )
        existing_plan = connection.execute(
            "SELECT chapter_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_plan_id = ?",
            (project_id, chapter_plan_id),
        ).fetchone()
        if existing_plan:
            raise ConflictError(
                "chapter_plan_already_materialized",
                "该章纲已经关联其他章节",
                {"chapterId": existing_plan["chapter_id"]},
            )
        pointer = connection.execute(
            "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'plan'",
            (project_id,),
        ).fetchone()
        if not pointer or not pointer["accepted_revision_id"]:
            raise ConflictError("planning_baseline_required", "请先采用近期章纲")
        plan_row = connection.execute(
            "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ?",
            (pointer["accepted_revision_id"], project_id),
        ).fetchone()
        plan = json.loads(plan_row["payload_json"])
        outline_pointer = connection.execute(
            "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'outline'",
            (project_id,),
        ).fetchone()
        if (
            not outline_pointer
            or plan.get("sourceOutlineRevisionId")
            != outline_pointer["accepted_revision_id"]
        ):
            raise ConflictError(
                "stale_planning_baseline",
                "正式章纲已不再基于当前正式大纲",
                recovery="先基于新版大纲修订并采用近章规划",
            )
        entry = next(
            (
                item
                for item in plan.get("chapters", [])
                if item.get("chapterPlanId") == chapter_plan_id
            ),
            None,
        )
        if entry is None:
            raise NotFoundError("章纲条目不存在", {"chapterPlanId": chapter_plan_id})
        if entry.get("chapterNumber") != chapter["number"]:
            raise ConflictError(
                "chapter_plan_number_mismatch",
                "章节序号与章纲序号不一致",
                {
                    "chapterNumber": chapter["number"],
                    "planChapterNumber": entry.get("chapterNumber"),
                },
            )
        commit_id = _id("commit")
        result_revision = expected_revision + 1
        receipt = {
            "commitId": commit_id,
            "operation": "associate-plan",
            "projectId": project_id,
            "chapterPlanId": chapter_plan_id,
            "chapterId": chapter_id,
            "baseRevision": expected_revision,
            "resultRevision": result_revision,
        }
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, receipt_json, created_at) VALUES (?, ?, 'associate-plan', ?, ?, ?, ?, ?, ?)",
            (
                commit_id,
                project_id,
                idempotency_key,
                digest,
                expected_revision,
                result_revision,
                _json(receipt),
                now(),
            ),
        )
        connection.execute(
            "INSERT INTO chapter_plan_bindings(project_id, plan_revision_id, chapter_plan_id, chapter_id, materialized_by_commit_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            (
                project_id,
                pointer["accepted_revision_id"],
                chapter_plan_id,
                chapter_id,
                commit_id,
                now(),
            ),
        )
        connection.execute(
            "UPDATE task_runs SET status = 'cancelled', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, error = '章节规划关联已变化', capability_revoked_at = ?, updated_at = ? WHERE project_id = ? AND status IN ('queued','running','waiting')",
            (now(), now(), project_id),
        )
        connection.execute(
            "UPDATE review_bundles SET status = 'stale' WHERE project_id = ? AND status = 'ready'",
            (project_id,),
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, project_id),
        )
        connection.execute(
            "UPDATE projects SET updated_at = ? WHERE id = ?", (now(), project_id)
        )
    return receipt
