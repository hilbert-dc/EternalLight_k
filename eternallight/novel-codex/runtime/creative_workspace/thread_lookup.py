"""Bounded exact task lookup, independent of workspace presentation pagination."""

import json
from pathlib import Path

from .errors import WorkspaceError

try:
    from ..storage import connect, initialize
except ImportError:
    from storage import connect, initialize


def lookup_thread_tasks(
    db_path: str | Path, project_id: str, thread_ids: list[str]
) -> dict:
    if (
        not isinstance(thread_ids, list)
        or not 1 <= len(thread_ids) <= 100
        or any(
            not isinstance(value, str) or not value.strip() or len(value) > 160
            for value in thread_ids
        )
    ):
        raise WorkspaceError(
            "invalid_thread_ids", "threadIds 必须包含 1 至 100 个有效线程 ID"
        )
    initialize(db_path)
    tasks = []
    with connect(db_path) as connection:
        for thread_id in dict.fromkeys(thread_ids):
            row = connection.execute(
                "SELECT id, thread_id, operation, purpose, status, target_json, updated_at "
                "FROM task_runs WHERE project_id = ? AND thread_id = ? "
                "ORDER BY created_at DESC, rowid DESC LIMIT 1",
                (project_id, thread_id),
            ).fetchone()
            if row is not None:
                tasks.append(
                    {
                        "id": row["id"],
                        "threadId": row["thread_id"],
                        "operation": row["operation"],
                        "purpose": row["purpose"],
                        "status": row["status"],
                        "updatedAt": row["updated_at"],
                        "target": json.loads(row["target_json"]),
                    }
                )
    return {"tasks": tasks}
