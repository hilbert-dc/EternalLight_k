import json
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

from .errors import ConflictError, NotFoundError, WorkspaceError
from .working_baselines import resolve_draft_revision, resolve_planning_revision

try:
    from ..storage import connect, initialize, now
except ImportError:
    from storage import connect, initialize, now


ACTIVE_STATUSES = {"queued", "running", "waiting"}
TASK_STATUSES = {*ACTIVE_STATUSES, "completed", "failed", "cancelled"}
OPERATION_CONTRACTS = {
    "advisory": ("advisory", "planner", None),
    "generateBrief": ("brief", "planner", "brief"),
    "generateOutline": ("outline", "planner", "outline"),
    "generatePlan": ("plan", "planner", "plan"),
    "generateChapter": ("draft", "writer", "draft"),
}
WHOLE_CHAPTER_SCENE_ORDER = 2_147_483_647


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _load(value: str | None, fallback: Any) -> Any:
    return json.loads(value) if value else fallback


def _validate_target(
    connection, project_id: str, purpose: str, target: dict[str, Any]
) -> dict[str, Any]:
    if not isinstance(target, dict):
        raise WorkspaceError("invalid_task_target", "生成目标必须是对象")
    if purpose != "writer":
        if target and target.get("type") not in {None, "project", "artifact"}:
            raise WorkspaceError("invalid_task_target", "该任务用途不支持此生成目标")
        return target
    if not target:
        return {}
    if target.get("type") != "chapter":
        raise WorkspaceError("invalid_task_target", "写作任务必须指定章节目标")
    chapter_id = target.get("chapterId")
    if not isinstance(chapter_id, str) or not chapter_id.strip():
        raise WorkspaceError("invalid_task_target", "写作任务缺少章节 ID")
    chapter = connection.execute(
        "SELECT id, number, title FROM chapters WHERE id = ? AND project_id = ?",
        (chapter_id, project_id),
    ).fetchone()
    if chapter is None:
        raise WorkspaceError("invalid_task_target", "目标章节不属于当前作品", 422)
    chapter_number = target.get("chapterNumber")
    if chapter_number is not None and chapter_number != chapter["number"]:
        raise WorkspaceError("invalid_task_target", "目标章节编号与章节 ID 不匹配")
    scope = target.get("scope")
    if not isinstance(scope, dict) or scope.get("type") not in {
        "wholeChapter",
        "throughScene",
    }:
        raise WorkspaceError(
            "invalid_task_target", "写作任务必须明确选择整章或场景范围"
        )
    if scope["type"] == "wholeChapter":
        if set(scope) != {"type"}:
            raise WorkspaceError("invalid_task_target", "整章范围不能包含额外字段")
        scene_order = WHOLE_CHAPTER_SCENE_ORDER
    else:
        if set(scope) != {"type", "endSceneOrder"}:
            raise WorkspaceError("invalid_task_target", "场景范围字段无效")
        scene_order = scope.get("endSceneOrder")
        if (
            isinstance(scene_order, bool)
            or not isinstance(scene_order, int)
            or scene_order < 0
        ):
            raise WorkspaceError("invalid_task_target", "目标场景顺序无效")
    pov_character_id = target.get("povCharacterId")
    if pov_character_id is not None:
        character = connection.execute(
            "SELECT 1 FROM characters WHERE id = ? AND project_id = ?",
            (pov_character_id, project_id),
        ).fetchone()
        if character is None:
            raise WorkspaceError("invalid_task_target", "视角人物不属于当前作品")
    story_time = target.get("storyTime")
    if story_time is not None and (
        isinstance(story_time, bool)
        or not isinstance(story_time, int)
        or story_time < 0
    ):
        raise WorkspaceError("invalid_task_target", "目标故事时间无效")
    return {
        "type": "chapter",
        "chapterId": chapter["id"],
        "chapterNumber": chapter["number"],
        "readingChapter": chapter["number"],
        "chapterTitle": chapter["title"],
        "scope": scope,
        "sceneOrder": scene_order,
        **({"povCharacterId": pov_character_id} if pov_character_id else {}),
        **({"storyTime": story_time} if story_time is not None else {}),
    }


def _task_row(connection, project_id: str, task_id: str, thread_id: str):
    task = connection.execute(
        "SELECT * FROM task_runs WHERE id = ? AND project_id = ? AND thread_id = ?",
        (task_id, project_id, thread_id),
    ).fetchone()
    if task is None:
        raise WorkspaceError("task_binding_invalid", "任务不属于当前作品或线程", 401)
    return task


def require_active_task(
    connection, project_id: str, task_id: str | None, thread_id: str | None
):
    if not task_id or not thread_id:
        raise WorkspaceError(
            "task_binding_required", "Agent 工具必须提供任务与线程绑定", 401
        )
    task = _task_row(connection, project_id, task_id, thread_id)
    if task["status"] not in ACTIVE_STATUSES:
        raise ConflictError(
            "task_not_writable",
            "任务已结束，不能继续写入",
            {"status": task["status"]},
            "创建新任务或从有效检查点重试",
        )
    workspace = connection.execute(
        "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
    ).fetchone()
    if workspace is None:
        raise NotFoundError("作品工作区不存在")
    if task["input_revision"] != workspace["revision"]:
        raise ConflictError(
            "stale_task_revision",
            "任务基于旧作品版本",
            {
                "taskRevision": task["input_revision"],
                "currentRevision": workspace["revision"],
            },
            "基于最新作品创建新任务",
        )
    return task


def task_target(task) -> dict[str, Any]:
    return _load(task["target_json"], {})


def assert_task_purpose(task, purpose: str) -> None:
    if task["purpose"] != purpose:
        raise WorkspaceError(
            "task_purpose_forbidden",
            "任务不能读取其他用途的创作上下文",
            401,
            {"taskPurpose": task["purpose"], "requestedPurpose": purpose},
            "使用与任务匹配的上下文用途",
        )


def assert_task_output(task, output_kind: str) -> None:
    required_output = task["required_output_kind"]
    allowed = {required_output} if required_output is not None else set()
    if required_output == "draft":
        allowed.add("draft-review")
    if output_kind not in allowed:
        raise WorkspaceError(
            "task_output_forbidden",
            "任务不能写入 operation 未授权的创作产物",
            401,
            {
                "operation": task["operation"],
                "requiredOutputKind": required_output,
                "outputKind": output_kind,
            },
            "使用该任务 operation 对应的候选工具",
        )


def assert_draft_target(task, chapter_id: str) -> None:
    target = task_target(task)
    target_chapter_id = target.get("chapterId")
    if target_chapter_id and target_chapter_id != chapter_id:
        raise WorkspaceError(
            "task_target_mismatch",
            "草稿章节与生成任务目标不一致",
            409,
            {"targetChapterId": target_chapter_id, "draftChapterId": chapter_id},
            "将草稿保存到任务指定章节",
        )


def record_task_output(
    connection,
    task_id: str,
    output_kind: str,
    revision_id: str,
    completeness: str | None = None,
) -> None:
    ordinal = connection.execute(
        "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM task_outputs WHERE task_id = ?",
        (task_id,),
    ).fetchone()[0]
    connection.execute(
        "INSERT INTO task_outputs(task_id, ordinal, output_kind, revision_id, completeness, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        (task_id, ordinal, output_kind, revision_id, completeness, now()),
    )


def create_task(
    db_path: str | Path,
    project_id: str,
    operation: str,
    input_revision: int,
    thread_id: str,
    model: str | None = None,
    target: dict[str, Any] | None = None,
    capability_hash: str | None = None,
    capability_expires_at: int | None = None,
    author_intent: str | None = None,
) -> dict[str, Any]:
    if not isinstance(operation, str) or operation not in OPERATION_CONTRACTS:
        raise WorkspaceError("invalid_task_operation", "任务 operation 无效")
    if not isinstance(thread_id, str) or not thread_id.strip():
        raise WorkspaceError("invalid_task", "任务线程不能为空")
    kind, resolved_purpose, required_output = OPERATION_CONTRACTS[operation]
    if operation == "generateChapter" and not target:
        raise WorkspaceError(
            "invalid_task_target", "章节生成必须绑定一个已经创建的明确章节"
        )
    if (capability_hash is None) != (capability_expires_at is None):
        raise WorkspaceError("invalid_task_capability", "任务能力凭证绑定不完整")
    if capability_hash is not None and (
        len(capability_hash) != 64
        or any(character not in "0123456789abcdef" for character in capability_hash)
        or isinstance(capability_expires_at, bool)
        or not isinstance(capability_expires_at, int)
        or capability_expires_at <= int(time.time())
        or capability_expires_at > int(time.time()) + 3600
    ):
        raise WorkspaceError("invalid_task_capability", "任务能力凭证绑定无效")
    if author_intent is not None and (
        not isinstance(author_intent, str)
        or not author_intent.strip()
        or len(author_intent.encode("utf-8")) > 64_000
    ):
        raise WorkspaceError("invalid_author_intent", "作者创作意图无效或超过 64 KiB")
    initialize(db_path)
    task_id = f"task-{uuid4().hex}"
    timestamp = now()
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        workspace = connection.execute(
            "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
        ).fetchone()
        if workspace is None:
            project = connection.execute(
                "SELECT 1 FROM projects WHERE id = ?", (project_id,)
            ).fetchone()
            if project is None:
                raise NotFoundError("作品不存在")
            connection.execute(
                "INSERT INTO project_workspace(project_id) VALUES (?)", (project_id,)
            )
            workspace = connection.execute(
                "SELECT revision FROM project_workspace WHERE project_id = ?",
                (project_id,),
            ).fetchone()
        if input_revision != workspace["revision"]:
            raise ConflictError(
                "stale_project_revision",
                "任务输入版本已过期",
                {"currentRevision": workspace["revision"]},
                "基于最新作品重新开始任务",
            )
        normalized_target = _validate_target(
            connection, project_id, resolved_purpose, target or {}
        )
        accepted_pointers = {
            row["artifact_kind"]: row["accepted_revision_id"]
            for row in connection.execute(
                "SELECT artifact_kind, accepted_revision_id FROM artifact_pointers WHERE project_id = ?",
                (project_id,),
            ).fetchall()
        }
        planning_baseline = None
        local_only_snapshot = None
        if operation != "generateChapter":
            brief_id, brief_role = resolve_planning_revision(
                connection, project_id, "brief"
            )
            outline_id, outline_role = resolve_planning_revision(
                connection, project_id, "outline"
            )
            plan_id, plan_role = resolve_planning_revision(
                connection, project_id, "plan"
            )
            planning_baseline = {
                "briefRevisionId": brief_id,
                "briefSourceRole": brief_role,
                "outlineRevisionId": outline_id,
                "outlineSourceRole": outline_role,
                "planRevisionId": plan_id,
                "planSourceRole": plan_role,
                "lineageStatus": "current",
            }
        if operation == "generateChapter":
            binding = connection.execute(
                "SELECT plan_revision_id, chapter_plan_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ?",
                (project_id, normalized_target["chapterId"]),
            ).fetchone()
            if binding:
                selected_plan_id, selected_plan_role = resolve_planning_revision(
                    connection, project_id, "plan"
                )
                plan_id = (
                    selected_plan_id
                    if selected_plan_role == "workingSelected"
                    else binding["plan_revision_id"]
                )
                plan_row = (
                    connection.execute(
                        "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ?",
                        (plan_id, project_id),
                    ).fetchone()
                    if plan_id
                    else None
                )
                plan = _load(plan_row["payload_json"], {}) if plan_row else {}
                plan_entry = next(
                    (
                        entry
                        for entry in plan.get("chapters", [])
                        if entry.get("chapterPlanId") == binding["chapter_plan_id"]
                    ),
                    None,
                )
                if plan_entry is None:
                    raise ConflictError(
                        "planning_binding_broken",
                        "本章绑定的历史章纲不存在",
                        recovery="检查作品数据并重新建立明确的规划绑定",
                    )
                outline_id = plan.get("sourceOutlineRevisionId")
                planning_baseline = {
                    "outlineRevisionId": outline_id,
                    "planRevisionId": plan_id,
                    "chapterPlanId": binding["chapter_plan_id"],
                    "planSourceRole": selected_plan_role
                    if plan_id == selected_plan_id
                    else "accepted",
                    "lineageStatus": "working"
                    if selected_plan_role == "workingSelected"
                    and plan_id == selected_plan_id
                    else "current"
                    if plan_id == accepted_pointers.get("plan")
                    and outline_id == accepted_pointers.get("outline")
                    else "historical",
                }
                if "povCharacterId" not in normalized_target and plan_entry.get(
                    "povCharacterId"
                ):
                    normalized_target["povCharacterId"] = plan_entry["povCharacterId"]
                if "storyTime" not in normalized_target and isinstance(
                    plan_entry.get("storyTime"), int
                ):
                    normalized_target["storyTime"] = plan_entry["storyTime"]
                normalized_target = _validate_target(
                    connection, project_id, resolved_purpose, normalized_target
                )
            else:
                local_only_snapshot = {
                    "chapterId": normalized_target["chapterId"],
                    "projectRevision": input_revision,
                    "localOnly": True,
                }
        candidate = None
        base_target_revision_id = None
        if required_output in {"brief", "outline", "plan"}:
            candidate = connection.execute(
                "SELECT candidate_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
                (project_id, required_output),
            ).fetchone()
            candidate = candidate["candidate_revision_id"] if candidate else None
        elif required_output == "draft" and normalized_target.get("chapterId"):
            candidate = connection.execute(
                "SELECT candidate_revision_id FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                (project_id, normalized_target["chapterId"]),
            ).fetchone()
            candidate = candidate["candidate_revision_id"] if candidate else None
            base_target_revision_id, base_target_source_role = resolve_draft_revision(
                connection, project_id, normalized_target["chapterId"]
            )
        reference_drafts = []
        if operation in {"generatePlan", "generateChapter"}:
            maximum_chapter = normalized_target.get("chapterNumber")
            chapter_rows = connection.execute(
                "SELECT id, number FROM chapters WHERE project_id = ? "
                + ("AND number < ? " if maximum_chapter is not None else "")
                + "ORDER BY number DESC LIMIT ?",
                (
                    (project_id, maximum_chapter, 6)
                    if maximum_chapter is not None
                    else (project_id, 3)
                ),
            ).fetchall()
            for chapter_row in chapter_rows:
                revision_id, source_role = resolve_draft_revision(
                    connection, project_id, chapter_row["id"]
                )
                if revision_id:
                    reference_drafts.append(
                        {
                            "chapterId": chapter_row["id"],
                            "chapterNumber": chapter_row["number"],
                            "revisionId": revision_id,
                            "sourceRole": source_role,
                        }
                    )
        manifest = {
            "operation": operation,
            "projectId": project_id,
            "projectRevision": input_revision,
            "target": normalized_target,
            "expectedCandidateRevisionId": candidate,
            "baseTargetRevisionId": base_target_revision_id,
            "referenceDrafts": reference_drafts,
            "branchContext": None,
        }
        if required_output == "draft":
            manifest["baseTargetSourceRole"] = base_target_source_role
        if author_intent is not None:
            manifest["authorIntent"] = author_intent.strip()
        if planning_baseline is not None:
            manifest["planningBaseline"] = planning_baseline
        if local_only_snapshot is not None:
            manifest["localOnlySnapshot"] = local_only_snapshot
        connection.execute(
            "INSERT INTO task_runs(id, project_id, thread_id, task_kind, purpose, target_json, "
            "required_output_kind, stage, delivery_status, status, input_revision, model, operation, input_manifest_json, candidate_head_revision_id, capability_hash, capability_expires_at, created_at, updated_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, 'preparing', 'pending', 'queued', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                task_id,
                project_id,
                thread_id,
                kind[:80],
                resolved_purpose,
                _json(normalized_target),
                required_output,
                input_revision,
                model[:120] if model else None,
                operation,
                _json(manifest),
                candidate,
                capability_hash,
                capability_expires_at,
                timestamp,
                timestamp,
            ),
        )
        if required_output == "draft" and author_intent is not None:
            connection.execute(
                "UPDATE continuation_intents SET status = 'launched', task_id = ?, updated_at = ? "
                "WHERE project_id = ? AND target_kind = 'draft' AND target_id = ? "
                "AND working_revision_id = ? AND author_intent = ?",
                (
                    task_id,
                    timestamp,
                    project_id,
                    normalized_target["chapterId"],
                    base_target_revision_id,
                    author_intent.strip(),
                ),
            )
    return {
        "id": task_id,
        "projectId": project_id,
        "threadId": thread_id,
        "kind": kind[:80],
        "purpose": resolved_purpose,
        "target": normalized_target,
        "requiredOutputKind": required_output,
        "operation": operation,
        "inputManifest": manifest,
        "stage": "preparing",
        "status": "queued",
        "deliveryStatus": "pending",
        "inputRevision": input_revision,
    }


def update_task(
    db_path: str | Path,
    project_id: str,
    task_id: str,
    thread_id: str,
    status: str,
    output_refs: list[dict[str, Any]] | None = None,
    error: str | None = None,
) -> dict[str, Any]:
    if status not in TASK_STATUSES:
        raise WorkspaceError("invalid_task_status", "任务状态无效")
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        task = _task_row(connection, project_id, task_id, thread_id)
        if task["status"] not in ACTIVE_STATUSES:
            raise ConflictError(
                "task_not_writable",
                "任务已结束，不能继续写入",
                {"status": task["status"]},
            )
        if status not in {"failed", "cancelled"}:
            require_active_task(connection, project_id, task_id, thread_id)
        refs = (
            output_refs
            if output_refs is not None
            else _load(task["output_refs_json"], [])
        )
        delivery_status = (
            "incomplete" if status in {"failed", "cancelled"} and refs else "pending"
        )
        connection.execute(
            "UPDATE task_runs SET status = ?, stage = ?, delivery_status = ?, output_refs_json = ?, "
            "error = ?, capability_revoked_at = CASE WHEN ? IN ('completed','failed','cancelled') THEN ? ELSE capability_revoked_at END, updated_at = ? WHERE id = ?",
            (
                status,
                status,
                delivery_status,
                _json(refs[:100]),
                error[:500] if error else None,
                status,
                now(),
                now(),
                task_id,
            ),
        )
        if status == "failed":
            connection.execute(
                "UPDATE continuation_intents SET status = 'failed', updated_at = ? "
                "WHERE project_id = ? AND task_id = ?",
                (now(), project_id, task_id),
            )
    return {
        "id": task_id,
        "status": status,
        "deliveryStatus": delivery_status,
        "outputRefs": refs,
    }


def cancel_task(db_path: str | Path, project_id: str, task_id: str) -> dict[str, Any]:
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        task = connection.execute(
            "SELECT * FROM task_runs WHERE id = ? AND project_id = ?",
            (task_id, project_id),
        ).fetchone()
        if task is None:
            raise NotFoundError("任务不存在")
        refs = _load(task["output_refs_json"], [])
        if task["status"] in ACTIVE_STATUSES:
            delivery_status = "incomplete" if refs else "pending"
            connection.execute(
                "UPDATE task_runs SET status = 'cancelled', stage = 'cancelled', delivery_status = ?, capability_revoked_at = ?, updated_at = ? WHERE id = ?",
                (delivery_status, now(), now(), task_id),
            )
        else:
            delivery_status = task["delivery_status"]
    return {
        "id": task_id,
        "status": "cancelled" if task["status"] in ACTIVE_STATUSES else task["status"],
        "deliveryStatus": delivery_status,
        "outputRefs": refs,
    }


def recover_orphaned_tasks(db_path: str | Path) -> dict[str, Any]:
    initialize(db_path)
    recovered_at = now()
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        rows = connection.execute(
            "SELECT id, output_refs_json FROM task_runs WHERE status IN ('queued','running','waiting')"
        ).fetchall()
        for task in rows:
            delivery_status = (
                "incomplete" if _load(task["output_refs_json"], []) else "pending"
            )
            connection.execute(
                "UPDATE task_runs SET status = 'cancelled', stage = 'cancelled', delivery_status = ?, "
                "error = '应用重启后已回收孤儿任务', capability_revoked_at = ?, updated_at = ? WHERE id = ?",
                (delivery_status, recovered_at, recovered_at, task["id"]),
            )
    return {"recovered": len(rows), "taskIds": [task["id"] for task in rows]}


def complete_task(
    db_path: str | Path,
    project_id: str,
    task_id: str,
    thread_id: str,
    summary: str = "",
) -> dict[str, Any]:
    initialize(db_path)
    missing_required_output = False
    failure_code = "required_output_missing"
    target: dict[str, Any] = {}
    refs: list[dict[str, Any]] = []
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        task = _task_row(connection, project_id, task_id, thread_id)
        if (
            task["status"] == "completed"
            and task["delivery_status"] == "ready_for_review"
        ):
            return {
                "id": task_id,
                "status": "completed",
                "stage": "ready_for_review",
                "deliveryStatus": "ready_for_review",
                "outputRefs": _load(task["output_refs_json"], []),
            }
        task = require_active_task(connection, project_id, task_id, thread_id)
        target = task_target(task)
        refs = _load(task["output_refs_json"], [])
        required = task["required_output_kind"]
        if required:
            output = connection.execute(
                "SELECT * FROM task_outputs WHERE task_id = ? ORDER BY ordinal DESC LIMIT 1",
                (task_id,),
            ).fetchone()
            valid = output is not None and output["output_kind"] == required
            if valid and task["operation"] == "reviewPlanningSeat":
                from .planning_reviews import valid_completed_report

                valid = valid_completed_report(connection, task, output)
            if valid and required == "draft":
                draft = connection.execute(
                    "SELECT chapter_id, body FROM draft_revisions WHERE id = ?",
                    (output["revision_id"],),
                ).fetchone()
                valid = bool(
                    draft
                    and draft["body"].strip()
                    and output["completeness"] == "chapterComplete"
                    and (
                        not target.get("chapterId")
                        or draft["chapter_id"] == target["chapterId"]
                    )
                )
            if valid and task["operation"] != "reviewPlanningSeat":
                if required == "draft":
                    current = connection.execute(
                        "SELECT candidate_revision_id FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                        (project_id, draft["chapter_id"]),
                    ).fetchone()
                else:
                    current = connection.execute(
                        "SELECT candidate_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
                        (project_id, required),
                    ).fetchone()
                if (
                    current is None
                    or current["candidate_revision_id"] != output["revision_id"]
                ):
                    valid = False
                    failure_code = "required_output_superseded"
            if not valid:
                connection.execute(
                    "UPDATE task_runs SET status = 'failed', stage = 'failed', delivery_status = 'incomplete', "
                    "error = ?, result_summary = ?, capability_revoked_at = ?, updated_at = ? WHERE id = ?",
                    (failure_code, summary[:4000], now(), now(), task_id),
                )
                missing_required_output = True
            else:
                refs = [
                    {
                        "kind": required,
                        "revisionId": output["revision_id"],
                        **(
                            {"chapterId": draft["chapter_id"]}
                            if required == "draft"
                            else {}
                        ),
                    }
                ]
        if not missing_required_output:
            connection.execute(
                "UPDATE task_runs SET status = 'completed', stage = 'ready_for_review', "
                "delivery_status = 'ready_for_review', output_refs_json = ?, result_summary = ?, error = NULL, capability_revoked_at = ?, updated_at = ? "
                "WHERE id = ?",
                (_json(refs[-100:]), summary[:4000], now(), now(), task_id),
            )
    if missing_required_output:
        raise WorkspaceError(
            failure_code,
            "模型回合结束，但任务候选已被更新版本替换"
            if failure_code == "required_output_superseded"
            else "模型回合结束，但没有保存目标章节的有效草稿"
            if task["required_output_kind"] == "draft"
            else "模型回合结束，但没有保存目标产物的有效候选",
            422,
            {
                "target": target,
                "requiredOutputKind": task["required_output_kind"]
                if "task" in locals()
                else None,
            },
            "基于当前候选重新开始生成"
            if failure_code == "required_output_superseded"
            else "保存任务要求的完整候选后重试完成",
        )
    return {
        "id": task_id,
        "status": "completed",
        "stage": "ready_for_review",
        "deliveryStatus": "ready_for_review",
        "outputRefs": refs,
    }
