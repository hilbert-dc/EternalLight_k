import argparse
import hashlib
import json
import secrets
import sqlite3
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse

try:
    from .creative_workspace import planning_reviews
    from .creative_workspace.author_export import export_project
    from .creative_workspace.thread_lookup import (
        lookup_recovery_tasks,
        lookup_thread_tasks,
    )
    from .creative_workspace.metrics import local_metrics, record_abandonment
    from .creative_workspace.operation_ledger import (
        accept_operation,
        begin_operation_commit,
        commit_operation,
        finish_operation,
        is_committed_operation_replay,
        operation_identity,
        operation_rows,
        recovery_snapshot,
    )
    from .creative_workspace.errors import UnauthorizedError, WorkspaceError
    from .creative_workspace.service import (
        accept_review,
        cancel_task,
        complete_task,
        create_review,
        create_task,
        get_context_snapshot,
        recover_orphaned_tasks,
        save_artifact,
        save_draft_checkpoint,
        set_artifact_lock,
        undo_commit,
        update_task,
        workspace_state,
    )
    from .creative_workspace.planning_bindings import (
        associate_standalone_chapter_to_plan,
        materialize_chapter_from_plan,
        materialize_standalone_chapter,
    )
    from .creative_workspace.impacts import (
        planning_impact_page,
        resolve_planning_impact,
    )
    from .creative_workspace.working_baselines import (
        clear_working_baseline,
        save_continuation_intent,
        set_working_baseline,
    )
    from .storage import (
        chapter_diffs,
        create_project,
        list_projects,
        project_state,
        recover_search_indexes,
        run_story_tests,
        search_project,
    )
except ImportError:
    from creative_workspace import planning_reviews
    from creative_workspace.author_export import export_project
    from creative_workspace.thread_lookup import (
        lookup_recovery_tasks,
        lookup_thread_tasks,
    )
    from creative_workspace.metrics import local_metrics, record_abandonment
    from creative_workspace.operation_ledger import (
        accept_operation,
        begin_operation_commit,
        commit_operation,
        finish_operation,
        is_committed_operation_replay,
        operation_identity,
        operation_rows,
        recovery_snapshot,
    )
    from creative_workspace.errors import UnauthorizedError, WorkspaceError
    from creative_workspace.service import (
        accept_review,
        cancel_task,
        complete_task,
        create_review,
        create_task,
        get_context_snapshot,
        recover_orphaned_tasks,
        save_artifact,
        save_draft_checkpoint,
        set_artifact_lock,
        undo_commit,
        update_task,
        workspace_state,
    )
    from creative_workspace.planning_bindings import (
        associate_standalone_chapter_to_plan,
        materialize_chapter_from_plan,
        materialize_standalone_chapter,
    )
    from creative_workspace.impacts import planning_impact_page, resolve_planning_impact
    from creative_workspace.working_baselines import (
        clear_working_baseline,
        save_continuation_intent,
        set_working_baseline,
    )
    from storage import (
        chapter_diffs,
        create_project,
        list_projects,
        project_state,
        recover_search_indexes,
        run_story_tests,
        search_project,
    )


def _string(
    payload: dict[str, Any], key: str, *, required: bool = False, limit: int = 1_200
) -> str | None:
    value = payload.get(key)
    if value is None and not required:
        return None
    if (
        not isinstance(value, str)
        or (required and not value.strip())
        or len(value) > limit
        or "\x00" in value
    ):
        raise WorkspaceError("invalid_request", f"{key} 无效")
    return value.strip() if required else value


def _integer(
    payload: dict[str, Any], key: str, *, required: bool = False
) -> int | None:
    value = payload.get(key)
    if value is None and not required:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        raise WorkspaceError("invalid_request", f"{key} 必须是整数")
    return value


def _string_list(
    payload: dict[str, Any], key: str, *, limit: int = 50
) -> list[str] | None:
    value = payload.get(key)
    if value is None:
        return None
    if (
        not isinstance(value, list)
        or len(value) > limit
        or any(
            not isinstance(item, str) or not item.strip() or len(item) > 160
            for item in value
        )
    ):
        raise WorkspaceError("invalid_request", f"{key} 无效")
    return [item.strip() for item in value]


def _query_integer(query: dict[str, list[str]], key: str) -> int | None:
    value = query.get(key, [None])[0]
    if value is None or value == "":
        return None
    try:
        return int(value)
    except ValueError as error:
        raise WorkspaceError("invalid_request", f"{key} 必须是整数") from error


def _require_author(actor: str) -> None:
    if actor != "author":
        raise UnauthorizedError("只有作者可以采用、撤销或停止任务")


def _context_kwargs(payload: dict[str, Any], actor: str) -> dict[str, Any]:
    return {
        "purpose": _string(payload, "purpose", required=True, limit=20),
        "project_revision": _integer(payload, "projectRevision"),
        "story_time": _integer(payload, "storyTime"),
        "reading_chapter": _integer(payload, "readingChapter"),
        "scene_order": _integer(payload, "sceneOrder"),
        "pov_character_id": _string(payload, "povCharacterId", limit=160),
        "budget": _integer(payload, "budget") or 24,
        "byte_budget": _integer(payload, "byteBudget") or 24_000,
        "actor": actor,
        "task_id": _string(payload, "taskId", limit=160),
        "thread_id": _string(payload, "threadId", limit=160),
    }


def response_payload(
    path: str,
    method: str,
    body: dict[str, Any] | None,
    db_path: str,
    actor: str = "author",
    operation_connection: sqlite3.Connection | None = None,
) -> tuple[int, dict[str, Any]]:
    parsed = urlparse(path)
    parts = [part for part in parsed.path.split("/") if part]
    query = parse_qs(parsed.query)
    payload = body or {}

    if parsed.path == "/health" and method == "GET":
        return 200, {"ok": True, "service": "creative-runtime"}
    if parsed.path == "/api/projects" and method == "GET":
        _require_author(actor)
        return 200, {"projects": list_projects(db_path)}
    if parsed.path == "/api/projects" and method == "POST":
        _require_author(actor)
        title = _string(payload, "title", required=True, limit=120)
        logline = _string(payload, "logline", limit=500) or ""
        return 201, create_project(db_path, title, logline)
    if parsed.path == "/api/metrics" and method == "GET":
        _require_author(actor)
        return 200, local_metrics(db_path)
    if parsed.path == "/api/metrics/abandon" and method == "POST":
        _require_author(actor)
        return 200, record_abandonment(
            db_path,
            _string(payload, "stage", required=True, limit=12),
            _string(payload, "idempotencyKey", required=True, limit=36),
        )
    if parsed.path == "/api/tasks/recover-orphans" and method == "POST":
        _require_author(actor)
        return 200, recover_orphaned_tasks(db_path)

    if len(parts) >= 3 and parts[:2] == ["api", "projects"]:
        project_id = parts[2]
        suffix = parts[3:]
        if suffix == ["planning-reviews"] and method == "POST":
            _require_author(actor)
            return 201, planning_reviews.create_planning_review(
                db_path, project_id, payload
            )
        if len(suffix) >= 2 and suffix[0] == "planning-reviews":
            run_id = suffix[1]
            if suffix[2:] == ["reports"] and method == "POST":
                if actor != "candidate":
                    raise UnauthorizedError("规划报告只能由绑定的顾问任务提交")
                return 201, planning_reviews.save_planning_report(
                    db_path,
                    project_id,
                    run_id,
                    payload,
                    operation_connection=operation_connection,
                )
            _require_author(actor)
            if len(suffix) == 2 and method == "GET":
                return 200, planning_reviews.get_planning_review(
                    db_path, project_id, run_id
                )
            if (
                len(suffix) == 5
                and suffix[2] == "seats"
                and suffix[4] == "tasks"
                and method == "POST"
            ):
                return 201, planning_reviews.create_seat_task(
                    db_path, project_id, run_id, suffix[3], payload
                )
            if suffix[2:] in (["finalize"], ["cancel"]) and method == "POST":
                if payload:
                    raise WorkspaceError(
                        "invalid_planning_review", "结束评审不接受额外字段"
                    )
                return 200, planning_reviews.finish_planning_review(
                    db_path, project_id, run_id, cancel=suffix[2] == "cancel"
                )
        if suffix == ["export"] and method == "POST":
            _require_author(actor)
            return 200, export_project(db_path, project_id, payload)
        if suffix == ["state"] and method == "GET":
            _require_author(actor)
            return 200, project_state(db_path, project_id)
        if suffix == ["creative-workspace"] and method == "GET":
            _require_author(actor)
            return 200, workspace_state(db_path, project_id)
        if suffix == ["impacts"] and method == "GET":
            _require_author(actor)
            return 200, planning_impact_page(
                db_path,
                project_id,
                _query_integer(query, "limit") or 25,
                query.get("cursor", [None])[0],
            )
        if suffix == ["search"] and method == "GET":
            _require_author(actor)
            limit = _query_integer(query, "limit") or 20
            return 200, {
                "results": search_project(
                    db_path,
                    project_id,
                    query.get("q", [""])[0],
                    limit,
                    query.get("kind", [None])[0],
                )
            }
        if suffix == ["story-tests"] and method == "POST":
            _require_author(actor)
            return 200, run_story_tests(db_path, project_id)
        if suffix == ["tasks", "lookup"] and method == "POST":
            _require_author(actor)
            if set(payload) != {"threadIds"}:
                raise WorkspaceError("invalid_thread_lookup", "仅允许 threadIds 字段")
            return 200, lookup_thread_tasks(db_path, project_id, payload["threadIds"])
        if suffix == ["tasks", "recovery-lookup"] and method == "POST":
            _require_author(actor)
            if set(payload) != {"taskIds"}:
                raise WorkspaceError("invalid_task_lookup", "仅允许 taskIds 字段")
            return 200, lookup_recovery_tasks(db_path, project_id, payload["taskIds"])
        if suffix == ["tasks", "recovery-operations"] and method == "POST":
            _require_author(actor)
            if set(payload) != {"taskIds"}:
                raise WorkspaceError("invalid_operation_lookup", "仅允许 taskIds 字段")
            return 200, {
                "operations": operation_rows(db_path, project_id, payload["taskIds"])
            }
        if suffix == ["tasks", "recovery-snapshot"] and method == "POST":
            _require_author(actor)
            if (
                not set(payload).issubset({"taskIds", "fence", "cursor", "limit"})
                or "taskIds" not in payload
            ):
                raise WorkspaceError("invalid_recovery_snapshot", "恢复快照字段无效")
            fence = payload.get("fence")
            if fence is not None and (not isinstance(fence, str) or len(fence) > 160):
                raise WorkspaceError("invalid_recovery_fence", "恢复快照 fence 无效")
            return 200, recovery_snapshot(
                db_path,
                project_id,
                payload["taskIds"],
                fence=fence,
                cursor=payload.get("cursor", 0),
                limit=payload.get("limit", 500),
            )
        if suffix == ["tasks"] and method == "POST":
            _require_author(actor)
            allowed_fields = {
                "operation",
                "inputRevision",
                "threadId",
                "model",
                "target",
                "capabilityHash",
                "capabilityExpiresAt",
                "authorIntent",
            }
            unknown_fields = sorted(set(payload) - allowed_fields)
            if unknown_fields:
                raise WorkspaceError(
                    "invalid_task_contract",
                    "任务创建包含调用方不可控制的字段",
                    details={"fields": unknown_fields},
                )
            target = payload.get("target")
            if target is not None and not isinstance(target, dict):
                raise WorkspaceError("invalid_task_target", "生成目标必须是对象")
            task = create_task(
                db_path,
                project_id,
                _string(payload, "operation", required=True, limit=40),
                _integer(payload, "inputRevision", required=True),
                _string(payload, "threadId", required=True, limit=160),
                _string(payload, "model", limit=120),
                target,
                _string(payload, "capabilityHash", required=True, limit=64),
                _integer(payload, "capabilityExpiresAt", required=True),
                _string(payload, "authorIntent", required=True, limit=32_000),
            )
            return 201, task
        if len(suffix) == 2 and suffix[0] == "working-baselines" and method == "PUT":
            _require_author(actor)
            return 200, set_working_baseline(
                db_path,
                project_id,
                suffix[1],
                _string(payload, "objectId", limit=160),
                _string(payload, "revisionId", required=True, limit=160),
                _integer(payload, "expectedSelectionRevision", required=True),
            )
        if len(suffix) == 2 and suffix[0] == "continuations" and method == "PUT":
            _require_author(actor)
            return 200, save_continuation_intent(
                db_path,
                project_id,
                suffix[1],
                _string(payload, "targetId", limit=160),
                _string(payload, "workingRevisionId", required=True, limit=160),
                _string(payload, "authorIntent", required=True, limit=32_000),
            )
        if len(suffix) == 2 and suffix[0] == "working-baselines" and method == "DELETE":
            _require_author(actor)
            return 200, clear_working_baseline(
                db_path,
                project_id,
                suffix[1],
                _string(payload, "objectId", limit=160),
                _integer(payload, "expectedSelectionRevision", required=True),
            )
        if len(suffix) == 2 and suffix[0] == "tasks" and method == "PATCH":
            _require_author(actor)
            return 200, update_task(
                db_path,
                project_id,
                suffix[1],
                _string(payload, "threadId", required=True, limit=160),
                _string(payload, "status", required=True, limit=30),
                payload.get("outputRefs")
                if isinstance(payload.get("outputRefs"), list)
                else None,
                _string(payload, "error", limit=500),
            )
        if (
            len(suffix) == 3
            and suffix[0] == "tasks"
            and suffix[2] == "cancel"
            and method == "POST"
        ):
            _require_author(actor)
            return 200, cancel_task(db_path, project_id, suffix[1])
        if (
            len(suffix) == 3
            and suffix[0] == "tasks"
            and suffix[2] == "complete"
            and method == "POST"
        ):
            return 200, complete_task(
                db_path,
                project_id,
                suffix[1],
                _string(payload, "threadId", required=True, limit=160),
                _string(payload, "summary", limit=4000) or "",
                operation_connection=operation_connection,
            )
        if len(suffix) == 2 and suffix[0] == "artifacts" and method == "POST":
            artifact_payload = payload.get("payload")
            if not isinstance(artifact_payload, dict):
                raise WorkspaceError("invalid_artifact", "payload 必须是对象")
            if "expectedCandidateRevisionId" not in payload:
                raise WorkspaceError(
                    "candidate_cas_required",
                    "expectedCandidateRevisionId 必须显式提供；首次保存请传 null",
                )
            return 201, save_artifact(
                db_path,
                project_id,
                suffix[1],
                artifact_payload,
                _integer(payload, "expectedRevision", required=True),
                actor,
                _string(payload, "taskId", limit=160),
                _string(payload, "threadId", limit=160),
                _string(payload, "expectedCandidateRevisionId", limit=160),
                _string(payload, "idempotencyKey", required=True, limit=160),
                "expectedCandidateRevisionId" in payload,
                _string_list(payload, "linkedImpactIds", limit=50),
                operation_connection,
            )
        if suffix == ["chapters", "materialize-from-plan"] and method == "POST":
            _require_author(actor)
            return 201, materialize_chapter_from_plan(
                db_path,
                project_id,
                _string(payload, "chapterPlanId", required=True, limit=160),
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "idempotencyKey", required=True, limit=160),
            )
        if suffix == ["chapters", "materialize-standalone"] and method == "POST":
            _require_author(actor)
            return 201, materialize_standalone_chapter(
                db_path,
                project_id,
                _string(payload, "title", limit=160) or "",
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "idempotencyKey", required=True, limit=160),
            )
        if suffix == ["chapters", "associate-plan"] and method == "POST":
            _require_author(actor)
            return 201, associate_standalone_chapter_to_plan(
                db_path,
                project_id,
                _string(payload, "chapterId", required=True, limit=160),
                _string(payload, "chapterPlanId", required=True, limit=160),
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "idempotencyKey", required=True, limit=160),
            )
        if (
            len(suffix) == 3
            and suffix[0] == "impacts"
            and suffix[2] == "resolve"
            and method == "POST"
        ):
            _require_author(actor)
            return 200, resolve_planning_impact(
                db_path,
                project_id,
                suffix[1],
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "resolution", required=True, limit=20),
                _string(payload, "note", limit=2_000) or "",
                _string(payload, "idempotencyKey", required=True, limit=160),
            )
        if (
            len(suffix) == 3
            and suffix[0] == "artifacts"
            and suffix[2] == "lock"
            and method == "PATCH"
        ):
            _require_author(actor)
            locked = payload.get("locked")
            if not isinstance(locked, bool):
                raise WorkspaceError("invalid_request", "locked 必须是布尔值")
            return 200, set_artifact_lock(
                db_path,
                project_id,
                suffix[1],
                locked,
                _integer(payload, "expectedRevision", required=True),
            )
        if suffix == ["drafts", "checkpoints"] and method == "POST":
            draft_payload = payload.get("payload")
            if not isinstance(draft_payload, dict):
                raise WorkspaceError("invalid_draft", "payload 必须是对象")
            if "expectedCandidateRevisionId" not in payload:
                raise WorkspaceError(
                    "candidate_cas_required",
                    "expectedCandidateRevisionId 必须显式提供；首次保存请传 null",
                )
            saved = save_draft_checkpoint(
                db_path,
                project_id,
                draft_payload,
                _integer(payload, "expectedRevision", required=True),
                actor,
                _string(payload, "taskId", limit=160),
                _string(payload, "threadId", limit=160),
                _string(payload, "expectedCandidateRevisionId", limit=160),
                _string(payload, "idempotencyKey", required=True, limit=160),
                "expectedCandidateRevisionId" in payload,
                operation_connection,
            )
            if actor == "candidate":
                saved = {
                    key: value
                    for key, value in saved.items()
                    if key not in {"body", "sceneGroups"}
                }
            return 201, saved
        if suffix == ["reviews"] and method == "POST":
            proposals = payload.get("proposals")
            selections = payload.get("selections")
            if proposals is not None and not isinstance(proposals, list):
                raise WorkspaceError("invalid_review", "proposals 必须是数组")
            if selections is not None and not isinstance(selections, dict):
                raise WorkspaceError("invalid_review", "selections 必须是对象")
            return 201, create_review(
                db_path,
                project_id,
                _string(payload, "artifactKind", required=True, limit=20),
                _string(payload, "revisionId", required=True, limit=160),
                proposals,
                selections,
                actor,
                _string(payload, "taskId", limit=160),
                _string(payload, "threadId", limit=160),
                _string(payload, "idempotencyKey", required=True, limit=160),
                operation_connection,
            )
        if (
            len(suffix) == 3
            and suffix[0] == "reviews"
            and suffix[2] == "accept"
            and method == "POST"
        ):
            _require_author(actor)
            selections = payload.get("selections")
            if selections is not None and not isinstance(selections, dict):
                raise WorkspaceError("invalid_review", "selections 必须是对象")
            return 200, accept_review(
                db_path,
                project_id,
                suffix[1],
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "idempotencyKey", required=True, limit=160),
                selections,
            )
        if (
            len(suffix) == 3
            and suffix[0] == "commits"
            and suffix[2] == "undo"
            and method == "POST"
        ):
            _require_author(actor)
            return 200, undo_commit(
                db_path,
                project_id,
                suffix[1],
                _integer(payload, "expectedRevision", required=True),
                _string(payload, "idempotencyKey", required=True, limit=160),
            )
        if (
            len(suffix) == 3
            and suffix[0] == "chapters"
            and suffix[2] == "diffs"
            and method == "GET"
        ):
            _require_author(actor)
            return 200, {"diffs": chapter_diffs(db_path, project_id, suffix[1])}
        if suffix == ["context"] and method in {"GET", "POST"}:
            context_payload: dict[str, Any] = (
                payload
                if method == "POST"
                else {key: values[0] for key, values in query.items()}
            )
            for key in [
                "projectRevision",
                "storyTime",
                "readingChapter",
                "sceneOrder",
                "budget",
                "byteBudget",
            ]:
                if key in context_payload and isinstance(context_payload[key], str):
                    try:
                        context_payload[key] = int(context_payload[key])
                    except ValueError as error:
                        raise WorkspaceError(
                            "invalid_request", f"{key} 必须是整数"
                        ) from error
            return 200, get_context_snapshot(
                db_path, project_id, **_context_kwargs(context_payload, actor)
            )

    raise KeyError("Route not found")


def _error_payload(error: WorkspaceError) -> dict[str, Any]:
    return error.payload()


def _actor_from_header(
    header: str | None, candidate_token: str, author_token: str
) -> str:
    if not header or not header.startswith("Bearer "):
        raise UnauthorizedError("缺少 Runtime 认证")
    token = header.removeprefix("Bearer ").strip()
    if secrets.compare_digest(token, author_token):
        return "author"
    if secrets.compare_digest(token, candidate_token):
        return "candidate"
    raise UnauthorizedError("Runtime 认证无效")


def authorize_candidate_capability(
    db_path: str,
    path: str,
    body: dict[str, Any] | None,
    capability: str | None,
) -> None:
    """Authorize one candidate request against its bound TaskRun capability."""
    parsed = urlparse(path)
    parts = [part for part in parsed.path.split("/") if part]
    query = parse_qs(parsed.query)
    payload = body or {}
    project_id = (
        parts[2] if len(parts) >= 3 and parts[:2] == ["api", "projects"] else None
    )
    task_id = payload.get("taskId") or query.get("taskId", [None])[0]
    if len(parts) == 6 and parts[3] == "tasks" and parts[5] == "complete":
        task_id = parts[4]
    thread_id = payload.get("threadId") or query.get("threadId", [None])[0]
    expected_revision = (
        payload["expectedRevision"]
        if "expectedRevision" in payload
        else query.get("projectRevision", [None])[0]
    )
    if isinstance(expected_revision, str):
        try:
            expected_revision = int(expected_revision)
        except ValueError as error:
            raise UnauthorizedError("候选请求的作品版本绑定无效") from error
    if not all(
        isinstance(value, str) and value.strip()
        for value in (project_id, task_id, thread_id, capability)
    ):
        raise UnauthorizedError("候选请求缺少任务能力凭证或完整绑定")
    if len(capability) < 24:
        raise UnauthorizedError("任务能力凭证无效")
    capability_hash = hashlib.sha256(capability.encode("utf-8")).hexdigest()
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    try:
        task = connection.execute(
            "SELECT project_id, thread_id, status, input_revision, capability_hash, capability_expires_at, capability_revoked_at "
            "FROM task_runs WHERE id = ? AND project_id = ? AND thread_id = ?",
            (task_id, project_id, thread_id),
        ).fetchone()
    finally:
        connection.close()
    if task is None:
        raise UnauthorizedError("任务能力凭证与作品或线程不匹配")
    if (
        isinstance(expected_revision, bool)
        or not isinstance(expected_revision, int)
        or expected_revision != task["input_revision"]
    ):
        raise UnauthorizedError("候选请求的作品版本与任务不匹配")
    expected_hash = task["capability_hash"]
    if not isinstance(expected_hash, str) or not secrets.compare_digest(
        capability_hash, expected_hash
    ):
        raise UnauthorizedError("任务能力凭证无效")
    if (
        task["status"] not in {"queued", "running", "waiting"}
        or task["capability_revoked_at"] is not None
    ):
        raise UnauthorizedError("任务能力凭证已撤销")
    expires_at = task["capability_expires_at"]
    if (
        isinstance(expires_at, bool)
        or not isinstance(expires_at, int)
        or expires_at <= int(time.time())
    ):
        raise UnauthorizedError("任务能力凭证已过期")


def _execute_request(
    db_path: str,
    path: str,
    method: str,
    body: dict[str, Any] | None,
    actor: str,
    execution_headers: dict[str, str | None],
) -> tuple[int, dict[str, Any]]:
    identity = (
        operation_identity(path, method, body, execution_headers)
        if actor == "candidate"
        else None
    )
    if identity is None:
        return response_payload(path, method, body, db_path, actor)
    existing = accept_operation(db_path, identity)
    if existing is not None:
        state = existing["state"]
        if state == "committed":
            receipt = json.loads(existing["receipt_json"])
            return receipt["status"], receipt["payload"]
        raise WorkspaceError(
            "operation_not_replayable", f"工具调用已登记为 {state}，不会自动重放", 409
        )
    operation_connection = None
    try:
        operation_connection = begin_operation_commit(db_path, identity)
        status, payload = response_payload(
            path,
            method,
            body,
            db_path,
            actor,
            operation_connection=operation_connection,
        )
        commit_operation(
            operation_connection,
            identity.operation_id,
            {"status": status, "payload": payload},
        )
    except WorkspaceError as error:
        if operation_connection is not None:
            operation_connection.rollback()
            operation_connection.close()
        finish_operation(
            db_path,
            identity.operation_id,
            "cancelled-before-execution"
            if isinstance(error, UnauthorizedError)
            else "rejected",
            error_code=error.code,
        )
        raise
    except Exception:
        if operation_connection is not None:
            operation_connection.rollback()
            operation_connection.close()
        finish_operation(
            db_path, identity.operation_id, "rejected", error_code="runtime_exception"
        )
        raise
    try:
        operation_connection.commit()
    except Exception:
        operation_connection.rollback()
        operation_connection.close()
        finish_operation(
            db_path,
            identity.operation_id,
            "outcome-unknown",
            error_code="commit_outcome_unknown",
        )
        raise
    operation_connection.close()
    return status, payload


class RuntimeHandler(BaseHTTPRequestHandler):
    db_path = "novel-codex-v2.sqlite3"
    candidate_token = ""
    author_token = ""

    def log_message(self, format: str, *args: object) -> None:
        return

    def send_json(self, status: int, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def handle_request(self, method: str) -> None:
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length > 512 * 1024:
                raise WorkspaceError("request_too_large", "请求体超过 512 KiB", 413)
            parsed_body = json.loads(self.rfile.read(length)) if length else None
            if parsed_body is not None and not isinstance(parsed_body, dict):
                raise WorkspaceError("invalid_request", "请求体必须是对象")
            actor = (
                "author"
                if self.path == "/health"
                else _actor_from_header(
                    self.headers.get("Authorization"),
                    self.candidate_token,
                    self.author_token,
                )
            )
            execution_headers = {
                "root_thread_id": self.headers.get("X-Novel-Root-Thread"),
                "caller_thread_id": self.headers.get("X-Novel-Caller-Thread"),
                "parent_thread_id": self.headers.get("X-Novel-Parent-Thread"),
                "turn_id": self.headers.get("X-Novel-Turn"),
                "tool_call_id": self.headers.get("X-Novel-Tool-Call"),
                "tool_name": self.headers.get("X-Novel-Tool-Name"),
            }
            if actor == "candidate" and not is_committed_operation_replay(
                self.db_path, self.path, method, parsed_body, execution_headers
            ):
                authorize_candidate_capability(
                    self.db_path,
                    self.path,
                    parsed_body,
                    self.headers.get("X-Novel-Task-Capability"),
                )
            status, payload = _execute_request(
                self.db_path,
                self.path,
                method,
                parsed_body,
                actor,
                execution_headers,
            )
            self.send_json(status, payload)
        except WorkspaceError as error:
            self.send_json(error.status, _error_payload(error))
        except KeyError as error:
            self.send_json(
                404,
                {
                    "error": {
                        "code": "not_found",
                        "message": str(error),
                        "details": {},
                        "recovery": None,
                    }
                },
            )
        except (ValueError, TypeError, json.JSONDecodeError) as error:
            self.send_json(
                400,
                {
                    "error": {
                        "code": "invalid_request",
                        "message": str(error),
                        "details": {},
                        "recovery": None,
                    }
                },
            )
        except sqlite3.OperationalError as error:
            status = (
                503
                if "locked" in str(error).lower() or "busy" in str(error).lower()
                else 500
            )
            self.send_json(
                status,
                {
                    "error": {
                        "code": "storage_busy" if status == 503 else "storage_error",
                        "message": str(error),
                        "details": {},
                        "recovery": "稍后重试" if status == 503 else None,
                    }
                },
            )
        except Exception as error:  # noqa: BLE001 - HTTP boundary must serialize failures.
            self.send_json(
                500,
                {
                    "error": {
                        "code": "internal_error",
                        "message": str(error),
                        "details": {},
                        "recovery": "重试；若持续失败请查看 Runtime 日志",
                    }
                },
            )

    def do_GET(self) -> None:
        self.handle_request("GET")

    def do_POST(self) -> None:
        self.handle_request("POST")

    def do_PATCH(self) -> None:
        self.handle_request("PATCH")


def create_fastapi_app(
    db_path: str,
    candidate_token: str = "candidate-test",
    author_token: str = "author-test",
):
    from fastapi import FastAPI, Request
    from fastapi.responses import JSONResponse

    app = FastAPI(title="Novel Codex Creative Runtime", version="0.2.0")

    @app.api_route("/{route:path}", methods=["GET", "POST", "PATCH"])
    async def dispatch(route: str, request: Request):
        path = f"/{route}"
        if request.url.query:
            path = f"{path}?{request.url.query}"
        try:
            if int(request.headers.get("content-length", "0")) > 512 * 1024:
                raise WorkspaceError("request_too_large", "请求体超过 512 KiB", 413)
            parsed_body = (
                await request.json() if request.method in {"POST", "PATCH"} else None
            )
            if parsed_body is not None and not isinstance(parsed_body, dict):
                raise WorkspaceError("invalid_request", "请求体必须是对象")
            actor = (
                "author"
                if path == "/health"
                else _actor_from_header(
                    request.headers.get("authorization"), candidate_token, author_token
                )
            )
            execution_headers = {
                "root_thread_id": request.headers.get("x-novel-root-thread"),
                "caller_thread_id": request.headers.get("x-novel-caller-thread"),
                "parent_thread_id": request.headers.get("x-novel-parent-thread"),
                "turn_id": request.headers.get("x-novel-turn"),
                "tool_call_id": request.headers.get("x-novel-tool-call"),
                "tool_name": request.headers.get("x-novel-tool-name"),
            }
            if actor == "candidate" and not is_committed_operation_replay(
                db_path, path, request.method, parsed_body, execution_headers
            ):
                authorize_candidate_capability(
                    db_path,
                    path,
                    parsed_body,
                    request.headers.get("x-novel-task-capability"),
                )
            status, payload = _execute_request(
                db_path,
                path,
                request.method,
                parsed_body,
                actor,
                execution_headers,
            )
            return JSONResponse(payload, status_code=status)
        except WorkspaceError as error:
            return JSONResponse(_error_payload(error), status_code=error.status)
        except KeyError as error:
            return JSONResponse(
                {
                    "error": {
                        "code": "not_found",
                        "message": str(error),
                        "details": {},
                        "recovery": None,
                    }
                },
                status_code=404,
            )
        except (ValueError, TypeError, json.JSONDecodeError) as error:
            return JSONResponse(
                {
                    "error": {
                        "code": "invalid_request",
                        "message": str(error),
                        "details": {},
                        "recovery": None,
                    }
                },
                status_code=400,
            )
        except sqlite3.OperationalError as error:
            status = (
                503
                if "locked" in str(error).lower() or "busy" in str(error).lower()
                else 500
            )
            return JSONResponse(
                {
                    "error": {
                        "code": "storage_busy" if status == 503 else "storage_error",
                        "message": str(error),
                        "details": {},
                        "recovery": "稍后重试" if status == 503 else None,
                    }
                },
                status_code=status,
            )
        except Exception as error:  # noqa: BLE001 - HTTP boundary must serialize failures.
            return JSONResponse(
                {
                    "error": {
                        "code": "internal_error",
                        "message": str(error),
                        "details": {},
                        "recovery": None,
                    }
                },
                status_code=500,
            )

    return app


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument(
        "--db", default=str(Path.home() / ".novel-codex" / "novel-codex-v2.sqlite3")
    )
    parser.add_argument("--token-stdin", action="store_true", required=True)
    args = parser.parse_args()
    if args.host != "127.0.0.1":
        raise SystemExit("Creative Runtime only supports loopback host 127.0.0.1")
    raw_credentials = sys.stdin.buffer.readline(4097)
    if not raw_credentials or len(raw_credentials) > 4096:
        raise SystemExit("Creative Runtime credentials were not supplied over stdin")
    try:
        credentials = json.loads(raw_credentials)
        candidate_token = credentials["candidateToken"]
        author_token = credentials["authorToken"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise SystemExit("Creative Runtime credentials are invalid") from error
    if (
        not isinstance(candidate_token, str)
        or not isinstance(author_token, str)
        or len(candidate_token) < 24
        or len(author_token) < 24
        or candidate_token == author_token
    ):
        raise SystemExit(
            "Creative Runtime tokens must be distinct and at least 24 characters"
        )
    Path(args.db).parent.mkdir(parents=True, exist_ok=True)
    recover_search_indexes(args.db)
    try:
        import uvicorn

        uvicorn.run(
            create_fastapi_app(args.db, candidate_token, author_token),
            host=args.host,
            port=args.port,
            log_level="warning",
        )
    except ImportError:
        RuntimeHandler.db_path = args.db
        RuntimeHandler.candidate_token = candidate_token
        RuntimeHandler.author_token = author_token
        server = ThreadingHTTPServer((args.host, args.port), RuntimeHandler)
        print(
            f"Creative Runtime listening on http://{args.host}:{args.port}", flush=True
        )
        try:
            server.serve_forever()
        finally:
            server.server_close()


if __name__ == "__main__":
    main()
