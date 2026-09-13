from pathlib import Path
from typing import Any

from .errors import ConflictError, NotFoundError, WorkspaceError

try:
    from ..storage import connect, initialize, now
except ImportError:
    from storage import connect, initialize, now


PLANNING_KINDS = {"brief", "outline", "plan"}
WORKING_KINDS = {*PLANNING_KINDS, "draft"}


def _selection_object_id(kind: str, object_id: str | None) -> str:
    if kind in PLANNING_KINDS:
        if object_id not in {None, kind}:
            raise WorkspaceError(
                "invalid_working_baseline", "规划工作版本的对象 ID 必须等于产物类型"
            )
        return kind
    if kind == "draft" and isinstance(object_id, str) and object_id.strip():
        return object_id.strip()
    raise WorkspaceError("invalid_working_baseline", "正文工作版本必须指定章节 ID")


def _validate_revision(
    connection,
    project_id: str,
    kind: str,
    object_id: str,
    revision_id: str,
) -> dict[str, Any]:
    if kind in PLANNING_KINDS:
        row = connection.execute(
            "SELECT id, version, source FROM artifact_revisions "
            "WHERE id = ? AND project_id = ? AND artifact_kind = ?",
            (revision_id, project_id, kind),
        ).fetchone()
    else:
        row = connection.execute(
            "SELECT id, version, source FROM draft_revisions "
            "WHERE id = ? AND project_id = ? AND chapter_id = ?",
            (revision_id, project_id, object_id),
        ).fetchone()
    if row is None:
        raise WorkspaceError(
            "working_revision_invalid",
            "所选修订不属于当前作品和对象",
            422,
            {"objectKind": kind, "objectId": object_id, "revisionId": revision_id},
        )
    return dict(row)


def selected_revision_id(
    connection, project_id: str, kind: str, object_id: str | None = None
) -> str | None:
    normalized_object_id = _selection_object_id(kind, object_id)
    row = connection.execute(
        "SELECT revision_id FROM working_baseline_selections "
        "WHERE project_id = ? AND object_kind = ? AND object_id = ?",
        (project_id, kind, normalized_object_id),
    ).fetchone()
    return row["revision_id"] if row and row["revision_id"] else None


def resolve_planning_revision(
    connection, project_id: str, kind: str
) -> tuple[str | None, str]:
    selected = selected_revision_id(connection, project_id, kind, kind)
    if selected:
        return selected, "workingSelected"
    pointer = connection.execute(
        "SELECT accepted_revision_id FROM artifact_pointers "
        "WHERE project_id = ? AND artifact_kind = ?",
        (project_id, kind),
    ).fetchone()
    accepted = pointer["accepted_revision_id"] if pointer else None
    return accepted, "accepted" if accepted else "missing"


def resolve_draft_revision(
    connection, project_id: str, chapter_id: str
) -> tuple[str | None, str]:
    selected = selected_revision_id(connection, project_id, "draft", chapter_id)
    if selected:
        return selected, "workingSelected"
    pointer = connection.execute(
        "SELECT accepted_revision_id FROM chapter_workspace "
        "WHERE project_id = ? AND chapter_id = ?",
        (project_id, chapter_id),
    ).fetchone()
    accepted = pointer["accepted_revision_id"] if pointer else None
    return accepted, "accepted" if accepted else "missing"


def list_working_baselines(connection, project_id: str) -> list[dict[str, Any]]:
    return [
        {
            "objectKind": row["object_kind"],
            "objectId": row["object_id"],
            "revisionId": row["revision_id"],
            "selectionRevision": row["selection_revision"],
            "selectedAt": row["selected_at"],
        }
        for row in connection.execute(
            "SELECT object_kind, object_id, revision_id, selection_revision, selected_at "
            "FROM working_baseline_selections WHERE project_id = ? "
            "ORDER BY object_kind, object_id",
            (project_id,),
        ).fetchall()
    ]


def list_continuation_intents(connection, project_id: str) -> list[dict[str, Any]]:
    return [
        {
            "targetKind": row["target_kind"],
            "targetId": row["target_id"],
            "workingRevisionId": row["working_revision_id"],
            "authorIntent": row["author_intent"],
            "status": row["status"],
            "taskId": row["task_id"],
            "updatedAt": row["updated_at"],
        }
        for row in connection.execute(
            "SELECT target_kind, target_id, working_revision_id, author_intent, status, task_id, updated_at "
            "FROM continuation_intents WHERE project_id = ? ORDER BY updated_at DESC",
            (project_id,),
        ).fetchall()
    ]


def save_continuation_intent(
    db_path: str | Path,
    project_id: str,
    kind: str,
    target_id: str | None,
    working_revision_id: str,
    author_intent: str,
) -> dict[str, Any]:
    if kind not in WORKING_KINDS:
        raise WorkspaceError("invalid_continuation", "续创目标类型无效")
    normalized_target_id = _selection_object_id(kind, target_id)
    if (
        not isinstance(author_intent, str)
        or not author_intent.strip()
        or len(author_intent.encode("utf-8")) > 64_000
    ):
        raise WorkspaceError("invalid_continuation", "续创意图无效或超过 64 KiB")
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        selected = selected_revision_id(
            connection, project_id, kind, normalized_target_id
        )
        if selected != working_revision_id:
            raise ConflictError(
                "working_source_changed",
                "保存续创意图前工作版本已变化",
                {"selectedRevisionId": selected},
                "重新载入并确认要继续使用的版本",
            )
        updated_at = now()
        connection.execute(
            "INSERT INTO continuation_intents(project_id, target_kind, target_id, working_revision_id, author_intent, status, task_id, updated_at) "
            "VALUES (?, ?, ?, ?, ?, 'ready', NULL, ?) "
            "ON CONFLICT(project_id, target_kind, target_id) DO UPDATE SET "
            "working_revision_id = excluded.working_revision_id, author_intent = excluded.author_intent, "
            "status = 'ready', task_id = NULL, updated_at = excluded.updated_at",
            (
                project_id,
                kind,
                normalized_target_id,
                working_revision_id,
                author_intent.strip(),
                updated_at,
            ),
        )
    return {
        "targetKind": kind,
        "targetId": normalized_target_id,
        "workingRevisionId": working_revision_id,
        "authorIntent": author_intent.strip(),
        "status": "ready",
        "taskId": None,
        "updatedAt": updated_at,
    }


def set_working_baseline(
    db_path: str | Path,
    project_id: str,
    kind: str,
    object_id: str | None,
    revision_id: str,
    expected_selection_revision: int,
) -> dict[str, Any]:
    if kind not in WORKING_KINDS:
        raise WorkspaceError("invalid_working_baseline", "工作版本类型无效")
    if not isinstance(revision_id, str) or not revision_id.strip():
        raise WorkspaceError("invalid_working_baseline", "必须指定要选用的修订")
    if (
        isinstance(expected_selection_revision, bool)
        or not isinstance(expected_selection_revision, int)
        or expected_selection_revision < 0
    ):
        raise WorkspaceError("invalid_working_baseline", "选择版本号无效")
    normalized_object_id = _selection_object_id(kind, object_id)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        if connection.execute(
            "SELECT 1 FROM projects WHERE id = ?", (project_id,)
        ).fetchone() is None:
            raise NotFoundError("作品不存在")
        revision = _validate_revision(
            connection, project_id, kind, normalized_object_id, revision_id.strip()
        )
        current = connection.execute(
            "SELECT selection_revision FROM working_baseline_selections "
            "WHERE project_id = ? AND object_kind = ? AND object_id = ?",
            (project_id, kind, normalized_object_id),
        ).fetchone()
        current_revision = current["selection_revision"] if current else 0
        if current_revision != expected_selection_revision:
            raise ConflictError(
                "stale_working_selection",
                "工作版本选择已变化",
                {"currentSelectionRevision": current_revision},
                "重新载入来源选择后再操作",
            )
        next_revision = current_revision + 1
        selected_at = now()
        connection.execute(
            "INSERT INTO working_baseline_selections(project_id, object_kind, object_id, revision_id, selection_revision, selected_at) "
            "VALUES (?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(project_id, object_kind, object_id) DO UPDATE SET "
            "revision_id = excluded.revision_id, selection_revision = excluded.selection_revision, selected_at = excluded.selected_at",
            (
                project_id,
                kind,
                normalized_object_id,
                revision_id.strip(),
                next_revision,
                selected_at,
            ),
        )
    return {
        "objectKind": kind,
        "objectId": normalized_object_id,
        "revisionId": revision["id"],
        "revisionVersion": revision["version"],
        "revisionSource": revision["source"],
        "selectionRevision": next_revision,
        "selectedAt": selected_at,
    }


def clear_working_baseline(
    db_path: str | Path,
    project_id: str,
    kind: str,
    object_id: str | None,
    expected_selection_revision: int,
) -> dict[str, Any]:
    if kind not in WORKING_KINDS:
        raise WorkspaceError("invalid_working_baseline", "工作版本类型无效")
    normalized_object_id = _selection_object_id(kind, object_id)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        current = connection.execute(
            "SELECT selection_revision FROM working_baseline_selections "
            "WHERE project_id = ? AND object_kind = ? AND object_id = ?",
            (project_id, kind, normalized_object_id),
        ).fetchone()
        if current is None:
            raise NotFoundError("当前对象没有已选用的工作版本")
        if current["selection_revision"] != expected_selection_revision:
            raise ConflictError(
                "stale_working_selection",
                "工作版本选择已变化",
                {"currentSelectionRevision": current["selection_revision"]},
                "重新载入来源选择后再操作",
            )
        next_revision = current["selection_revision"] + 1
        connection.execute(
            "UPDATE working_baseline_selections SET revision_id = NULL, selection_revision = ?, selected_at = ? "
            "WHERE project_id = ? AND object_kind = ? AND object_id = ?",
            (next_revision, now(), project_id, kind, normalized_object_id),
        )
    return {
        "objectKind": kind,
        "objectId": normalized_object_id,
        "revisionId": None,
        "selectionRevision": next_revision,
        "cleared": True,
    }
