import hashlib
import json
import sqlite3
import time
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

from .errors import UnauthorizedError, WorkspaceError


WRITE_TOOLS = {
    "save_brief",
    "save_outline",
    "save_chapter_plans",
    "save_draft_checkpoint",
    "submit_fact_proposals",
    "submit_planning_review_report",
    "complete_task",
}


def _now() -> str:
    return datetime.now(UTC).isoformat(timespec="milliseconds")


def _stable_hash(value: Any) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


@dataclass(frozen=True)
class OperationIdentity:
    operation_id: str
    run_id: str
    project_id: str
    root_thread_id: str
    caller_thread_id: str
    parent_thread_id: str | None
    turn_id: str | None
    tool_call_id: str
    tool_name: str
    request_hash: str
    idempotency_key: str | None


def operation_identity(
    path: str,
    method: str,
    payload: dict[str, Any] | None,
    headers: dict[str, str | None],
) -> OperationIdentity | None:
    clean_path = path.split("?")[0]
    parts = [part for part in clean_path.split("/") if part]
    expected_tool = None
    if (
        len(parts) == 5
        and parts[:2] == ["api", "projects"]
        and parts[3] == "artifacts"
        and method == "POST"
    ):
        expected_tool = {
            "brief": "save_brief",
            "outline": "save_outline",
            "plan": "save_chapter_plans",
        }.get(parts[4])
    elif (
        len(parts) == 5
        and parts[:2] == ["api", "projects"]
        and parts[3:] == ["drafts", "checkpoints"]
        and method == "POST"
    ):
        expected_tool = "save_draft_checkpoint"
    elif (
        len(parts) == 4
        and parts[:2] == ["api", "projects"]
        and parts[3] == "reviews"
        and method == "POST"
    ):
        expected_tool = "submit_fact_proposals"
    elif (
        len(parts) == 6
        and parts[:2] == ["api", "projects"]
        and parts[3] == "planning-reviews"
        and parts[5] == "reports"
        and method == "POST"
    ):
        expected_tool = "submit_planning_review_report"
    elif (
        len(parts) == 6
        and parts[:2] == ["api", "projects"]
        and parts[3] == "tasks"
        and parts[5] == "complete"
        and method == "POST"
    ):
        expected_tool = "complete_task"
    tool_name = headers.get("tool_name")
    if expected_tool is None:
        return None
    if tool_name != expected_tool or tool_name not in WRITE_TOOLS:
        raise UnauthorizedError("候选写入缺少匹配的可信工具身份")
    values = {
        key: headers.get(key)
        for key in ("root_thread_id", "caller_thread_id", "tool_call_id")
    }
    if any(not isinstance(value, str) or not value for value in values.values()):
        raise UnauthorizedError("候选写入缺少可信执行身份")
    project_id = (
        parts[2] if len(parts) >= 3 and parts[:2] == ["api", "projects"] else None
    )
    body = payload or {}
    run_id = body.get("taskId")
    if len(parts) == 6 and parts[3] == "tasks" and parts[5] == "complete":
        run_id = parts[4]
    if not isinstance(project_id, str) or not isinstance(run_id, str):
        raise UnauthorizedError("候选写入缺少作品或运行绑定")
    operation_id = _stable_hash(
        [values["root_thread_id"], values["tool_call_id"], tool_name]
    )
    return OperationIdentity(
        operation_id=operation_id,
        run_id=run_id,
        project_id=project_id,
        root_thread_id=values["root_thread_id"],
        caller_thread_id=values["caller_thread_id"],
        parent_thread_id=headers.get("parent_thread_id"),
        turn_id=headers.get("turn_id"),
        tool_call_id=values["tool_call_id"],
        tool_name=tool_name,
        request_hash=_stable_hash(
            {"method": method, "path": clean_path, "payload": body}
        ),
        idempotency_key=(
            body.get("idempotencyKey")
            if isinstance(body.get("idempotencyKey"), str)
            else None
        ),
    )


def accept_operation(
    db_path: str, identity: OperationIdentity
) -> dict[str, Any] | None:
    if identity.caller_thread_id != identity.root_thread_id:
        raise UnauthorizedError("只有任务根线程可以执行候选写入")
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    try:
        connection.execute("BEGIN IMMEDIATE")
        task = connection.execute(
            "SELECT thread_id, status, capability_hash, capability_expires_at, capability_revoked_at FROM task_runs WHERE id = ? AND project_id = ?",
            (identity.run_id, identity.project_id),
        ).fetchone()
        existing = connection.execute(
            "SELECT * FROM runtime_operations WHERE operation_id = ?",
            (identity.operation_id,),
        ).fetchone()
        if existing is not None:
            if (
                task is None
                or task["thread_id"] != identity.root_thread_id
                or existing["request_hash"] != identity.request_hash
                or existing["tool_name"] != identity.tool_name
                or existing["root_thread_id"] != identity.root_thread_id
                or existing["caller_thread_id"] != identity.caller_thread_id
            ):
                raise WorkspaceError("operation_conflict", "同一工具调用对应不同请求")
            connection.commit()
            return dict(existing)
        if task is None or task["thread_id"] != identity.root_thread_id:
            raise UnauthorizedError("写入身份与任务根线程不匹配")
        if (
            task["status"] not in {"queued", "running", "waiting"}
            or task["capability_revoked_at"] is not None
            or not isinstance(task["capability_expires_at"], int)
            or task["capability_expires_at"] <= int(time.time())
        ):
            raise UnauthorizedError("写入任务能力已撤销或到期")
        now = _now()
        connection.execute(
            "INSERT INTO runtime_operations(operation_id, run_id, project_id, root_thread_id, caller_thread_id, parent_thread_id, turn_id, tool_call_id, tool_name, request_hash, idempotency_key, state, authority_epoch, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'accepted', ?, ?, ?)",
            (
                identity.operation_id,
                identity.run_id,
                identity.project_id,
                identity.root_thread_id,
                identity.caller_thread_id,
                identity.parent_thread_id,
                identity.turn_id,
                identity.tool_call_id,
                identity.tool_name,
                identity.request_hash,
                identity.idempotency_key,
                task["capability_hash"] or "unbound",
                now,
                now,
            ),
        )
        connection.commit()
        return None
    except Exception:
        connection.rollback()
        raise
    finally:
        connection.close()


def is_committed_operation_replay(
    db_path: str,
    path: str,
    method: str,
    payload: dict[str, Any] | None,
    headers: dict[str, str | None],
) -> bool:
    """Allow an exact committed request to retrieve its receipt after revocation."""
    identity = operation_identity(path, method, payload, headers)
    if identity is None:
        return False
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    try:
        row = connection.execute(
            "SELECT state, request_hash, tool_name, root_thread_id, caller_thread_id FROM runtime_operations WHERE operation_id = ? AND run_id = ? AND project_id = ?",
            (identity.operation_id, identity.run_id, identity.project_id),
        ).fetchone()
        return bool(
            row
            and row["state"] == "committed"
            and row["request_hash"] == identity.request_hash
            and row["tool_name"] == identity.tool_name
            and row["root_thread_id"] == identity.root_thread_id
            and row["caller_thread_id"] == identity.caller_thread_id
        )
    finally:
        connection.close()


def begin_operation_commit(
    db_path: str, identity: OperationIdentity
) -> sqlite3.Connection:
    """Acquire the one transaction that owns both the domain write and receipt."""
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute("PRAGMA busy_timeout = 5000")
    try:
        connection.execute("BEGIN IMMEDIATE")
        operation = connection.execute(
            "SELECT state, request_hash, authority_epoch FROM runtime_operations WHERE operation_id = ?",
            (identity.operation_id,),
        ).fetchone()
        task = connection.execute(
            "SELECT thread_id, status, capability_hash, capability_expires_at, capability_revoked_at FROM task_runs WHERE id = ? AND project_id = ?",
            (identity.run_id, identity.project_id),
        ).fetchone()
        if (
            operation is None
            or operation["state"] != "accepted"
            or operation["request_hash"] != identity.request_hash
        ):
            raise WorkspaceError(
                "operation_not_executable", "工具调用没有可执行的 accepted 登记"
            )
        if (
            task is None
            or task["thread_id"] != identity.root_thread_id
            or task["status"] not in {"queued", "running", "waiting"}
            or task["capability_revoked_at"] is not None
            or not isinstance(task["capability_expires_at"], int)
            or task["capability_expires_at"] <= int(time.time())
            or operation["authority_epoch"] != (task["capability_hash"] or "unbound")
        ):
            raise UnauthorizedError("工具调用执行权已撤销或发生变化")
        return connection
    except Exception:
        connection.rollback()
        connection.close()
        raise


def commit_operation(
    connection: sqlite3.Connection, operation_id: str, receipt: Any
) -> None:
    receipt_ref = _stable_hash(receipt)
    receipt_json = json.dumps(
        receipt, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    updated = connection.execute(
        "UPDATE runtime_operations SET state = 'committed', receipt_ref = ?, receipt_json = ?, error_code = NULL, updated_at = ? WHERE operation_id = ? AND state = 'accepted'",
        (receipt_ref, receipt_json, _now(), operation_id),
    )
    if updated.rowcount != 1:
        raise WorkspaceError("operation_commit_conflict", "工具调用回执不能提交")


def finish_operation(
    db_path: str,
    operation_id: str,
    state: str,
    *,
    receipt: Any = None,
    error_code: str | None = None,
) -> None:
    if state not in {
        "rejected",
        "cancelled-before-execution",
        "outcome-unknown",
    }:
        raise ValueError("invalid operation state")
    receipt_ref = _stable_hash(receipt) if receipt is not None else None
    connection = sqlite3.connect(db_path)
    try:
        connection.execute("BEGIN IMMEDIATE")
        connection.execute(
            "UPDATE runtime_operations SET state = ?, receipt_ref = ?, error_code = ?, updated_at = ? WHERE operation_id = ? AND state = 'accepted'",
            (state, receipt_ref, error_code, _now(), operation_id),
        )
        connection.commit()
    except Exception:
        connection.rollback()
        raise
    finally:
        connection.close()


def operation_rows(
    db_path: str, project_id: str, run_ids: list[str]
) -> list[dict[str, Any]]:
    if (
        not run_ids
        or len(run_ids) > 100
        or any(not isinstance(value, str) for value in run_ids)
    ):
        raise WorkspaceError("invalid_operation_lookup", "运行列表无效")
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    try:
        placeholders = ",".join("?" for _ in run_ids)
        rows = connection.execute(
            f"SELECT operation_id, run_id, root_thread_id, caller_thread_id, parent_thread_id, turn_id, tool_call_id, tool_name, request_hash, state, receipt_ref, error_code, authority_epoch, created_at, updated_at FROM runtime_operations WHERE project_id = ? AND run_id IN ({placeholders}) ORDER BY created_at, operation_id",
            (project_id, *run_ids),
        ).fetchall()
        return [dict(row) for row in rows]
    finally:
        connection.close()


def recovery_snapshot(
    db_path: str,
    project_id: str,
    run_ids: list[str],
    *,
    fence: str | None = None,
    cursor: int = 0,
    limit: int = 500,
) -> dict[str, Any]:
    """Return task state and operation receipts from one fenced SQLite snapshot.

    The first page revokes the old write authority and converts operations which
    provably never entered their commit transaction. Subsequent pages are bound
    to the same task set and operation rowid watermark.
    """
    if (
        not run_ids
        or len(run_ids) > 100
        or any(
            not isinstance(value, str) or not value or len(value) > 160
            for value in run_ids
        )
        or isinstance(cursor, bool)
        or not isinstance(cursor, int)
        or cursor < 0
        or isinstance(limit, bool)
        or not isinstance(limit, int)
        or not 1 <= limit <= 500
    ):
        raise WorkspaceError("invalid_recovery_snapshot", "恢复快照参数无效")
    ordered_ids = list(dict.fromkeys(run_ids))
    task_ids_hash = _stable_hash(ordered_ids)
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    try:
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA busy_timeout = 5000")
        connection.execute("BEGIN IMMEDIATE")
        placeholders = ",".join("?" for _ in ordered_ids)
        if fence is None:
            revoked_at = _now()
            connection.execute(
                f"UPDATE task_runs SET capability_revoked_at = COALESCE(capability_revoked_at, ?), updated_at = ? WHERE project_id = ? AND id IN ({placeholders})",
                (revoked_at, revoked_at, project_id, *ordered_ids),
            )
            # BEGIN IMMEDIATE excludes a concurrent commit owner. Once authority
            # is revoked, an accepted row can no longer enter begin_operation_commit.
            connection.execute(
                f"UPDATE runtime_operations SET state = 'cancelled-before-execution', error_code = 'recovery_fence_revoked', updated_at = ? WHERE project_id = ? AND run_id IN ({placeholders}) AND state = 'accepted'",
                (revoked_at, project_id, *ordered_ids),
            )
            task_rows = connection.execute(
                f"SELECT id, thread_id, operation, status, delivery_status, updated_at FROM task_runs WHERE project_id = ? AND id IN ({placeholders}) ORDER BY id",
                (project_id, *ordered_ids),
            ).fetchall()
            tasks = []
            for row in task_rows:
                output_kinds = [
                    output["output_kind"]
                    for output in connection.execute(
                        "SELECT DISTINCT output_kind FROM task_outputs WHERE task_id = ? ORDER BY output_kind",
                        (row["id"],),
                    ).fetchall()
                ]
                tasks.append(
                    {
                        "id": row["id"],
                        "threadId": row["thread_id"],
                        "operation": row["operation"],
                        "status": row["status"],
                        "deliveryStatus": row["delivery_status"],
                        "outputKinds": output_kinds,
                        "updatedAt": row["updated_at"],
                    }
                )
            watermark = connection.execute(
                f"SELECT COALESCE(MAX(rowid), 0) AS watermark FROM runtime_operations WHERE project_id = ? AND run_id IN ({placeholders})",
                (project_id, *ordered_ids),
            ).fetchone()["watermark"]
            fence = str(uuid.uuid4())
            connection.execute(
                "INSERT INTO runtime_recovery_snapshots(fence, project_id, task_ids_hash, operation_watermark, tasks_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    fence,
                    project_id,
                    task_ids_hash,
                    watermark,
                    json.dumps(tasks, ensure_ascii=False, separators=(",", ":")),
                    revoked_at,
                ),
            )
        else:
            snapshot = connection.execute(
                "SELECT task_ids_hash, operation_watermark, tasks_json FROM runtime_recovery_snapshots WHERE fence = ? AND project_id = ?",
                (fence, project_id),
            ).fetchone()
            if snapshot is None or snapshot["task_ids_hash"] != task_ids_hash:
                raise WorkspaceError(
                    "invalid_recovery_fence", "恢复快照 fence 无效或与任务集合不匹配"
                )
            watermark = snapshot["operation_watermark"]
            tasks = json.loads(snapshot["tasks_json"])
        rows = connection.execute(
            f"SELECT rowid AS ledger_cursor, operation_id, run_id, root_thread_id, caller_thread_id, parent_thread_id, turn_id, tool_call_id, tool_name, request_hash, state, receipt_ref, error_code, authority_epoch, created_at, updated_at FROM runtime_operations WHERE project_id = ? AND run_id IN ({placeholders}) AND rowid > ? AND rowid <= ? ORDER BY rowid LIMIT ?",
            (project_id, *ordered_ids, cursor, watermark, limit + 1),
        ).fetchall()
        page = rows[:limit]
        complete = len(rows) <= limit
        next_cursor = None if complete else page[-1]["ledger_cursor"]
        connection.commit()
        return {
            "fence": fence,
            "watermark": watermark,
            "complete": complete,
            "nextCursor": next_cursor,
            "tasks": tasks,
            "operations": [dict(row) for row in page],
        }
    except Exception:
        connection.rollback()
        raise
    finally:
        connection.close()
