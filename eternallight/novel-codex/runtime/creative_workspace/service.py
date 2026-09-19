import hashlib
import json
from pathlib import Path
from typing import Any
from uuid import uuid4

from .errors import ConflictError, NotFoundError, WorkspaceError
from .context_limits import fit_context_snapshot
from .generation import (
    assert_draft_target,
    assert_task_output,
    assert_task_purpose,
    cancel_task,
    complete_task,
    create_task,
    record_task_output,
    recover_orphaned_tasks,
    require_active_task,
    task_target,
    update_task,
)
from .planning import (
    assert_outline_protections,
    normalize_outline,
    normalize_plan,
    validate_outline,
)
from .planning_reviews import list_planning_reviews
from .impacts import (
    advance_planning_impacts,
    assert_open_impact_targets_preserved,
    link_planning_impacts,
    list_open_planning_impacts,
    preview_planning_impacts,
    record_planning_impacts,
)
from .review_decisions import validate_draft_acceptance
from .working_baselines import (
    list_continuation_intents,
    list_working_baselines,
    resolve_draft_revision,
    resolve_planning_revision,
)

try:
    from ..storage import (
        connect,
        initialize,
        now,
        refresh_search_index,
        run_story_tests,
    )
except ImportError:
    from storage import connect, initialize, now, refresh_search_index, run_story_tests


ARTIFACT_KINDS = {"brief", "outline", "plan"}
MAX_BODY = 120_000
SCENE_SEPARATOR = "\n\n---\n\n"


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _load(value: str | None, fallback: Any) -> Any:
    if not value:
        return fallback
    return json.loads(value)


def _id(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex}"


def _candidate_write_owner(actor: str, task_id: str | None) -> str:
    return f"task:{task_id}" if actor == "candidate" and task_id else "author"


def _project(connection, project_id: str):
    row = connection.execute(
        "SELECT id, title, logline FROM projects WHERE id = ?", (project_id,)
    ).fetchone()
    if row is None:
        raise NotFoundError("作品不存在", {"projectId": project_id})
    return row


def _workspace(connection, project_id: str):
    _project(connection, project_id)
    row = connection.execute(
        "SELECT * FROM project_workspace WHERE project_id = ?", (project_id,)
    ).fetchone()
    if row is None:
        connection.execute(
            "INSERT INTO project_workspace(project_id) VALUES (?)", (project_id,)
        )
        row = connection.execute(
            "SELECT * FROM project_workspace WHERE project_id = ?", (project_id,)
        ).fetchone()
    for kind in ARTIFACT_KINDS:
        connection.execute(
            "INSERT OR IGNORE INTO artifact_pointers(project_id, artifact_kind) VALUES (?, ?)",
            (project_id, kind),
        )
    return row


def _source_for_actor(actor: str) -> str:
    return "agent" if actor == "candidate" else actor


def _require_task(
    connection, project_id: str, task_id: str | None, thread_id: str | None
) -> Any:
    return require_active_task(connection, project_id, task_id, thread_id)


def _assert_actor(
    connection, project_id: str, actor: str, task_id: str | None, thread_id: str | None
) -> None:
    if actor == "candidate":
        _require_task(connection, project_id, task_id, thread_id)
    elif actor != "author":
        raise WorkspaceError("invalid_actor", "未知写入身份", 401)


def _artifact_row(connection, project_id: str, kind: str, revision_id: str):
    row = connection.execute(
        "SELECT * FROM artifact_revisions WHERE id = ? AND project_id = ? AND artifact_kind = ?",
        (revision_id, project_id, kind),
    ).fetchone()
    if row is None:
        raise NotFoundError(
            "创作产物修订不存在", {"artifactKind": kind, "revisionId": revision_id}
        )
    return row


def _draft_row(connection, project_id: str, revision_id: str):
    row = connection.execute(
        "SELECT * FROM draft_revisions WHERE id = ? AND project_id = ?",
        (revision_id, project_id),
    ).fetchone()
    if row is None:
        raise NotFoundError("草稿修订不存在", {"revisionId": revision_id})
    return row


def _text_field(
    payload: dict[str, Any], key: str, *, required: bool = False, limit: int = 12_000
) -> str:
    value = payload.get(key, "")
    if (
        not isinstance(value, str)
        or len(value) > limit
        or (required and not value.strip())
    ):
        raise WorkspaceError("invalid_artifact_contract", f"{key} 字段无效")
    return value


def _validate_artifact_payload(kind: str, payload: dict[str, Any]) -> None:
    if kind == "brief":
        allowed = {"premise", "promise", "stakes", "endingDirection"}
        _text_field(payload, "premise", required=True)
        for key in allowed - {"premise"}:
            _text_field(payload, key)
    elif kind == "outline":
        allowed = {"nodes"}
        validate_outline(payload)
    else:
        if kind == "plan":
            allowed = {"sourceOutlineRevisionId", "chapters", "window"}
            chapters = payload.get("chapters")
            if not isinstance(chapters, list) or not 1 <= len(chapters) <= 5:
                raise WorkspaceError("invalid_plan_contract", "近期计划必须包含 1–5 章")
            seen_ids: set[str] = set()
            seen_numbers: set[int] = set()
            for chapter in chapters:
                if not isinstance(chapter, dict):
                    raise WorkspaceError("invalid_plan_contract", "章节计划必须是对象")
                chapter_id = chapter.get("chapterPlanId")
                number = chapter.get("chapterNumber")
                if (
                    not isinstance(chapter_id, str)
                    or not chapter_id
                    or chapter_id in seen_ids
                ):
                    raise WorkspaceError(
                        "invalid_plan_contract", "章纲必须有唯一稳定 ID"
                    )
                if (
                    isinstance(number, bool)
                    or not isinstance(number, int)
                    or number < 1
                    or number in seen_numbers
                ):
                    raise WorkspaceError(
                        "invalid_plan_contract", "章节序号必须是唯一正整数"
                    )
                seen_ids.add(chapter_id)
                seen_numbers.add(number)
                if not isinstance(chapter.get("primaryNodeId"), str):
                    raise WorkspaceError(
                        "invalid_plan_contract", "章纲必须关联一个大纲节点"
                    )
                chapter_allowed = {
                    "chapterPlanId",
                    "primaryNodeId",
                    "chapterNumber",
                    "title",
                    "publicTitle",
                    "goal",
                    "conflict",
                    "writerGuidance",
                    "dependencyKeys",
                    "publicCommitments",
                    "allowances",
                    "scenes",
                    "locked",
                    "povCharacterId",
                    "storyTime",
                }
                if set(chapter) - chapter_allowed:
                    raise WorkspaceError(
                        "invalid_plan_contract",
                        "章节计划包含未知字段",
                        details={"fields": sorted(set(chapter) - chapter_allowed)},
                    )
                if "locked" in chapter and not isinstance(chapter["locked"], bool):
                    raise WorkspaceError(
                        "invalid_plan_contract", "章节锁定状态必须是布尔值"
                    )
                story_time = chapter.get("storyTime")
                if story_time is not None and (
                    isinstance(story_time, bool)
                    or not isinstance(story_time, int)
                    or story_time < 0
                ):
                    raise WorkspaceError(
                        "invalid_plan_contract", "章节故事时间必须是非负整数或 null"
                    )
                for key in (
                    "title",
                    "publicTitle",
                    "goal",
                    "conflict",
                    "writerGuidance",
                ):
                    _text_field(chapter, key, limit=4_000)
                for key in ("dependencyKeys", "allowances", "publicCommitments"):
                    values = chapter.get(key, [])
                    if (
                        not isinstance(values, list)
                        or len(values) > 100
                        or not all(
                            isinstance(item, str)
                            and item.strip()
                            and len(item) <= 2_000
                            for item in values
                        )
                    ):
                        raise WorkspaceError(
                            "invalid_plan_contract", f"{key} 必须是有效字符串数组"
                        )
                scenes = chapter.get("scenes", [])
                if not isinstance(scenes, list) or len(scenes) > 30:
                    raise WorkspaceError("invalid_plan_contract", "场景计划无效")
                seen_scene_ids: set[str] = set()
                seen_beat_ids: set[str] = set()
                for scene in scenes:
                    if not isinstance(scene, dict) or set(scene) - {
                        "id",
                        "title",
                        "goal",
                        "conflict",
                        "beats",
                        "locked",
                    }:
                        raise WorkspaceError(
                            "invalid_plan_contract", "场景计划字段无效"
                        )
                    scene_id = scene.get("id")
                    if (
                        not isinstance(scene_id, str)
                        or not scene_id.strip()
                        or scene_id in seen_scene_ids
                    ):
                        raise WorkspaceError(
                            "invalid_plan_contract", "每个场景计划必须有唯一稳定 ID"
                        )
                    seen_scene_ids.add(scene_id)
                    _text_field(scene, "goal", required=True, limit=4_000)
                    for key in ("title", "conflict"):
                        _text_field(scene, key, limit=4_000)
                    if "locked" in scene and not isinstance(scene["locked"], bool):
                        raise WorkspaceError(
                            "invalid_plan_contract", "场景锁定状态必须是布尔值"
                        )
                    beats = scene.get("beats", [])
                    if not isinstance(beats, list) or len(beats) > 40:
                        raise WorkspaceError(
                            "invalid_plan_contract", "场景 Beat 必须是数组"
                        )
                    for beat in beats:
                        if not isinstance(beat, dict) or set(beat) - {
                            "id",
                            "description",
                            "locked",
                        }:
                            raise WorkspaceError(
                                "invalid_plan_contract",
                                "Beat 必须是 id/description/locked 对象",
                            )
                        beat_id = beat.get("id")
                        if (
                            not isinstance(beat_id, str)
                            or not beat_id.strip()
                            or beat_id in seen_beat_ids
                        ):
                            raise WorkspaceError(
                                "invalid_plan_contract", "Beat 必须有唯一稳定 ID"
                            )
                        seen_beat_ids.add(beat_id)
                        _text_field(beat, "description", required=True, limit=2_000)
                        if "locked" in beat and not isinstance(beat["locked"], bool):
                            raise WorkspaceError(
                                "invalid_plan_contract", "Beat 锁定状态必须是布尔值"
                            )
            unknown = set(payload) - allowed
            if unknown:
                raise WorkspaceError(
                    "invalid_artifact_contract",
                    "创作产物包含未知字段",
                    details={"fields": sorted(unknown)},
                )
            return
    unknown = set(payload) - allowed
    if unknown:
        raise WorkspaceError(
            "invalid_artifact_contract",
            "创作产物包含未知字段",
            details={"fields": sorted(unknown)},
        )


def _locked_node_unchanged(
    previous: dict[str, Any], candidate: dict[str, Any] | None, actor: str, label: str
) -> None:
    if candidate == previous:
        return
    if (
        actor == "author"
        and candidate is not None
        and previous.get("locked") is True
        and candidate.get("locked") is False
    ):
        previous_without_lock = {
            key: value for key, value in previous.items() if key != "locked"
        }
        candidate_without_lock = {
            key: value for key, value in candidate.items() if key != "locked"
        }
        if previous_without_lock == candidate_without_lock:
            return
    raise ConflictError(
        "locked_plan_node_changed",
        f"锁定的{label}不能被修改、删除或移动",
        recovery="先单独解锁该节点，再创建新的修订候选",
    )


def _assert_plan_locks(
    previous_payload: dict[str, Any], candidate_payload: dict[str, Any], actor: str
) -> None:
    previous_chapters = {
        chapter.get("chapterPlanId"): (index, chapter)
        for index, chapter in enumerate(previous_payload.get("chapters", []))
        if isinstance(chapter, dict) and chapter.get("chapterPlanId")
    }
    candidate_chapters = {
        chapter.get("chapterPlanId"): (index, chapter)
        for index, chapter in enumerate(candidate_payload.get("chapters", []))
        if isinstance(chapter, dict) and chapter.get("chapterPlanId")
    }
    for chapter_id, (
        previous_chapter_index,
        previous_chapter,
    ) in previous_chapters.items():
        candidate_entry = candidate_chapters.get(chapter_id)
        candidate_chapter_index, candidate_chapter = (
            candidate_entry if candidate_entry is not None else (-1, None)
        )
        if previous_chapter.get("locked") is True:
            if candidate_chapter_index != previous_chapter_index:
                raise ConflictError(
                    "locked_plan_node_changed",
                    "锁定的章节计划不能被移动",
                    recovery="先单独解锁该节点，再创建新的修订候选",
                )
            _locked_node_unchanged(
                previous_chapter, candidate_chapter, actor, "章节计划"
            )
            continue
        previous_scenes = {
            scene.get("id"): (index, scene)
            for index, scene in enumerate(previous_chapter.get("scenes", []))
            if isinstance(scene, dict) and scene.get("id")
        }
        candidate_scenes = {
            scene.get("id"): (index, scene)
            for index, scene in enumerate(
                candidate_chapter.get("scenes", []) if candidate_chapter else []
            )
            if isinstance(scene, dict) and scene.get("id")
        }
        for scene_id, (previous_scene_index, previous_scene) in previous_scenes.items():
            candidate_scene_entry = candidate_scenes.get(scene_id)
            candidate_scene_index, candidate_scene = (
                candidate_scene_entry
                if candidate_scene_entry is not None
                else (-1, None)
            )
            if previous_scene.get("locked") is True:
                if (
                    candidate_chapter_index != previous_chapter_index
                    or candidate_scene_index != previous_scene_index
                ):
                    raise ConflictError(
                        "locked_plan_node_changed",
                        "锁定的场景计划不能被移动",
                        recovery="先单独解锁该节点，再创建新的修订候选",
                    )
                _locked_node_unchanged(
                    previous_scene, candidate_scene, actor, "场景计划"
                )
                continue
            previous_beats = {
                beat.get("id"): (index, beat)
                for index, beat in enumerate(previous_scene.get("beats", []))
                if isinstance(beat, dict) and beat.get("id")
            }
            candidate_beats = {
                beat.get("id"): (index, beat)
                for index, beat in enumerate(
                    candidate_scene.get("beats", []) if candidate_scene else []
                )
                if isinstance(beat, dict) and beat.get("id")
            }
            for beat_id, (previous_beat_index, previous_beat) in previous_beats.items():
                if previous_beat.get("locked") is True:
                    candidate_beat_entry = candidate_beats.get(beat_id)
                    candidate_beat_index, candidate_beat = (
                        candidate_beat_entry
                        if candidate_beat_entry is not None
                        else (-1, None)
                    )
                    if (
                        candidate_chapter_index != previous_chapter_index
                        or candidate_scene_index != previous_scene_index
                        or candidate_beat_index != previous_beat_index
                    ):
                        raise ConflictError(
                            "locked_plan_node_changed",
                            "锁定的 Beat 不能被移动",
                            recovery="先单独解锁该节点，再创建新的修订候选",
                        )
                    _locked_node_unchanged(previous_beat, candidate_beat, actor, "Beat")


def _scene_group_content(group: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": group.get("id"),
        "sceneId": group.get("sceneId"),
        "body": group.get("body"),
        "locked": group.get("locked", False),
    }


def _assert_scene_group_locks(
    previous_groups: list[dict[str, Any]],
    candidate_groups: list[dict[str, Any]],
    actor: str,
) -> None:
    candidates = {
        group.get("id"): (index, group)
        for index, group in enumerate(candidate_groups)
        if isinstance(group, dict) and group.get("id")
    }
    for previous_index, previous in enumerate(previous_groups):
        if not isinstance(previous, dict) or previous.get("locked") is not True:
            continue
        candidate_entry = candidates.get(previous.get("id"))
        candidate_index, candidate = (
            candidate_entry if candidate_entry is not None else (-1, None)
        )
        if candidate_index != previous_index:
            raise ConflictError(
                "locked_plan_node_changed",
                "锁定的正文场景不能被移动",
                recovery="先单独解锁该节点，再创建新的修订候选",
            )
        _locked_node_unchanged(
            _scene_group_content(previous),
            _scene_group_content(candidate) if candidate is not None else None,
            actor,
            "正文场景",
        )


def _revision_payload(row) -> dict[str, Any] | None:
    if row is None:
        return None
    return {
        "id": row["id"],
        "version": row["version"],
        "parentRevisionId": row["parent_revision_id"],
        "baseProjectRevision": row["base_project_revision"],
        "payload": _load(row["payload_json"], {}),
        "source": row["source"],
        "createdAt": row["created_at"],
    }


def _draft_payload(row) -> dict[str, Any] | None:
    if row is None:
        return None
    return {
        "id": row["id"],
        "chapterId": row["chapter_id"],
        "version": row["version"],
        "parentRevisionId": row["parent_revision_id"],
        "sourcePlanRefs": _load(row["source_plan_refs_json"], {}),
        "baseProjectRevision": row["base_project_revision"],
        "body": row["body"],
        "sceneGroups": _load(row["scene_groups_json"], []),
        "source": row["source"],
        "createdAt": row["created_at"],
    }


def workspace_state(db_path: str | Path, project_id: str) -> dict[str, Any]:
    initialize(db_path)
    with connect(db_path) as connection:
        project = _project(connection, project_id)
        workspace = _workspace(connection, project_id)
        working_baselines = list_working_baselines(connection, project_id)
        continuation_intents = list_continuation_intents(connection, project_id)
        working_by_object = {
            (item["objectKind"], item["objectId"]): item for item in working_baselines
        }
        artifacts: dict[str, Any] = {}
        for kind in sorted(ARTIFACT_KINDS):
            working_selection = working_by_object.get((kind, kind))
            pointer = connection.execute(
                "SELECT * FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
                (project_id, kind),
            ).fetchone()
            candidate = (
                _artifact_row(
                    connection, project_id, kind, pointer["candidate_revision_id"]
                )
                if pointer and pointer["candidate_revision_id"]
                else None
            )
            accepted = (
                _artifact_row(
                    connection, project_id, kind, pointer["accepted_revision_id"]
                )
                if pointer and pointer["accepted_revision_id"]
                else None
            )
            working = (
                _artifact_row(
                    connection, project_id, kind, working_selection["revisionId"]
                )
                if working_selection and working_selection["revisionId"]
                else None
            )
            artifacts[kind] = {
                "candidate": _revision_payload(candidate),
                "accepted": _revision_payload(accepted),
                "working": _revision_payload(working),
                "locked": bool(pointer["locked"]) if pointer else False,
                "workingSelection": working_selection,
            }
        accepted_outline = artifacts["outline"]["accepted"]
        accepted_plan = artifacts["plan"]["accepted"]
        if accepted_plan is None:
            plan_lineage_status = "missing"
        elif (
            accepted_outline
            and accepted_plan["payload"].get("sourceOutlineRevisionId")
            == accepted_outline["id"]
        ):
            plan_lineage_status = "current"
        else:
            plan_lineage_status = "stale"
        chapters = []
        for chapter in connection.execute(
            "SELECT id, number, title, status, summary, updated_at FROM chapters WHERE project_id = ? ORDER BY number",
            (project_id,),
        ).fetchall():
            working_selection = working_by_object.get(("draft", chapter["id"]))
            pointer = connection.execute(
                "SELECT * FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                (project_id, chapter["id"]),
            ).fetchone()
            candidate = (
                _draft_row(connection, project_id, pointer["candidate_revision_id"])
                if pointer and pointer["candidate_revision_id"]
                else None
            )
            accepted = (
                _draft_row(connection, project_id, pointer["accepted_revision_id"])
                if pointer and pointer["accepted_revision_id"]
                else None
            )
            working = (
                _draft_row(connection, project_id, working_selection["revisionId"])
                if working_selection and working_selection["revisionId"]
                else None
            )
            chapters.append(
                {
                    **dict(chapter),
                    "candidate": _draft_payload(candidate),
                    "accepted": _draft_payload(accepted),
                    "working": _draft_payload(working),
                    "workingSelection": working_selection,
                }
            )
        proposals = [
            {**_fact_payload(row), "draft_revision_id": row["draft_revision_id"]}
            for row in connection.execute(
                "SELECT * FROM fact_proposals WHERE project_id = ? ORDER BY created_at DESC LIMIT 100",
                (project_id,),
            ).fetchall()
        ]
        impacts = list_open_planning_impacts(connection, project_id)
        impact_count = connection.execute(
            "SELECT COUNT(*) FROM planning_impacts WHERE project_id = ? AND state = 'open'",
            (project_id,),
        ).fetchone()[0]
        bindings = []
        for row in connection.execute(
            "SELECT * FROM chapter_plan_bindings WHERE project_id = ?", (project_id,)
        ).fetchall():
            binding = dict(row)
            plan_row = connection.execute(
                "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ? AND artifact_kind = 'plan'",
                (row["plan_revision_id"], project_id),
            ).fetchone()
            plan_payload = _load(plan_row["payload_json"], {}) if plan_row else {}
            plan_entry = next(
                (
                    entry
                    for entry in plan_payload.get("chapters", [])
                    if entry.get("chapterPlanId") == row["chapter_plan_id"]
                ),
                None,
            )
            binding["lineageStatus"] = (
                "current"
                if accepted_plan and accepted_plan["id"] == row["plan_revision_id"]
                else "historical"
            )
            binding["writerProjection"] = (
                None
                if plan_entry is None
                else {
                    "chapterPlanId": plan_entry["chapterPlanId"],
                    "primaryNodeId": plan_entry.get("primaryNodeId"),
                    "chapterNumber": plan_entry.get("chapterNumber"),
                    "publicTitle": plan_entry.get("publicTitle", ""),
                    "writerGuidance": plan_entry.get("writerGuidance", ""),
                    "publicCommitments": plan_entry.get("publicCommitments", []),
                    "povCharacterId": plan_entry.get("povCharacterId"),
                    "storyTime": plan_entry.get("storyTime"),
                }
            )
            bindings.append(binding)
        commits = [
            {**dict(row), "receipt": _load(row["receipt_json"], {})}
            for row in connection.execute(
                "SELECT * FROM acceptance_commits WHERE project_id = ? ORDER BY result_revision DESC LIMIT 30",
                (project_id,),
            ).fetchall()
        ]
        tasks = [
            {
                **{
                    key: value
                    for key, value in dict(row).items()
                    if key
                    not in {
                        "capability_hash",
                        "capability_expires_at",
                        "capability_revoked_at",
                    }
                },
                "target": _load(row["target_json"], {}),
                "outputRefs": _load(row["output_refs_json"], []),
            }
            for row in connection.execute(
                "SELECT * FROM task_runs WHERE project_id = ? ORDER BY updated_at DESC LIMIT 30",
                (project_id,),
            ).fetchall()
        ]
        reviews = [
            {
                **dict(row),
                "checks": _load(row["checks_json"], []),
                "selections": _load(row["selections_json"], {}),
            }
            for row in connection.execute(
                "SELECT * FROM review_bundles WHERE project_id = ? ORDER BY created_at DESC LIMIT 30",
                (project_id,),
            ).fetchall()
        ]
        return {
            "project": dict(project),
            "revision": workspace["revision"],
            "planWindow": workspace["plan_window"],
            "planLineageStatus": plan_lineage_status,
            "index": {
                "targetRevision": workspace["index_target_revision"],
                "indexedRevision": workspace["indexed_revision"],
                "status": "ready"
                if workspace["indexed_revision"] >= workspace["index_target_revision"]
                else "stale",
            },
            "artifacts": artifacts,
            "chapters": chapters,
            "factProposals": proposals,
            "reviews": reviews,
            "impacts": impacts,
            "openImpactCount": impact_count,
            "chapterPlanBindings": bindings,
            "commits": commits,
            "tasks": tasks,
            "workingBaselines": working_baselines,
            "continuationIntents": continuation_intents,
            "planningReviews": list_planning_reviews(connection, project_id),
        }


def save_artifact(
    db_path: str | Path,
    project_id: str,
    kind: str,
    payload: dict[str, Any],
    expected_revision: int,
    actor: str = "author",
    task_id: str | None = None,
    thread_id: str | None = None,
    expected_candidate_revision_id: str | None = None,
    idempotency_key: str | None = None,
    enforce_candidate_cas: bool = False,
    linked_impact_ids: list[str] | None = None,
) -> dict[str, Any]:
    if kind not in ARTIFACT_KINDS or not isinstance(payload, dict):
        raise WorkspaceError("invalid_artifact", "创作产物类型或内容无效")
    digest = _request_hash(
        {
            "kind": kind,
            "payload": payload,
            "expectedRevision": expected_revision,
            "expectedCandidateRevisionId": expected_candidate_revision_id,
            "linkedImpactIds": linked_impact_ids or [],
        }
    )
    initialize(db_path)
    if kind == "outline":
        payload = normalize_outline(payload, _id)
    revision_id = _id(kind)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        _assert_actor(connection, project_id, actor, task_id, thread_id)
        owner_key = _candidate_write_owner(actor, task_id)
        if idempotency_key:
            existing = connection.execute(
                "SELECT request_hash, result_json FROM candidate_write_requests WHERE project_id = ? AND owner_key = ? AND idempotency_key = ?",
                (project_id, owner_key, idempotency_key),
            ).fetchone()
            if existing:
                if existing["request_hash"] != digest:
                    raise ConflictError(
                        "idempotency_conflict", "幂等键已用于不同候选写入"
                    )
                return _load(existing["result_json"], {})
        if actor == "candidate":
            assert_task_output(
                _require_task(connection, project_id, task_id, thread_id), kind
            )
        workspace = _workspace(connection, project_id)
        if workspace["revision"] != expected_revision:
            raise ConflictError(
                "stale_project_revision",
                "作品版本已变化",
                {"currentRevision": workspace["revision"]},
                "重新载入后再保存",
            )
        pointer = connection.execute(
            "SELECT * FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
            (project_id, kind),
        ).fetchone()
        current_candidate = pointer["candidate_revision_id"] if pointer else None
        if (
            enforce_candidate_cas or expected_candidate_revision_id is not None
        ) and current_candidate != expected_candidate_revision_id:
            raise ConflictError(
                "stale_candidate_revision",
                "候选版本已变化",
                {"currentCandidateRevisionId": current_candidate},
                "比较并复制为最新版候选",
            )
        if kind == "plan":
            if actor == "candidate":
                task = _require_task(connection, project_id, task_id, thread_id)
                baseline = _load(task["input_manifest_json"], {}).get(
                    "planningBaseline", {}
                )
                outline_id = baseline.get("outlineRevisionId")
            else:
                outline_id, _ = resolve_planning_revision(
                    connection, project_id, "outline"
                )
            if not outline_id:
                raise ConflictError(
                    "planning_baseline_required", "请先选用或采用层级大纲，再生成章纲"
                )
            outline_row = _artifact_row(connection, project_id, "outline", outline_id)
            outline_payload = _load(outline_row["payload_json"], {})
            payload = {**payload, "sourceOutlineRevisionId": outline_id}
            payload = normalize_plan(payload, outline_id, outline_payload, _id)
            for chapter_plan in payload["chapters"]:
                pov_character_id = chapter_plan.get("povCharacterId")
                if pov_character_id is not None:
                    if not isinstance(pov_character_id, str) or not pov_character_id:
                        raise WorkspaceError(
                            "invalid_plan_contract",
                            "章纲视角人物必须是本作品角色或 null",
                        )
                    character = connection.execute(
                        "SELECT 1 FROM characters WHERE id = ? AND project_id = ?",
                        (pov_character_id, project_id),
                    ).fetchone()
                    if character is None:
                        raise WorkspaceError(
                            "invalid_plan_contract",
                            "章纲视角人物必须是本作品角色或 null",
                        )
        _validate_artifact_payload(kind, payload)
        encoded = _json(payload)
        if len(encoded.encode("utf-8")) > 48 * 1024:
            raise WorkspaceError("artifact_too_large", "创作产物超过 48 KiB", 413)
        if pointer and pointer["locked"]:
            raise ConflictError(
                "artifact_locked", "该产物已锁定", {"artifactKind": kind}
            )
        if kind == "outline" and pointer and pointer["accepted_revision_id"]:
            accepted = _artifact_row(
                connection, project_id, kind, pointer["accepted_revision_id"]
            )
            assert_outline_protections(
                _load(accepted["payload_json"], {}), payload, actor
            )
        if kind == "plan" and pointer and pointer["accepted_revision_id"]:
            accepted = _artifact_row(
                connection, project_id, kind, pointer["accepted_revision_id"]
            )
            _assert_plan_locks(_load(accepted["payload_json"], {}), payload, actor)
        version = connection.execute(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM artifact_revisions WHERE project_id = ? AND artifact_kind = ?",
            (project_id, kind),
        ).fetchone()[0]
        parent = pointer["candidate_revision_id"] if pointer else None
        connection.execute(
            "INSERT INTO artifact_revisions(id, project_id, artifact_kind, version, base_project_revision, parent_revision_id, payload_json, source, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                revision_id,
                project_id,
                kind,
                version,
                expected_revision,
                parent,
                encoded,
                _source_for_actor(actor),
                now(),
            ),
        )
        connection.execute(
            "UPDATE artifact_pointers SET candidate_revision_id = ? WHERE project_id = ? AND artifact_kind = ?",
            (revision_id, project_id, kind),
        )
        if linked_impact_ids:
            if actor != "author":
                raise WorkspaceError(
                    "impact_link_forbidden", "只有作者可以把反馈链接到规划候选", 403
                )
            linked_impact_ids = link_planning_impacts(
                connection, project_id, kind, revision_id, payload, linked_impact_ids
            )
        if task_id:
            task = connection.execute(
                "SELECT output_refs_json FROM task_runs WHERE id = ?", (task_id,)
            ).fetchone()
            refs = _load(task["output_refs_json"], []) if task else []
            refs.append({"kind": kind, "revisionId": revision_id})
            connection.execute(
                "UPDATE task_runs SET status = 'running', output_refs_json = ?, candidate_head_revision_id = ?, updated_at = ? WHERE id = ?",
                (_json(refs[-100:]), revision_id, now(), task_id),
            )
            record_task_output(connection, task_id, kind, revision_id)
        result = {
            "artifactKind": kind,
            "revisionId": revision_id,
            "version": version,
            "payload": payload,
            "linkedImpactIds": linked_impact_ids or [],
        }
        if idempotency_key:
            connection.execute(
                "INSERT INTO candidate_write_requests(project_id, owner_key, idempotency_key, request_hash, result_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                (project_id, owner_key, idempotency_key, digest, _json(result), now()),
            )
    return result


def set_artifact_lock(
    db_path: str | Path,
    project_id: str,
    kind: str,
    locked: bool,
    expected_revision: int,
) -> dict[str, Any]:
    if kind not in ARTIFACT_KINDS or not isinstance(locked, bool):
        raise WorkspaceError("invalid_artifact_lock", "产物锁定参数无效")
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        workspace = _workspace(connection, project_id)
        if workspace["revision"] != expected_revision:
            raise ConflictError(
                "stale_project_revision",
                "作品版本已变化",
                {"currentRevision": workspace["revision"]},
                "重新载入后再操作",
            )
        pointer = connection.execute(
            "SELECT * FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
            (project_id, kind),
        ).fetchone()
        if pointer is None or pointer["accepted_revision_id"] is None:
            raise WorkspaceError("artifact_not_accepted", "只有正式产物可以锁定")
        if bool(pointer["locked"]) == locked:
            return {
                "artifactKind": kind,
                "locked": locked,
                "resultRevision": expected_revision,
            }
        result_revision = expected_revision + 1
        connection.execute(
            "UPDATE artifact_pointers SET locked = ? WHERE project_id = ? AND artifact_kind = ?",
            (int(locked), project_id, kind),
        )
        connection.execute(
            "UPDATE review_bundles SET status = 'stale' WHERE project_id = ? AND status = 'ready'",
            (project_id,),
        )
        indexed_revision = (
            result_revision
            if workspace["indexed_revision"] >= workspace["index_target_revision"]
            else workspace["indexed_revision"]
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ?, indexed_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, indexed_revision, project_id),
        )
        connection.execute(
            "UPDATE projects SET updated_at = ? WHERE id = ?", (now(), project_id)
        )
    return {"artifactKind": kind, "locked": locked, "resultRevision": result_revision}


def _ensure_chapter(
    connection,
    project_id: str,
    chapter_id: str | None,
    number: int | None,
    title: str | None,
) -> Any:
    if chapter_id:
        chapter = connection.execute(
            "SELECT * FROM chapters WHERE id = ? AND project_id = ?",
            (chapter_id, project_id),
        ).fetchone()
        if chapter is None:
            raise NotFoundError("章节不存在", {"chapterId": chapter_id})
        return chapter
    next_number = (
        number
        or connection.execute(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM chapters WHERE project_id = ?",
            (project_id,),
        ).fetchone()[0]
    )
    if next_number < 1:
        raise WorkspaceError("invalid_chapter_number", "章节序号无效")
    chapter_id = _id("chapter")
    connection.execute(
        "INSERT INTO chapters(id, project_id, number, title, status, summary, updated_at) VALUES (?, ?, ?, ?, 'draft', '', ?)",
        (
            chapter_id,
            project_id,
            next_number,
            (title or f"第 {next_number} 章")[:160],
            now(),
        ),
    )
    connection.execute(
        "INSERT INTO chapter_workspace(project_id, chapter_id) VALUES (?, ?)",
        (project_id, chapter_id),
    )
    return connection.execute(
        "SELECT * FROM chapters WHERE id = ?", (chapter_id,)
    ).fetchone()


def save_draft_checkpoint(
    db_path: str | Path,
    project_id: str,
    payload: dict[str, Any],
    expected_revision: int,
    actor: str = "author",
    task_id: str | None = None,
    thread_id: str | None = None,
    expected_candidate_revision_id: str | None = None,
    idempotency_key: str | None = None,
    enforce_candidate_cas: bool = False,
) -> dict[str, Any]:
    body = payload.get("body", "")
    if not isinstance(body, str) or len(body) > MAX_BODY:
        raise WorkspaceError("draft_too_large", "章节草稿超过长度限制", 413)
    groups = payload.get("sceneGroups") or []
    if not isinstance(groups, list) or len(groups) > 100:
        raise WorkspaceError("invalid_scene_groups", "场景变更组无效")
    completeness = payload.get("completeness", "chapterComplete")
    if completeness not in {"partial", "sceneComplete", "chapterComplete"}:
        raise WorkspaceError("invalid_checkpoint_completeness", "草稿检查点完整度无效")
    digest = _request_hash(
        {
            "kind": "draft",
            "payload": payload,
            "expectedRevision": expected_revision,
            "expectedCandidateRevisionId": expected_candidate_revision_id,
        }
    )
    initialize(db_path)
    revision_id = _id("draft")
    checkpoint_id = _id("checkpoint")
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        _assert_actor(connection, project_id, actor, task_id, thread_id)
        owner_key = _candidate_write_owner(actor, task_id)
        if idempotency_key:
            existing = connection.execute(
                "SELECT request_hash, result_json FROM candidate_write_requests WHERE project_id = ? AND owner_key = ? AND idempotency_key = ?",
                (project_id, owner_key, idempotency_key),
            ).fetchone()
            if existing:
                if existing["request_hash"] != digest:
                    raise ConflictError(
                        "idempotency_conflict", "幂等键已用于不同候选写入"
                    )
                return _load(existing["result_json"], {})
        workspace = _workspace(connection, project_id)
        if workspace["revision"] != expected_revision:
            raise ConflictError(
                "stale_project_revision",
                "作品版本已变化",
                {"currentRevision": workspace["revision"]},
                "重新载入并基于最新版本保存",
            )
        chapter = _ensure_chapter(
            connection,
            project_id,
            payload.get("chapterId"),
            payload.get("chapterNumber"),
            payload.get("title"),
        )
        authorized_scene_order = None
        if actor == "candidate":
            task = _require_task(connection, project_id, task_id, thread_id)
            assert_task_output(task, "draft")
            assert_draft_target(task, chapter["id"])
            target = task_target(task)
            target_scope = target.get("scope", {})
            checkpoint_scene_order = payload.get("sceneOrder")
            if target_scope.get("type") == "throughScene":
                if (
                    isinstance(checkpoint_scene_order, bool)
                    or not isinstance(checkpoint_scene_order, int)
                    or checkpoint_scene_order < 0
                    or checkpoint_scene_order > target["sceneOrder"]
                ):
                    raise ConflictError(
                        "checkpoint_scope_exceeded",
                        "草稿检查点超出任务授权的场景范围",
                        {
                            "maximumSceneOrder": target["sceneOrder"],
                            "checkpointSceneOrder": checkpoint_scene_order,
                        },
                        "仅保存目标场景范围内的检查点",
                    )
                authorized_scene_order = target["sceneOrder"]
            elif checkpoint_scene_order is not None and (
                isinstance(checkpoint_scene_order, bool)
                or not isinstance(checkpoint_scene_order, int)
                or checkpoint_scene_order < 0
            ):
                raise WorkspaceError(
                    "invalid_checkpoint_scope", "草稿检查点场景位置无效"
                )
        pointer = connection.execute(
            "SELECT * FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
            (project_id, chapter["id"]),
        ).fetchone()
        if pointer is None:
            connection.execute(
                "INSERT INTO chapter_workspace(project_id, chapter_id) VALUES (?, ?)",
                (project_id, chapter["id"]),
            )
            pointer = connection.execute(
                "SELECT * FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                (project_id, chapter["id"]),
            ).fetchone()
        if (
            enforce_candidate_cas or expected_candidate_revision_id is not None
        ) and pointer["candidate_revision_id"] != expected_candidate_revision_id:
            raise ConflictError(
                "stale_candidate_revision",
                "草稿候选版本已变化",
                {"currentCandidateRevisionId": pointer["candidate_revision_id"]},
                "比较并复制为最新版候选",
            )
        version = connection.execute(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM draft_revisions WHERE project_id = ? AND chapter_id = ?",
            (project_id, chapter["id"]),
        ).fetchone()[0]
        parent = pointer["candidate_revision_id"]
        if not groups:
            groups = [
                {"id": f"scene-group-{chapter['id']}", "sceneId": None, "body": body}
            ]
        accepted_revision_id = pointer["accepted_revision_id"]
        normalized_groups = []
        seen_group_ids: set[str] = set()
        for index, group in enumerate(groups):
            if (
                not isinstance(group, dict)
                or set(group) - {"id", "sceneId", "body", "locked"}
                or not isinstance(group.get("body"), str)
            ):
                raise WorkspaceError(
                    "invalid_scene_group", "每个场景变更组必须包含正文"
                )
            if "locked" in group and not isinstance(group["locked"], bool):
                raise WorkspaceError("invalid_scene_group", "场景锁定状态必须是布尔值")
            group_id = str(
                group.get("id") or f"scene-group-{chapter['id']}-{index + 1}"
            )[:160]
            if not group_id.strip() or group_id in seen_group_ids:
                raise WorkspaceError(
                    "invalid_scene_group", "每个场景变更组必须有唯一稳定 ID"
                )
            seen_group_ids.add(group_id)
            normalized_groups.append(
                {
                    **group,
                    "id": group_id,
                    "locked": group.get("locked", False),
                    "baseAcceptedRevisionId": accepted_revision_id,
                    "candidateDraftRevisionId": revision_id,
                }
            )
        groups = normalized_groups
        submitted_body = SCENE_SEPARATOR.join(group["body"].strip() for group in groups)
        if submitted_body != body.strip():
            raise WorkspaceError(
                "draft_scene_groups_mismatch",
                "草稿正文与场景变更组内容不一致",
                details={"sceneGroupCount": len(groups)},
            )
        if authorized_scene_order is not None:
            baseline_revision_id = (
                task["candidate_head_revision_id"] or accepted_revision_id
            )
            baseline_groups = []
            if baseline_revision_id:
                baseline = _draft_row(connection, project_id, baseline_revision_id)
                if baseline["chapter_id"] != chapter["id"]:
                    raise ConflictError(
                        "task_candidate_head_invalid",
                        "任务冻结的草稿候选不属于目标章节",
                        recovery="停止当前任务并基于目标章节重新生成",
                    )
                baseline_groups = _load(baseline["scene_groups_json"], [])
            tail = baseline_groups[authorized_scene_order:]
            if (
                len(groups) > authorized_scene_order
                or (tail and len(groups) != authorized_scene_order)
                or any(group["id"] in seen_group_ids for group in tail)
            ):
                raise ConflictError(
                    "checkpoint_scope_exceeded",
                    "草稿检查点包含任务范围外的场景",
                    {"maximumSceneOrder": authorized_scene_order},
                    "仅提交授权范围内的正文和场景组；存在后续场景时须保持授权前缀的场景数量",
                )
            groups.extend(
                {
                    **group,
                    "baseAcceptedRevisionId": accepted_revision_id,
                    "candidateDraftRevisionId": revision_id,
                }
                for group in tail
            )
        if accepted_revision_id:
            accepted = _draft_row(connection, project_id, accepted_revision_id)
            _assert_scene_group_locks(
                _load(accepted["scene_groups_json"], []), groups, actor
            )
        grouped_body = SCENE_SEPARATOR.join(group["body"].strip() for group in groups)
        if len(grouped_body) > MAX_BODY or len(groups) > 100:
            raise WorkspaceError("draft_too_large", "合并后的章节草稿超过长度限制", 413)
        body = grouped_body
        binding = connection.execute(
            "SELECT plan_revision_id, chapter_plan_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ?",
            (project_id, chapter["id"]),
        ).fetchone()
        if actor == "candidate":
            manifest = _load(task["input_manifest_json"], {})
            local_only_snapshot = manifest.get("localOnlySnapshot")
            if local_only_snapshot:
                source_refs = {"lineageStatus": "current", "localOnly": True}
            else:
                baseline = manifest.get("planningBaseline") or {}
                source_refs = {
                    "lineageStatus": baseline.get("lineageStatus", "current"),
                    "planRevisionId": baseline.get("planRevisionId"),
                    "outlineRevisionId": baseline.get("outlineRevisionId"),
                    "chapterPlanId": baseline.get("chapterPlanId"),
                    "localOnly": False,
                }
        elif binding:
            pointers = {
                row["artifact_kind"]: row["accepted_revision_id"]
                for row in connection.execute(
                    "SELECT artifact_kind, accepted_revision_id FROM artifact_pointers WHERE project_id = ?",
                    (project_id,),
                ).fetchall()
            }
            plan_id = binding["plan_revision_id"]
            plan = _load(
                _artifact_row(connection, project_id, "plan", plan_id)["payload_json"],
                {},
            )
            chapter_plan = next(
                (
                    item
                    for item in plan.get("chapters", [])
                    if item.get("chapterPlanId") == binding["chapter_plan_id"]
                ),
                None,
            )
            if chapter_plan is None:
                raise ConflictError(
                    "planning_binding_broken",
                    "章节绑定的历史章纲不存在",
                    recovery="检查作品数据并重新建立明确的规划绑定",
                )
            outline_id = plan.get("sourceOutlineRevisionId")
            source_refs = {
                "lineageStatus": "current"
                if plan_id == pointers.get("plan")
                and outline_id == pointers.get("outline")
                else "historical",
                "planRevisionId": plan_id,
                "outlineRevisionId": outline_id,
                "chapterPlanId": binding["chapter_plan_id"],
                "localOnly": False,
            }
        else:
            source_refs = {"lineageStatus": "current", "localOnly": True}
        connection.execute(
            "INSERT INTO draft_revisions(id, project_id, chapter_id, version, parent_revision_id, source_plan_refs_json, base_project_revision, body, scene_groups_json, source, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                revision_id,
                project_id,
                chapter["id"],
                version,
                parent,
                _json(source_refs),
                expected_revision,
                body,
                _json(groups),
                _source_for_actor(actor),
                now(),
            ),
        )
        connection.execute(
            "UPDATE chapter_workspace SET candidate_revision_id = ? WHERE project_id = ? AND chapter_id = ?",
            (revision_id, project_id, chapter["id"]),
        )
        if parent:
            connection.execute(
                "UPDATE review_bundles SET status = 'stale' WHERE project_id = ? AND artifact_revision_id = ? AND status = 'ready'",
                (project_id, parent),
            )
        if task_id:
            connection.execute(
                "INSERT INTO task_checkpoints(id, project_id, task_id, draft_revision_id, scene_id, completeness, created_at) "
                "VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    checkpoint_id,
                    project_id,
                    task_id,
                    revision_id,
                    payload.get("sceneId"),
                    completeness,
                    now(),
                ),
            )
            task = connection.execute(
                "SELECT output_refs_json FROM task_runs WHERE id = ?", (task_id,)
            ).fetchone()
            refs = _load(task["output_refs_json"], []) if task else []
            refs.append(
                {"kind": "draft", "revisionId": revision_id, "chapterId": chapter["id"]}
            )
            connection.execute(
                "UPDATE task_runs SET status = 'running', stage = 'writing', output_refs_json = ?, updated_at = ? WHERE id = ?",
                (_json(refs[-100:]), now(), task_id),
            )
            connection.execute(
                "UPDATE task_runs SET candidate_head_revision_id = ? WHERE id = ?",
                (revision_id, task_id),
            )
            record_task_output(connection, task_id, "draft", revision_id, completeness)
        result = {
            "chapterId": chapter["id"],
            "revisionId": revision_id,
            "version": version,
            "body": body,
            "sceneGroups": groups,
            "checkpointId": checkpoint_id if task_id else None,
            "completeness": completeness,
        }
        if idempotency_key:
            connection.execute(
                "INSERT INTO candidate_write_requests(project_id, owner_key, idempotency_key, request_hash, result_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                (project_id, owner_key, idempotency_key, digest, _json(result), now()),
            )
    return result


def _fact_payload(row) -> dict[str, Any]:
    return {
        "id": row["id"],
        "sceneGroupId": row["scene_group_id"],
        "sceneId": row["scene_id"],
        "factKind": row["fact_kind"],
        "subjectId": row["subject_id"],
        "predicate": row["predicate"],
        "value": _load(row["value_json"], None),
        "epistemicOwnerId": row["epistemic_owner_id"],
        "epistemicState": row["epistemic_state"],
        "storyTime": row["story_time"],
        "readingChapter": row["reading_chapter"],
        "sceneOrder": row["scene_order"],
        "evidence": row["evidence"],
        "reviewDecision": row["review_decision"],
        "source": row["source"],
    }


def create_review(
    db_path: str | Path,
    project_id: str,
    artifact_kind: str,
    revision_id: str,
    proposals: list[dict[str, Any]] | None = None,
    selections: dict[str, Any] | None = None,
    actor: str = "author",
    task_id: str | None = None,
    thread_id: str | None = None,
    idempotency_key: str | None = None,
) -> dict[str, Any]:
    if artifact_kind not in {*ARTIFACT_KINDS, "draft"}:
        raise WorkspaceError("invalid_artifact_kind", "审阅产物类型无效")
    initialize(db_path)
    review_id = _id("review")
    checks: list[dict[str, Any]] = []
    provisional_impacts: list[dict[str, Any]] = []
    with connect(db_path) as connection:
        _assert_actor(connection, project_id, actor, task_id, thread_id)
        owner_key = _candidate_write_owner(actor, task_id)
        candidate_task = None
        digest = _request_hash(
            {
                "operation": "createReview",
                "artifactKind": artifact_kind,
                "revisionId": revision_id,
                "proposals": proposals or [],
                "selections": selections or {},
            }
        )
        if idempotency_key is not None:
            if not idempotency_key.strip() or len(idempotency_key) > 160:
                raise WorkspaceError("invalid_idempotency_key", "幂等键无效")
            existing = connection.execute(
                "SELECT request_hash, result_json FROM candidate_write_requests WHERE project_id = ? AND owner_key = ? AND idempotency_key = ?",
                (project_id, owner_key, idempotency_key),
            ).fetchone()
            if existing:
                if existing["request_hash"] != digest:
                    raise ConflictError(
                        "idempotency_key_reused",
                        "幂等键已用于不同的审阅请求",
                        recovery="为新的审阅操作生成新的幂等键",
                    )
                return _load(existing["result_json"], {})
        if actor == "candidate":
            candidate_task = _require_task(connection, project_id, task_id, thread_id)
            assert_task_output(
                candidate_task,
                f"{artifact_kind}-review"
                if artifact_kind == "draft"
                else artifact_kind,
            )
        workspace = _workspace(connection, project_id)
        if artifact_kind == "draft":
            artifact = _draft_row(connection, project_id, revision_id)
            draft_groups = _load(artifact["scene_groups_json"], [])
            chapter_number = connection.execute(
                "SELECT number FROM chapters WHERE id = ? AND project_id = ?",
                (artifact["chapter_id"], project_id),
            ).fetchone()[0]
            candidate_target: dict[str, Any] = {}
            if candidate_task is not None:
                latest_output = connection.execute(
                    "SELECT output_kind, revision_id FROM task_outputs WHERE task_id = ? ORDER BY ordinal DESC LIMIT 1",
                    (candidate_task["id"],),
                ).fetchone()
                candidate_target = task_target(candidate_task)
                if (
                    latest_output is None
                    or latest_output["output_kind"] != "draft"
                    or latest_output["revision_id"] != revision_id
                ):
                    raise ConflictError(
                        "review_task_output_mismatch",
                        "只能审阅当前任务最新保存的草稿",
                        recovery="使用当前任务最后一次草稿检查点创建审阅",
                    )
                if candidate_target.get("chapterId") != artifact["chapter_id"]:
                    raise ConflictError(
                        "review_task_target_mismatch",
                        "待审草稿不属于当前任务目标章节",
                        recovery="审阅当前任务目标章节的最新草稿",
                    )
            pointer = connection.execute(
                "SELECT candidate_revision_id FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                (project_id, artifact["chapter_id"]),
            ).fetchone()
            current = pointer and pointer["candidate_revision_id"] == revision_id
            checks.append(
                {
                    "id": "current-draft",
                    "passed": bool(current),
                    "blocking": True,
                    "message": "草稿是当前候选版本",
                }
            )
            scene_orders_by_group = {
                group.get("id"): scene_order
                for scene_order, group in enumerate(draft_groups, start=1)
                if isinstance(group, dict) and isinstance(group.get("id"), str)
            }
            for item in proposals or []:
                predicate = str(item.get("predicate", "")).strip()
                evidence = str(item.get("evidence", "")).strip()
                fact_kind = item.get("factKind", "world")
                epistemic = item.get("epistemicState", "known")
                if (
                    not predicate
                    or not evidence
                    or fact_kind
                    not in {"world", "reader", "character", "author-secret"}
                    or epistemic not in {"unknown", "suspected", "known", "false"}
                ):
                    raise WorkspaceError(
                        "invalid_fact_proposal", "候选事实缺少谓词、证据或有效分类"
                    )
                proposal_id = _id("fact")
                group_ids = set(scene_orders_by_group)
                scene_group_id = item.get("sceneGroupId") or (
                    next(iter(group_ids)) if len(group_ids) == 1 else None
                )
                if scene_group_id not in group_ids:
                    raise WorkspaceError(
                        "fact_scene_group_required",
                        "候选事实必须绑定一个有效场景变更组",
                    )
                scene_group = next(
                    group for group in draft_groups if group.get("id") == scene_group_id
                )
                reading_chapter = item.get("readingChapter")
                scene_order = item.get("sceneOrder")
                story_time = item.get("storyTime")
                if (
                    isinstance(reading_chapter, bool)
                    or not isinstance(reading_chapter, int)
                    or reading_chapter != chapter_number
                ):
                    raise WorkspaceError(
                        "fact_scope_mismatch", "候选事实阅读章节与目标章节不一致"
                    )
                if (
                    isinstance(scene_order, bool)
                    or not isinstance(scene_order, int)
                    or scene_order < 0
                ):
                    raise WorkspaceError("fact_scope_mismatch", "候选事实场景顺序无效")
                if scene_order != scene_orders_by_group[scene_group_id]:
                    raise WorkspaceError(
                        "fact_scene_order_mismatch",
                        "事实位置必须由所属场景变更组确定",
                        {
                            "sceneGroupId": scene_group_id,
                            "expectedSceneOrder": scene_orders_by_group[scene_group_id],
                            "actualSceneOrder": scene_order,
                        },
                    )
                if story_time is not None and (
                    isinstance(story_time, bool)
                    or not isinstance(story_time, int)
                    or story_time < 0
                ):
                    raise WorkspaceError("fact_scope_mismatch", "候选事实故事时间无效")
                if item.get("sceneId") != scene_group.get("sceneId"):
                    raise WorkspaceError(
                        "fact_scope_mismatch", "候选事实场景与变更组不一致"
                    )
                if candidate_task is not None:
                    maximum_scene_order = candidate_target.get("sceneOrder")
                    if scene_order > maximum_scene_order:
                        raise WorkspaceError(
                            "fact_scope_mismatch", "候选事实超出任务场景范围"
                        )
                    if reading_chapter != candidate_target.get("readingChapter"):
                        raise WorkspaceError(
                            "fact_scope_mismatch", "候选事实超出任务章节范围"
                        )
                    if story_time != candidate_target.get("storyTime"):
                        raise WorkspaceError(
                            "fact_scope_mismatch", "候选事实故事时间与任务不一致"
                        )
                connection.execute(
                    "INSERT INTO fact_proposals(id, project_id, draft_revision_id, scene_group_id, scene_id, fact_kind, subject_id, predicate, value_json, epistemic_owner_id, epistemic_state, story_time, reading_chapter, scene_order, evidence, source, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        proposal_id,
                        project_id,
                        revision_id,
                        scene_group_id,
                        item.get("sceneId"),
                        fact_kind,
                        item.get("subjectId"),
                        predicate[:160],
                        _json(item.get("value")),
                        item.get("epistemicOwnerId"),
                        epistemic,
                        story_time,
                        reading_chapter,
                        scene_order,
                        evidence[:1000],
                        _source_for_actor(actor),
                        now(),
                    ),
                )
            facts = connection.execute(
                "SELECT * FROM fact_proposals WHERE project_id = ? AND draft_revision_id = ? ORDER BY created_at",
                (project_id, revision_id),
            ).fetchall()
            provisional_impacts = preview_planning_impacts(
                connection,
                project_id,
                [_fact_payload(row) for row in facts],
                chapter_number,
            )
            checks.append(
                {
                    "id": "fact-evidence",
                    "passed": all(row["evidence"].strip() for row in facts),
                    "blocking": True,
                    "message": "所有候选事实都有原文证据",
                }
            )
            reconstructed = SCENE_SEPARATOR.join(
                group.get("body", "").strip() for group in draft_groups
            )
            checks.append(
                {
                    "id": "draft-scene-groups",
                    "passed": bool(draft_groups)
                    and reconstructed == artifact["body"].strip(),
                    "blocking": True,
                    "message": "待采用正文与场景变更组一致",
                }
            )
            groups_by_id = {group.get("id"): group for group in draft_groups}
            anchored = all(
                row["fact_kind"] == "author-secret"
                or row["evidence"].strip()
                in groups_by_id.get(row["scene_group_id"], {}).get("body", "")
                for row in facts
            )
            checks.append(
                {
                    "id": "fact-evidence-anchor",
                    "passed": anchored,
                    "blocking": True,
                    "message": "候选事实证据存在于对应场景正文",
                }
            )
            checks.append(
                {
                    "id": "semantic-review",
                    "passed": False,
                    "blocking": False,
                    "message": "叙事与语言语义检查尚未执行",
                }
            )
        else:
            artifact = _artifact_row(connection, project_id, artifact_kind, revision_id)
            pointer = connection.execute(
                "SELECT candidate_revision_id, locked FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
                (project_id, artifact_kind),
            ).fetchone()
            if pointer and pointer["locked"]:
                raise ConflictError(
                    "artifact_locked", "该正式产物已锁定", recovery="先由作者显式解锁"
                )
            checks.append(
                {
                    "id": "current-artifact",
                    "passed": bool(
                        pointer and pointer["candidate_revision_id"] == revision_id
                    ),
                    "blocking": True,
                    "message": "产物是当前候选版本",
                }
            )
        source_revision = artifact["base_project_revision"]
        checks.append(
            {
                "id": "source-revision",
                "passed": source_revision == workspace["revision"],
                "blocking": True,
                "message": "候选基于当前正式版本",
            }
        )
        stored_selections = dict(selections or {})
        if artifact_kind == "draft":
            stored_selections["provisionalImpacts"] = provisional_impacts
        connection.execute(
            "UPDATE review_bundles SET status = 'stale' WHERE project_id = ? AND artifact_kind = ? AND status = 'ready'",
            (project_id, artifact_kind),
        )
        connection.execute(
            "INSERT INTO review_bundles(id, project_id, artifact_kind, artifact_revision_id, project_revision, selections_json, checks_json, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'ready', ?)",
            (
                review_id,
                project_id,
                artifact_kind,
                revision_id,
                workspace["revision"],
                _json(stored_selections),
                _json(checks),
                now(),
            ),
        )
        facts = (
            connection.execute(
                "SELECT * FROM fact_proposals WHERE project_id = ? AND draft_revision_id = ? ORDER BY created_at",
                (project_id, revision_id),
            ).fetchall()
            if artifact_kind == "draft"
            else []
        )
        result = {
            "id": review_id,
            "artifactKind": artifact_kind,
            "artifactRevisionId": revision_id,
            "projectRevision": workspace["revision"],
            "status": "ready",
            "checks": checks,
            "factProposals": [_fact_payload(row) for row in facts],
            "provisionalImpacts": provisional_impacts,
            "selections": stored_selections,
        }
        if idempotency_key is not None:
            connection.execute(
                "INSERT INTO candidate_write_requests(project_id, owner_key, idempotency_key, request_hash, result_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                (project_id, owner_key, idempotency_key, digest, _json(result), now()),
            )
    return result


def _request_hash(payload: dict[str, Any]) -> str:
    return hashlib.sha256(_json(payload).encode("utf-8")).hexdigest()


def _merge_scene_groups(
    connection, project_id: str, draft, selections: dict[str, Any]
) -> tuple[str, list[dict[str, Any]]]:
    candidate_groups = _load(draft["scene_groups_json"], [])
    selected = selections.get("sceneGroupIds")
    if not isinstance(selected, list) or not selected:
        return draft["body"], candidate_groups
    candidate_ids = {item.get("id") for item in candidate_groups if item.get("id")}
    selected_ids = set(selected)
    if not selected_ids.issubset(candidate_ids):
        raise WorkspaceError(
            "invalid_scene_selection", "选择中包含不属于当前草稿的场景"
        )
    pointer = connection.execute(
        "SELECT accepted_revision_id FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
        (project_id, draft["chapter_id"]),
    ).fetchone()
    for group in candidate_groups:
        if (
            group.get("baseAcceptedRevisionId")
            != (pointer["accepted_revision_id"] if pointer else None)
            or group.get("candidateDraftRevisionId") != draft["id"]
        ):
            raise ConflictError(
                "scene_group_base_changed",
                "场景变更组的基线已变化",
                recovery="基于最新正式稿重新保存并检查",
            )
    if selected_ids == candidate_ids:
        return draft["body"], candidate_groups
    base = (
        _draft_row(connection, project_id, pointer["accepted_revision_id"])
        if pointer and pointer["accepted_revision_id"]
        else None
    )
    base_groups = _load(base["scene_groups_json"], []) if base else []
    base_by_id = {item.get("id"): item for item in base_groups}
    candidate_by_id = {item.get("id"): item for item in candidate_groups}
    ordered_ids = list(
        dict.fromkeys(
            [
                item.get("id")
                for item in candidate_groups + base_groups
                if item.get("id")
            ]
        )
    )
    merged = [
        candidate_by_id[group_id]
        if group_id in selected_ids and group_id in candidate_by_id
        else base_by_id.get(group_id)
        for group_id in ordered_ids
    ]
    merged = [item for item in merged if item]
    return SCENE_SEPARATOR.join(
        str(item.get("body", "")).strip() for item in merged
    ).strip(), merged


def _ensure_passage(
    connection, project_id: str, chapter_id: str, body: str
) -> tuple[str, str]:
    passage = connection.execute(
        "SELECT id, body FROM passages WHERE chapter_id = ? ORDER BY position LIMIT 1",
        (chapter_id,),
    ).fetchone()
    previous = passage["body"] if passage else ""
    if passage:
        connection.execute(
            "UPDATE passages SET body = ? WHERE id = ?", (body, passage["id"])
        )
    else:
        scene_id = _id("scene")
        beat_id = _id("beat")
        passage_id = _id("passage")
        connection.execute(
            "INSERT INTO scenes(id, project_id, title, location, time, summary, status) VALUES (?, ?, '未命名场景', '', '', '', 'ready')",
            (scene_id, project_id),
        )
        connection.execute(
            "INSERT INTO beats(id, scene_id, position, description) VALUES (?, ?, 1, '章节正文')",
            (beat_id, scene_id),
        )
        connection.execute(
            "INSERT INTO passages(id, chapter_id, position, body, scene_id, beat_ids) VALUES (?, ?, 1, ?, ?, ?)",
            (passage_id, chapter_id, body, scene_id, _json([beat_id])),
        )
    if previous != body:
        connection.execute(
            "INSERT INTO chapter_diffs(chapter_id, from_text, to_text, summary, created_at) VALUES (?, ?, ?, '采用草稿修订', ?)",
            (chapter_id, previous, body, now()),
        )
    connection.execute(
        "UPDATE chapters SET status = 'accepted', updated_at = ? WHERE id = ?",
        (now(), chapter_id),
    )
    return previous, body


def _accepted_plan(connection, project_id: str) -> tuple[str | None, dict[str, Any]]:
    pointer = connection.execute(
        "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'plan'",
        (project_id,),
    ).fetchone()
    if not pointer or not pointer["accepted_revision_id"]:
        return None, {}
    row = _artifact_row(connection, project_id, "plan", pointer["accepted_revision_id"])
    return row["id"], _load(row["payload_json"], {})


def _state_event_payload(row) -> dict[str, Any] | None:
    if row is None:
        return None
    return {
        "id": row["id"],
        "factKind": row["fact_kind"],
        "subjectId": row["subject_id"],
        "predicate": row["predicate"],
        "value": _load(row["value_json"], None),
        "epistemicOwnerId": row["epistemic_owner_id"],
        "epistemicState": row["epistemic_state"],
        "storyTime": row["story_time"],
        "readingChapter": row["reading_chapter"],
        "sceneOrder": row["scene_order"],
        "sourceDraftRevisionId": row["source_draft_revision_id"],
        "evidence": row["evidence"],
    }


def _existing_commit(
    connection, project_id: str, operation_kind: str, idempotency_key: str, digest: str
) -> dict[str, Any] | None:
    existing = connection.execute(
        "SELECT * FROM acceptance_commits WHERE project_id = ? AND operation_kind = ? AND idempotency_key = ?",
        (project_id, operation_kind, idempotency_key),
    ).fetchone()
    if existing is None:
        return None
    if existing["request_hash"] != digest:
        raise ConflictError("idempotency_key_reused", "同一幂等键对应不同请求")
    return _load(existing["receipt_json"], {})


def accept_review(
    db_path: str | Path,
    project_id: str,
    review_id: str,
    expected_revision: int,
    idempotency_key: str,
    selections: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if not idempotency_key.strip() or len(idempotency_key) > 160:
        raise WorkspaceError("invalid_idempotency_key", "幂等键无效")
    request = {
        "reviewId": review_id,
        "expectedRevision": expected_revision,
        "selections": selections or {},
    }
    digest = _request_hash(request)
    initialize(db_path)
    with connect(db_path) as connection:
        existing_receipt = _existing_commit(
            connection, project_id, "accept", idempotency_key, digest
        )
    if existing_receipt is not None:
        return existing_receipt
    story_report = run_story_tests(db_path, project_id)
    if not story_report["passed"]:
        failed = [
            check["id"]
            for check in story_report["checks"]
            if not check["passed"] and check["severity"] == "error"
        ]
        raise WorkspaceError(
            "story_tests_failed",
            "现有正式故事结构未通过硬检查",
            details={"checkIds": failed},
            recovery="修复 Story Tests 后重新审阅",
        )
    receipt: dict[str, Any]
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        _project(connection, project_id)
        existing_receipt = _existing_commit(
            connection, project_id, "accept", idempotency_key, digest
        )
        if existing_receipt is not None:
            return existing_receipt
        workspace = _workspace(connection, project_id)
        review = connection.execute(
            "SELECT * FROM review_bundles WHERE id = ? AND project_id = ?",
            (review_id, project_id),
        ).fetchone()
        if review is None:
            raise NotFoundError("审阅包不存在")
        if (
            review["status"] != "ready"
            or review["project_revision"] != expected_revision
            or workspace["revision"] != expected_revision
        ):
            raise ConflictError(
                "stale_review",
                "审阅基于旧版本",
                {"currentRevision": workspace["revision"]},
                "基于最新草稿重新检查",
            )
        artifact_kind = review["artifact_kind"]
        checks = _load(review["checks_json"], [])
        deferred_draft_checks = {"fact-evidence", "fact-evidence-anchor"}
        if any(
            check.get("blocking")
            and not check.get("passed")
            and not (
                artifact_kind == "draft" and check.get("id") in deferred_draft_checks
            )
            for check in checks
        ):
            raise WorkspaceError("review_checks_failed", "审阅仍有阻塞问题")
        commit_id = _id("commit")
        result_revision = expected_revision + 1
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, review_id, receipt_json, created_at) VALUES (?, ?, 'accept', ?, ?, ?, ?, ?, '{}', ?)",
            (
                commit_id,
                project_id,
                idempotency_key,
                digest,
                expected_revision,
                result_revision,
                review_id,
                now(),
            ),
        )
        previous_accepted = None
        confirmed_facts: list[dict[str, Any]] = []
        impacts: list[dict[str, Any]] = []
        resolved_impact_ids: list[str] = []
        migrated_impact_ids: list[str] = []
        merged_revision_id = review["artifact_revision_id"]
        if artifact_kind in ARTIFACT_KINDS:
            artifact = _artifact_row(
                connection, project_id, artifact_kind, review["artifact_revision_id"]
            )
            if artifact["base_project_revision"] != expected_revision:
                raise ConflictError(
                    "stale_artifact_base",
                    "候选产物基于旧作品版本",
                    recovery="基于最新正式版本重新生成候选",
                )
            pointer = connection.execute(
                "SELECT * FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?",
                (project_id, artifact_kind),
            ).fetchone()
            if pointer["candidate_revision_id"] != artifact["id"]:
                raise ConflictError("stale_artifact", "候选产物已更新")
            if pointer["locked"]:
                raise ConflictError(
                    "artifact_locked", "当前正式产物已锁定", recovery="先由作者显式解锁"
                )
            previous_accepted = pointer["accepted_revision_id"]
            if artifact_kind == "plan":
                plan_payload = _load(artifact["payload_json"], {})
                outline_pointer = connection.execute(
                    "SELECT accepted_revision_id FROM artifact_pointers "
                    "WHERE project_id = ? AND artifact_kind = 'outline'",
                    (project_id,),
                ).fetchone()
                accepted_outline_id = (
                    outline_pointer["accepted_revision_id"] if outline_pointer else None
                )
                if plan_payload.get("sourceOutlineRevisionId") != accepted_outline_id:
                    raise ConflictError(
                        "plan_source_not_accepted",
                        "章纲依据的工作大纲尚未正式采用",
                        {
                            "sourceOutlineRevisionId": plan_payload.get(
                                "sourceOutlineRevisionId"
                            ),
                            "acceptedOutlineRevisionId": accepted_outline_id,
                        },
                        "先采用该大纲修订，再重新审阅章纲",
                    )
            if previous_accepted:
                assert_open_impact_targets_preserved(
                    connection,
                    project_id,
                    artifact_kind,
                    previous_accepted,
                    _load(artifact["payload_json"], {}),
                )
            if artifact_kind == "plan" and previous_accepted:
                accepted = _artifact_row(
                    connection, project_id, artifact_kind, previous_accepted
                )
                lock_actor = "candidate" if artifact["source"] == "agent" else "author"
                _assert_plan_locks(
                    _load(accepted["payload_json"], {}),
                    _load(artifact["payload_json"], {}),
                    lock_actor,
                )
            if artifact_kind == "outline" and previous_accepted:
                accepted = _artifact_row(
                    connection, project_id, artifact_kind, previous_accepted
                )
                outline_actor = (
                    "candidate" if artifact["source"] == "agent" else "author"
                )
                assert_outline_protections(
                    _load(accepted["payload_json"], {}),
                    _load(artifact["payload_json"], {}),
                    outline_actor,
                )
            connection.execute(
                "UPDATE artifact_pointers SET accepted_revision_id = ? WHERE project_id = ? AND artifact_kind = ?",
                (artifact["id"], project_id, artifact_kind),
            )
            advanced = advance_planning_impacts(
                connection,
                project_id,
                artifact_kind,
                previous_accepted,
                artifact["id"],
                _load(artifact["payload_json"], {}),
                commit_id,
            )
            resolved_impact_ids = advanced["resolved"]
            migrated_impact_ids = advanced["migrated"]
        else:
            draft = _draft_row(connection, project_id, review["artifact_revision_id"])
            if draft["base_project_revision"] != expected_revision:
                raise ConflictError(
                    "stale_draft_base",
                    "候选草稿基于旧作品版本",
                    recovery="基于最新正式版本重新生成草稿",
                )
            pointer = connection.execute(
                "SELECT * FROM chapter_workspace WHERE project_id = ? AND chapter_id = ?",
                (project_id, draft["chapter_id"]),
            ).fetchone()
            if not pointer or pointer["candidate_revision_id"] != draft["id"]:
                raise ConflictError("stale_draft", "草稿已被更新")
            previous_accepted = pointer["accepted_revision_id"]
            if previous_accepted:
                accepted = _draft_row(connection, project_id, previous_accepted)
                lock_actor = "candidate" if draft["source"] == "agent" else "author"
                _assert_scene_group_locks(
                    _load(accepted["scene_groups_json"], []),
                    _load(draft["scene_groups_json"], []),
                    lock_actor,
                )
            fact_rows = connection.execute(
                "SELECT * FROM fact_proposals WHERE project_id = ? AND draft_revision_id = ? ORDER BY created_at",
                (project_id, draft["id"]),
            ).fetchall()
            fact_payloads = [_fact_payload(row) for row in fact_rows]
            accepted_chapter = connection.execute(
                "SELECT number FROM chapters WHERE id = ?", (draft["chapter_id"],)
            ).fetchone()[0]
            acceptance = validate_draft_acceptance(
                connection,
                project_id,
                draft,
                review,
                selections or {},
                fact_rows,
                fact_payloads,
                accepted_chapter,
            )
            decisions = acceptance["decisions"]
            selected_group_ids = acceptance["selectedGroupIds"]
            manual_links = acceptance["manualLinks"]
            direction_decisions = acceptance["directionDecisions"]
            for link in manual_links:
                connection.execute(
                    "INSERT INTO author_fact_dependency_links(id, project_id, fact_proposal_id, target_artifact_kind, target_revision_id, target_node_id, created_by_commit_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        _id("fact-dependency"),
                        project_id,
                        link["factProposalId"],
                        link["targetArtifactKind"],
                        link["targetRevisionId"],
                        link["targetNodeId"],
                        commit_id,
                        now(),
                    ),
                )
            merged_body, merged_groups = _merge_scene_groups(
                connection, project_id, draft, selections or {}
            )
            if merged_body != draft["body"]:
                version = connection.execute(
                    "SELECT COALESCE(MAX(version), 0) + 1 FROM draft_revisions WHERE project_id = ? AND chapter_id = ?",
                    (project_id, draft["chapter_id"]),
                ).fetchone()[0]
                merged_revision_id = _id("draft")
                connection.execute(
                    "INSERT INTO draft_revisions(id, project_id, chapter_id, version, parent_revision_id, source_plan_refs_json, base_project_revision, body, scene_groups_json, source, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'author', ?)",
                    (
                        merged_revision_id,
                        project_id,
                        draft["chapter_id"],
                        version,
                        draft["id"],
                        draft["source_plan_refs_json"],
                        expected_revision,
                        merged_body,
                        _json(merged_groups),
                        now(),
                    ),
                )
            connection.execute(
                "UPDATE chapter_workspace SET accepted_revision_id = ? WHERE project_id = ? AND chapter_id = ?",
                (merged_revision_id, project_id, draft["chapter_id"]),
            )
            previous_body, _ = _ensure_passage(
                connection, project_id, draft["chapter_id"], merged_body
            )
            for fact in fact_rows:
                decision = decisions.get(fact["id"], fact["review_decision"])
                if fact["scene_group_id"] not in selected_group_ids:
                    if decision == "confirmed":
                        raise WorkspaceError(
                            "fact_scene_not_selected",
                            "不能确认未采用场景中的事实",
                            details={
                                "factId": fact["id"],
                                "sceneGroupId": fact["scene_group_id"],
                            },
                        )
                    decision = "rejected"
                if decision not in {"confirmed", "rejected"}:
                    raise WorkspaceError(
                        "fact_decision_required",
                        "每条候选事实都必须确认或拒绝",
                        details={"factId": fact["id"]},
                    )
                connection.execute(
                    "UPDATE fact_proposals SET review_decision = ? WHERE id = ?",
                    (decision, fact["id"]),
                )
                if decision == "confirmed":
                    previous_event = connection.execute(
                        "SELECT * FROM temporal_state_events WHERE project_id = ? AND fact_kind = ? AND subject_id IS ? AND predicate = ? AND epistemic_owner_id IS ? ORDER BY project_revision DESC, id DESC LIMIT 1",
                        (
                            project_id,
                            fact["fact_kind"],
                            fact["subject_id"],
                            fact["predicate"],
                            fact["epistemic_owner_id"],
                        ),
                    ).fetchone()
                    event_id = _id("state")
                    connection.execute(
                        "INSERT INTO temporal_state_events(id, project_id, project_revision, fact_kind, subject_id, predicate, value_json, epistemic_owner_id, epistemic_state, story_time, reading_chapter, scene_order, source_draft_revision_id, evidence, supersedes_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            event_id,
                            project_id,
                            result_revision,
                            fact["fact_kind"],
                            fact["subject_id"],
                            fact["predicate"],
                            fact["value_json"],
                            fact["epistemic_owner_id"],
                            fact["epistemic_state"],
                            fact["story_time"],
                            fact["reading_chapter"],
                            fact["scene_order"],
                            merged_revision_id,
                            fact["evidence"],
                            previous_event["id"] if previous_event else None,
                            now(),
                        ),
                    )
                    confirmed_facts.append(
                        {
                            "eventId": event_id,
                            "previousEvent": _state_event_payload(previous_event),
                            **_fact_payload(fact),
                        }
                    )
            impacts = record_planning_impacts(
                connection,
                project_id,
                commit_id,
                merged_revision_id,
                confirmed_facts,
                accepted_chapter,
                direction_decisions,
                manual_links,
            )
        receipt = {
            "commitId": commit_id,
            "projectId": project_id,
            "operation": "accept",
            "artifactKind": artifact_kind,
            "artifactRevisionId": merged_revision_id,
            "previousAcceptedRevisionId": previous_accepted,
            "baseRevision": expected_revision,
            "resultRevision": result_revision,
            "confirmedFacts": confirmed_facts,
            "impacts": impacts,
            "resolvedImpactIds": resolved_impact_ids,
            "migratedImpactIds": migrated_impact_ids,
            "indexTargetRevision": result_revision,
        }
        connection.execute(
            "UPDATE review_bundles SET status = 'accepted', selections_json = ? WHERE id = ?",
            (_json(selections or {}), review_id),
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, project_id),
        )
        connection.execute(
            "UPDATE projects SET updated_at = ? WHERE id = ?", (now(), project_id)
        )
        connection.execute(
            "UPDATE acceptance_commits SET receipt_json = ? WHERE id = ?",
            (_json(receipt), commit_id),
        )
    try:
        refresh_search_index(db_path, project_id)
        with connect(db_path) as connection:
            connection.execute(
                "UPDATE project_workspace SET indexed_revision = MAX(indexed_revision, ?) WHERE project_id = ?",
                (receipt["resultRevision"], project_id),
            )
    except Exception:
        pass
    return receipt


def undo_commit(
    db_path: str | Path,
    project_id: str,
    commit_id: str,
    expected_revision: int,
    idempotency_key: str,
) -> dict[str, Any]:
    request = {"commitId": commit_id, "expectedRevision": expected_revision}
    digest = _request_hash(request)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        _project(connection, project_id)
        existing_receipt = _existing_commit(
            connection, project_id, "undo", idempotency_key, digest
        )
        if existing_receipt is not None:
            return existing_receipt
        workspace = _workspace(connection, project_id)
        target = connection.execute(
            "SELECT * FROM acceptance_commits WHERE id = ? AND project_id = ? AND operation_kind != 'undo'",
            (commit_id, project_id),
        ).fetchone()
        if target is None:
            raise NotFoundError("采用提交不存在")
        latest = connection.execute(
            "SELECT id FROM acceptance_commits WHERE project_id = ? ORDER BY result_revision DESC LIMIT 1",
            (project_id,),
        ).fetchone()
        if (
            workspace["revision"] != expected_revision
            or target["result_revision"] != expected_revision
            or not latest
            or latest["id"] != commit_id
        ):
            raise ConflictError(
                "undo_has_dependencies",
                "该提交之后已有其他正式变更",
                {"currentRevision": workspace["revision"]},
                "查看后续依赖并进行局部纠正",
            )
        target_receipt = _load(target["receipt_json"], {})
        operation_kind = target["operation_kind"]
        artifact_kind = target_receipt.get("artifactKind", operation_kind)
        previous = target_receipt.get("previousAcceptedRevisionId")
        if operation_kind == "accept":
            if artifact_kind in ARTIFACT_KINDS:
                connection.execute(
                    "UPDATE artifact_pointers SET accepted_revision_id = ? WHERE project_id = ? AND artifact_kind = ?",
                    (previous, project_id, artifact_kind),
                )
            else:
                current_draft = _draft_row(
                    connection, project_id, target_receipt["artifactRevisionId"]
                )
                connection.execute(
                    "UPDATE chapter_workspace SET accepted_revision_id = ? WHERE project_id = ? AND chapter_id = ?",
                    (previous, project_id, current_draft["chapter_id"]),
                )
                previous_draft = (
                    _draft_row(connection, project_id, previous) if previous else None
                )
                _ensure_passage(
                    connection,
                    project_id,
                    current_draft["chapter_id"],
                    previous_draft["body"] if previous_draft else "",
                )
                for fact in target_receipt.get("confirmedFacts", []):
                    previous_event = fact.get("previousEvent")
                    restored = previous_event or {
                        "factKind": fact["factKind"],
                        "subjectId": fact.get("subjectId"),
                        "predicate": fact["predicate"],
                        "value": None,
                        "epistemicOwnerId": fact.get("epistemicOwnerId"),
                        "epistemicState": "unknown",
                        "storyTime": fact.get("storyTime"),
                        "readingChapter": fact["readingChapter"],
                        "sceneOrder": fact["sceneOrder"],
                        "sourceDraftRevisionId": previous,
                        "evidence": "撤销采用提交（恢复为空）",
                    }
                    connection.execute(
                        "INSERT INTO temporal_state_events(id, project_id, project_revision, fact_kind, subject_id, predicate, value_json, epistemic_owner_id, epistemic_state, story_time, reading_chapter, scene_order, source_draft_revision_id, evidence, supersedes_id, reverted_by_commit_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            _id("state"),
                            project_id,
                            expected_revision + 1,
                            restored["factKind"],
                            restored.get("subjectId"),
                            restored["predicate"],
                            _json(restored.get("value")),
                            restored.get("epistemicOwnerId"),
                            restored["epistemicState"],
                            restored.get("storyTime"),
                            restored["readingChapter"],
                            restored["sceneOrder"],
                            restored.get("sourceDraftRevisionId"),
                            restored["evidence"],
                            fact.get("eventId"),
                            commit_id,
                            now(),
                        ),
                    )
                connection.execute(
                    "UPDATE fact_proposals SET review_decision = 'candidate' WHERE project_id = ? AND draft_revision_id = ?",
                    (project_id, current_draft["id"]),
                )
            review_revision_id = connection.execute(
                "SELECT artifact_revision_id FROM review_bundles WHERE id = ?",
                (target["review_id"],),
            ).fetchone()[0]
            current_candidate = connection.execute(
                "SELECT CASE WHEN ? = 'draft' THEN (SELECT candidate_revision_id FROM chapter_workspace WHERE project_id = ? AND chapter_id = (SELECT chapter_id FROM draft_revisions WHERE id = ?)) ELSE (SELECT candidate_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = ?) END",
                (
                    artifact_kind,
                    project_id,
                    review_revision_id,
                    project_id,
                    artifact_kind,
                ),
            ).fetchone()[0]
            restored_review_status = (
                "ready" if current_candidate == review_revision_id else "stale"
            )
            connection.execute(
                "UPDATE review_bundles SET status = ? WHERE id = ?",
                (restored_review_status, target["review_id"]),
            )
        elif operation_kind in {"materialize-chapter", "materialize-standalone"}:
            chapter_id = target_receipt["chapterId"]
            dependent_draft = connection.execute(
                "SELECT 1 FROM draft_revisions WHERE project_id = ? AND chapter_id = ? LIMIT 1",
                (project_id, chapter_id),
            ).fetchone()
            dependent_task = connection.execute(
                "SELECT 1 FROM task_runs WHERE project_id = ? AND target_json LIKE ? LIMIT 1",
                (project_id, f'%"chapterId":"{chapter_id}"%'),
            ).fetchone()
            if dependent_draft or dependent_task:
                raise ConflictError(
                    "undo_has_dependencies",
                    "章节创建后已有草稿或生成任务",
                    recovery="保留章节并进行局部调整",
                )
            connection.execute(
                "DELETE FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ?",
                (project_id, chapter_id),
            )
            connection.execute(
                "DELETE FROM chapters WHERE project_id = ? AND id = ?",
                (project_id, chapter_id),
            )
        elif operation_kind == "associate-plan":
            connection.execute(
                "DELETE FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ? AND materialized_by_commit_id = ?",
                (project_id, target_receipt["chapterId"], commit_id),
            )
        elif operation_kind == "resolve-impact":
            restored = connection.execute(
                "UPDATE planning_impacts SET state = 'open', resolution_note = NULL, resolved_by_commit_id = NULL, resolved_at = NULL WHERE id = ? AND project_id = ? AND resolved_by_commit_id = ?",
                (target_receipt["impactId"], project_id, commit_id),
            )
            if restored.rowcount != 1:
                raise ConflictError(
                    "undo_has_dependencies",
                    "反馈状态已经发生变化",
                    recovery="重新载入反馈中心",
                )
        undo_id = _id("commit")
        receipt = {
            "commitId": undo_id,
            "operation": "undo",
            "undoOfCommitId": commit_id,
            "projectId": project_id,
            "baseRevision": expected_revision,
            "resultRevision": expected_revision + 1,
            "undoneOperation": operation_kind,
            "artifactKind": artifact_kind,
            "restoredRevisionId": previous,
            "indexTargetRevision": expected_revision + 1,
        }
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (expected_revision + 1, expected_revision + 1, project_id),
        )
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, undo_of_commit_id, receipt_json, created_at) VALUES (?, ?, 'undo', ?, ?, ?, ?, ?, ?, ?)",
            (
                undo_id,
                project_id,
                idempotency_key,
                digest,
                expected_revision,
                expected_revision + 1,
                commit_id,
                _json(receipt),
                now(),
            ),
        )
        if operation_kind == "accept" and artifact_kind == "draft":
            connection.execute(
                "UPDATE planning_impacts SET state = 'withdrawn', resolved_at = ?, resolution_note = ?, resolved_by_commit_id = ? "
                "WHERE project_id = ? AND source_commit_id = ? AND state IN ('open', 'resolved-false-positive')",
                (now(), "来源正文采用已撤销", undo_id, project_id, commit_id),
            )
            connection.execute(
                "DELETE FROM author_fact_dependency_links WHERE project_id = ? AND created_by_commit_id = ?",
                (project_id, commit_id),
            )
        elif operation_kind == "accept" and artifact_kind in ARTIFACT_KINDS:
            connection.execute(
                "DELETE FROM planning_impacts WHERE project_id = ? AND created_by_commit_id = ? AND source_commit_id != ?",
                (project_id, commit_id, commit_id),
            )
            connection.execute(
                "UPDATE planning_impacts SET state = 'open', resolution_note = NULL, resolved_by_commit_id = NULL, resolved_at = NULL "
                "WHERE project_id = ? AND resolved_by_commit_id = ? AND state IN ('resolved-updated', 'superseded')",
                (project_id, commit_id),
            )
    try:
        refresh_search_index(db_path, project_id)
        with connect(db_path) as connection:
            connection.execute(
                "UPDATE project_workspace SET indexed_revision = MAX(indexed_revision, ?) WHERE project_id = ?",
                (receipt["resultRevision"], project_id),
            )
    except Exception:
        pass
    return receipt


def get_context_snapshot(
    db_path: str | Path,
    project_id: str,
    purpose: str,
    project_revision: int | None = None,
    story_time: int | None = None,
    reading_chapter: int | None = None,
    scene_order: int | None = None,
    pov_character_id: str | None = None,
    budget: int = 24,
    byte_budget: int = 24_000,
    actor: str = "author",
    task_id: str | None = None,
    thread_id: str | None = None,
) -> dict[str, Any]:
    if purpose not in {"planner", "writer", "critic"}:
        raise WorkspaceError("invalid_context_purpose", "Context 目的无效")
    if project_revision is not None and project_revision < 0:
        raise WorkspaceError("invalid_project_revision", "作品版本不能为负数")
    if purpose in {"writer", "critic"} and (
        reading_chapter is None or scene_order is None
    ):
        raise WorkspaceError(
            "reading_position_required", "Writer 与 Critic 必须指定阅读章节和场景顺序"
        )
    bounded_budget = min(max(int(budget), 1), 80)
    initialize(db_path)
    with connect(db_path) as connection:
        if actor == "candidate":
            connection.execute("BEGIN IMMEDIATE")
        _assert_actor(connection, project_id, actor, task_id, thread_id)
        project = _project(connection, project_id)
        task = None
        if actor == "candidate":
            task = _require_task(connection, project_id, task_id, thread_id)
            assert_task_purpose(task, purpose)
            target = task_target(task)
            if project_revision != task["input_revision"]:
                raise WorkspaceError(
                    "context_revision_mismatch",
                    "上下文版本与生成任务冻结版本不一致",
                    409,
                    {
                        "expectedProjectRevision": task["input_revision"],
                        "actualProjectRevision": project_revision,
                    },
                    "使用任务清单中的作品版本读取上下文",
                )
            if (
                target.get("chapterNumber") is not None
                and target["chapterNumber"] != reading_chapter
            ):
                raise WorkspaceError(
                    "context_target_mismatch",
                    "上下文阅读章节与生成目标不一致",
                    409,
                    {
                        "targetChapter": target["chapterNumber"],
                        "readingChapter": reading_chapter,
                    },
                    "按生成任务指定章节读取上下文",
                )
            if purpose in {"writer", "critic"}:
                mismatches = {}
                if target.get("sceneOrder") != scene_order:
                    mismatches["sceneOrder"] = {
                        "expected": target.get("sceneOrder"),
                        "actual": scene_order,
                    }
                if target.get("povCharacterId") != pov_character_id:
                    mismatches["povCharacterId"] = {
                        "expected": target.get("povCharacterId"),
                        "actual": pov_character_id,
                    }
                if target.get("storyTime") != story_time:
                    mismatches["storyTime"] = {
                        "expected": target.get("storyTime"),
                        "actual": story_time,
                    }
                if mismatches:
                    raise WorkspaceError(
                        "context_target_mismatch",
                        "上下文范围与生成任务冻结目标不一致",
                        409,
                        mismatches,
                        "按任务清单中的场景、视角和故事时间读取",
                    )
        elif project_revision is None:
            raise WorkspaceError(
                "context_revision_required",
                "作者读取上下文必须指定当前作品版本",
                409,
                recovery="读取当前工作区版本后重试",
            )
        workspace = _workspace(connection, project_id)
        if project_revision is not None and project_revision != workspace["revision"]:
            raise WorkspaceError(
                "context_revision_mismatch",
                "上下文版本必须等于当前作品版本",
                409,
                {
                    "requestedRevision": project_revision,
                    "currentRevision": workspace["revision"],
                },
                "使用当前作品版本读取上下文",
            )
        revision = (
            project_revision if project_revision is not None else workspace["revision"]
        )
        if task is not None and task["input_snapshot_id"]:
            stored = connection.execute(
                "SELECT id, purpose, content_json, content_hash, byte_size, created_at "
                "FROM task_input_snapshots WHERE id = ? AND project_id = ? AND task_id = ?",
                (task["input_snapshot_id"], project_id, task["id"]),
            ).fetchone()
            if stored is None or stored["purpose"] != purpose:
                raise WorkspaceError(
                    "input_snapshot_invalid",
                    "任务冻结输入不存在或用途不匹配",
                    409,
                    recovery="停止该任务并基于当前工作依据重新开始",
                )
            return {
                **_load(stored["content_json"], {}),
                "inputSnapshot": {
                    "id": stored["id"],
                    "contentHash": stored["content_hash"],
                    "byteSize": stored["byte_size"],
                    "createdAt": stored["created_at"],
                    "replayed": True,
                },
            }
        manifest = _load(task["input_manifest_json"], {}) if task is not None else {}
        frozen_draft_sources = {
            item.get("chapterId"): item.get("revisionId")
            for item in manifest.get("referenceDrafts", [])
            if item.get("chapterId") and item.get("revisionId")
        }
        target_manifest = manifest.get("target") or {}
        if target_manifest.get("chapterId") and manifest.get("baseTargetRevisionId"):
            frozen_draft_sources[target_manifest["chapterId"]] = manifest[
                "baseTargetRevisionId"
            ]
        clauses = ["project_id = ?", "project_revision <= ?"]
        parameters: list[Any] = [project_id, revision]
        if reading_chapter is not None:
            clauses.append("reading_chapter <= ?")
            parameters.append(reading_chapter)
        if story_time is not None:
            clauses.append("(story_time IS NULL OR story_time <= ?)")
            parameters.append(story_time)
        if scene_order is not None and reading_chapter is not None:
            clauses.append("(reading_chapter < ? OR scene_order <= ?)")
            parameters.extend([reading_chapter, scene_order])
        rows = connection.execute(
            f"SELECT * FROM temporal_state_events WHERE {' AND '.join(clauses)} ORDER BY project_revision DESC, reading_chapter DESC, scene_order DESC, id DESC",
            tuple(parameters),
        ).fetchall()
        selected: list[dict[str, Any]] = []
        forbidden: list[dict[str, Any]] = []
        seen: set[tuple[Any, ...]] = set()
        for row in rows:
            key = (
                row["fact_kind"],
                row["subject_id"],
                row["predicate"],
                row["epistemic_owner_id"],
            )
            if key in seen:
                continue
            seen.add(key)
            if (
                row["reverted_by_commit_id"]
                and row["epistemic_state"] == "unknown"
                and _load(row["value_json"], None) is None
            ):
                continue
            payload = {
                "id": row["id"],
                "factKind": row["fact_kind"],
                "subjectId": row["subject_id"],
                "predicate": row["predicate"],
                "value": _load(row["value_json"], None),
                "epistemicOwnerId": row["epistemic_owner_id"],
                "epistemicState": row["epistemic_state"],
                "storyTime": row["story_time"],
                "readingChapter": row["reading_chapter"],
                "sceneOrder": row["scene_order"],
                "evidence": row["evidence"],
                "sourceDraftRevisionId": row["source_draft_revision_id"],
                "authority": "acceptedFact",
            }
            if row["source_draft_revision_id"]:
                source_chapter = connection.execute(
                    "SELECT chapter_id FROM draft_revisions WHERE id = ? AND project_id = ?",
                    (row["source_draft_revision_id"], project_id),
                ).fetchone()
                working_revision_id = None
                if source_chapter:
                    if task is not None:
                        working_revision_id = frozen_draft_sources.get(
                            source_chapter["chapter_id"]
                        )
                    else:
                        selection = connection.execute(
                            "SELECT revision_id FROM working_baseline_selections "
                            "WHERE project_id = ? AND object_kind = 'draft' AND object_id = ?",
                            (project_id, source_chapter["chapter_id"]),
                        ).fetchone()
                        working_revision_id = (
                            selection["revision_id"] if selection else None
                        )
                if (
                    working_revision_id
                    and working_revision_id != row["source_draft_revision_id"]
                ):
                    payload["workingApplicability"] = "needsVerification"
                else:
                    payload["workingApplicability"] = "compatible"
            if purpose == "writer" and (
                row["fact_kind"] == "author-secret"
                or row["epistemic_owner_id"] not in {None, pov_character_id}
            ):
                if not forbidden:
                    forbidden.append({"policy": "不要让当前视角获得尚未公开的信息"})
                continue
            selected.append(payload)
        baseline = manifest.get("planningBaseline") or {}
        if task is not None and purpose == "planner":
            plan_revision_id = baseline.get("planRevisionId")
            plan_payload = (
                _load(
                    _artifact_row(connection, project_id, "plan", plan_revision_id)[
                        "payload_json"
                    ],
                    {},
                )
                if plan_revision_id
                else {}
            )
        else:
            plan_revision_id, plan_payload = _accepted_plan(connection, project_id)
        commitments = (
            plan_payload.get("chapters", []) if isinstance(plan_payload, dict) else []
        )
        planning_context = None
        if purpose == "writer":
            chapter_id = (
                task_target(task).get("chapterId") if task is not None else None
            )
            if chapter_id is None and reading_chapter is not None:
                chapter_row = connection.execute(
                    "SELECT id FROM chapters WHERE project_id = ? AND number = ?",
                    (project_id, reading_chapter),
                ).fetchone()
                chapter_id = chapter_row["id"] if chapter_row else None
            if task is not None and baseline.get("planRevisionId"):
                binding = {
                    "plan_revision_id": baseline["planRevisionId"],
                    "chapter_plan_id": baseline.get("chapterPlanId"),
                }
            elif task is not None:
                binding = None
            else:
                binding = (
                    connection.execute(
                        "SELECT plan_revision_id, chapter_plan_id FROM chapter_plan_bindings WHERE project_id = ? AND chapter_id = ?",
                        (project_id, chapter_id),
                    ).fetchone()
                    if chapter_id
                    else None
                )
                selected_plan_id, selected_plan_role = resolve_planning_revision(
                    connection, project_id, "plan"
                )
                if binding and selected_plan_role == "workingSelected":
                    binding = {
                        "plan_revision_id": selected_plan_id,
                        "chapter_plan_id": binding["chapter_plan_id"],
                    }
            if binding:
                plan_revision_id = binding["plan_revision_id"]
                plan_payload = _load(
                    _artifact_row(connection, project_id, "plan", plan_revision_id)[
                        "payload_json"
                    ],
                    {},
                )
                commitments = plan_payload.get("chapters", [])
            else:
                plan_revision_id = None
                plan_payload = {}
                commitments = []
            if plan_revision_id:
                outline_pointer = connection.execute(
                    "SELECT accepted_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'outline'",
                    (project_id,),
                ).fetchone()
                current_outline_id = (
                    outline_pointer["accepted_revision_id"] if outline_pointer else None
                )
                outline_id = plan_payload.get("sourceOutlineRevisionId")
                item = next(
                    (
                        entry
                        for entry in commitments
                        if binding
                        and entry.get("chapterPlanId") == binding["chapter_plan_id"]
                    ),
                    None,
                )
                commitments = (
                    []
                    if item is None
                    else [
                        {
                            "chapterPlanId": item["chapterPlanId"],
                            "publicTitle": item.get("publicTitle", ""),
                            "writerGuidance": item.get("writerGuidance", ""),
                            "publicCommitments": item.get("publicCommitments", []),
                            "povCharacterId": item.get("povCharacterId"),
                            "storyTime": item.get("storyTime"),
                        }
                    ]
                )
                outline_payload = (
                    _load(
                        _artifact_row(connection, project_id, "outline", outline_id)[
                            "payload_json"
                        ],
                        {},
                    )
                    if outline_id
                    else {}
                )
                node_map = {
                    node["id"]: node for node in outline_payload.get("nodes", [])
                }
                node = node_map.get(item.get("primaryNodeId")) if item else None
                nodes = []
                while node:
                    nodes.append(
                        {
                            "id": node["id"],
                            "kind": node["kind"],
                            "publicTitle": node.get("publicTitle", ""),
                            "writerGuidance": node.get("writerGuidance", ""),
                        }
                    )
                    node = node_map.get(node.get("parentId"))
                current_plan_id, _ = _accepted_plan(connection, project_id)
                planning_context = {
                    "lineageStatus": baseline.get("lineageStatus")
                    if task is not None
                    else "working"
                    if selected_plan_role == "workingSelected"
                    else "current"
                    if current_plan_id == plan_revision_id
                    and current_outline_id == outline_id
                    else "historical",
                    "outlineRevisionId": outline_id,
                    "planRevisionId": plan_revision_id,
                    "chapter": commitments[0] if commitments else None,
                    "outlinePath": list(reversed(nodes)),
                }
            else:
                commitments = []
        constraints = [
            {
                "id": row["id"],
                "statement": row["statement"],
                "severity": row["severity"],
                "source": "worldRule",
            }
            for row in connection.execute(
                "SELECT id, statement, severity FROM world_rules WHERE project_id = ? ORDER BY id",
                (project_id,),
            ).fetchall()
        ]
        planning_revision_ids: dict[str, str | None] = {}
        planning_source_roles: dict[str, str] = {}
        for kind in ARTIFACT_KINDS:
            if task is not None:
                planning_revision_ids[kind] = baseline.get(f"{kind}RevisionId")
                planning_source_roles[kind] = baseline.get(
                    f"{kind}SourceRole", "accepted"
                )
            else:
                revision_id, source_role = resolve_planning_revision(
                    connection, project_id, kind
                )
                planning_revision_ids[kind] = revision_id
                planning_source_roles[kind] = source_role
        if task is None:
            planner_artifact_kinds = set(ARTIFACT_KINDS)
        elif task["operation"] == "generateOutline":
            planner_artifact_kinds = {"brief", "outline"}
        else:
            planner_artifact_kinds = {
                "advisory": {"brief"},
                "generateBrief": {"brief"},
                "generatePlan": {"outline"},
            }.get(task["operation"], set())
        accepted_artifacts: dict[str, Any] = {}
        for kind in ARTIFACT_KINDS:
            source_revision_id = planning_revision_ids.get(kind)
            artifact_payload = (
                None
                if purpose == "writer" or kind not in planner_artifact_kinds
                else _revision_payload(
                    _artifact_row(connection, project_id, kind, source_revision_id)
                )
                if source_revision_id
                else None
            )
            accepted_artifacts[kind] = artifact_payload
        source_excerpts: list[dict[str, Any]] = []
        if (
            purpose == "planner"
            and task is not None
            and task["operation"] == "generatePlan"
        ):
            source_rows = []
            source_roles = {}
            for reference in manifest.get("referenceDrafts", []):
                row = connection.execute(
                    "SELECT c.id AS chapter_id, c.number, c.title, d.id AS revision_id, d.body, d.source "
                    "FROM chapters c JOIN draft_revisions d ON d.chapter_id = c.id AND d.project_id = c.project_id "
                    "WHERE c.project_id = ? AND c.id = ? AND d.id = ?",
                    (
                        project_id,
                        reference.get("chapterId"),
                        reference.get("revisionId"),
                    ),
                ).fetchone()
                if row:
                    source_rows.append(row)
                    source_roles[row["revision_id"]] = reference.get(
                        "sourceRole", "accepted"
                    )
            source_excerpts = [
                {
                    "chapterId": row["chapter_id"],
                    "chapterNumber": row["number"],
                    "chapterTitle": row["title"],
                    "draftRevisionId": row["revision_id"],
                    "source": row["source"],
                    "relationship": "acceptedDraft"
                    if source_roles[row["revision_id"]] == "accepted"
                    else "referenceDraft",
                    **(
                        {
                            "sourceRole": "workingSelected",
                            "evidenceClass": "required",
                        }
                        if source_roles[row["revision_id"]] == "workingSelected"
                        else {}
                    ),
                    "excerptRange": "tail",
                    "body": row["body"]
                    if source_roles[row["revision_id"]] == "workingSelected"
                    else row["body"][-6_000:],
                    "truncated": False
                    if source_roles[row["revision_id"]] == "workingSelected"
                    else len(row["body"]) > 6_000,
                }
                for row in source_rows
            ]
        elif purpose == "writer" and reading_chapter is not None:
            if task is not None:
                source_rows = []
                source_roles = {}
                for reference in manifest.get("referenceDrafts", []):
                    row = connection.execute(
                        "SELECT c.id AS chapter_id, c.number, c.title, d.id AS revision_id, d.body, d.source "
                        "FROM chapters c JOIN draft_revisions d ON d.chapter_id = c.id AND d.project_id = c.project_id "
                        "WHERE c.project_id = ? AND c.id = ? AND d.id = ?",
                        (
                            project_id,
                            reference.get("chapterId"),
                            reference.get("revisionId"),
                        ),
                    ).fetchone()
                    if row:
                        source_rows.append(row)
                        source_roles[row["revision_id"]] = reference.get(
                            "sourceRole", "accepted"
                        )
            else:
                source_rows = []
                source_roles = {}
                chapter_rows = connection.execute(
                    "SELECT id, number FROM chapters WHERE project_id = ? AND number < ? "
                    "ORDER BY number DESC LIMIT 6",
                    (project_id, reading_chapter),
                ).fetchall()
                for chapter_row in chapter_rows:
                    revision_id, source_role = resolve_draft_revision(
                        connection, project_id, chapter_row["id"]
                    )
                    if not revision_id:
                        continue
                    row = connection.execute(
                        "SELECT c.id AS chapter_id, c.number, c.title, d.id AS revision_id, d.body, d.source "
                        "FROM chapters c JOIN draft_revisions d ON d.chapter_id = c.id AND d.project_id = c.project_id "
                        "WHERE c.project_id = ? AND c.id = ? AND d.id = ?",
                        (project_id, chapter_row["id"], revision_id),
                    ).fetchone()
                    if row:
                        source_rows.append(row)
                        source_roles[row["revision_id"]] = source_role
            source_excerpts = [
                {
                    "chapterId": row["chapter_id"],
                    "chapterNumber": row["number"],
                    "chapterTitle": row["title"],
                    "draftRevisionId": row["revision_id"],
                    "source": row["source"],
                    "relationship": "referenceDraft",
                    "sourceRole": source_roles[row["revision_id"]],
                    **(
                        {
                            "evidenceClass": "required",
                        }
                        if source_roles[row["revision_id"]] == "workingSelected"
                        else {}
                    ),
                    "body": row["body"]
                    if source_roles[row["revision_id"]] == "workingSelected"
                    else row["body"][:12_000],
                    "truncated": False
                    if source_roles[row["revision_id"]] == "workingSelected"
                    else len(row["body"]) > 12_000,
                }
                for row in source_rows
            ]
            target_chapter_id = (
                task_target(task).get("chapterId") if task is not None else None
            )
            if target_chapter_id is None:
                target_row = connection.execute(
                    "SELECT id FROM chapters WHERE project_id = ? AND number = ?",
                    (project_id, reading_chapter),
                ).fetchone()
                target_chapter_id = target_row["id"] if target_row else None
            if target_chapter_id:
                if task is not None:
                    target_revision_id = manifest.get("baseTargetRevisionId")
                    target_source_role = manifest.get(
                        "baseTargetSourceRole", "accepted"
                    )
                else:
                    target_revision_id, target_source_role = resolve_draft_revision(
                        connection, project_id, target_chapter_id
                    )
                target_draft = (
                    connection.execute(
                        "SELECT id, body, scene_groups_json, source FROM draft_revisions WHERE id = ? AND project_id = ? AND chapter_id = ?",
                        (target_revision_id, project_id, target_chapter_id),
                    ).fetchone()
                    if target_revision_id
                    else None
                )
                if target_draft:
                    target_body = target_draft["body"]
                    target_groups = _load(target_draft["scene_groups_json"], [])
                    visible_groups = target_groups
                    scope_truncated = False
                    if (
                        task is not None
                        and target.get("scope", {}).get("type") == "throughScene"
                    ):
                        visible_groups = target_groups[: target["sceneOrder"]]
                        target_body = SCENE_SEPARATOR.join(
                            str(group.get("body", "")).strip()
                            for group in visible_groups
                        )
                        scope_truncated = len(visible_groups) < len(target_groups)
                    source_excerpts.insert(
                        0,
                        {
                            "chapterId": target_chapter_id,
                            "chapterNumber": reading_chapter,
                            "chapterTitle": target.get("chapterTitle", "")
                            if task is not None
                            else "",
                            "draftRevisionId": target_draft["id"],
                            "source": target_draft["source"],
                            "relationship": "targetDraft",
                            "sourceRole": target_source_role,
                            "evidenceClass": "required",
                            "sceneGroups": [
                                {
                                    "id": group["id"],
                                    "sceneId": group.get("sceneId"),
                                    "sceneOrder": index + 1,
                                    "locked": group.get("locked", False),
                                }
                                for index, group in enumerate(visible_groups)
                            ],
                            "body": target_body,
                            "truncated": scope_truncated,
                        },
                    )
        omitted = max(0, len(selected) - bounded_budget)
        required_evidence = [
            {
                "type": "authorIntent",
                "id": task["id"],
                "reason": "作者对本次任务的原始要求",
            }
            for _ in [0]
            if task is not None and manifest.get("authorIntent")
        ]
        required_evidence.extend(
            {
                "type": "hardConstraint",
                "id": constraint["id"],
                "reason": "正式世界规则",
            }
            for constraint in constraints
        )
        required_evidence.extend(
            {
                "type": "planningArtifact",
                "id": artifact["id"],
                "reason": "任务冻结的规划依据",
            }
            for artifact in accepted_artifacts.values()
            if artifact is not None
        )
        required_evidence.extend(
            {
                "type": "draftExcerpt",
                "id": excerpt["draftRevisionId"],
                "reason": "作者明确选用的工作稿"
                if excerpt.get("sourceRole") == "workingSelected"
                else "当前编辑对象",
            }
            for excerpt in source_excerpts
            if excerpt.get("evidenceClass") == "required"
        )
        source_fingerprint = hashlib.sha256(
            _json(
                {
                    "purpose": purpose,
                    "storyTime": story_time,
                    "readingChapter": reading_chapter,
                    "sceneOrder": scene_order,
                    "povCharacterId": pov_character_id,
                    "planningContext": planning_context,
                    "sources": [
                        {
                            "chapterId": item.get("chapterId"),
                            "revisionId": item.get("draftRevisionId"),
                            "relationship": item.get("relationship"),
                            "sourceRole": item.get("sourceRole", "accepted"),
                        }
                        for item in source_excerpts
                    ],
                }
            ).encode("utf-8")
        ).hexdigest()
        snapshot = {
            "projectId": project_id,
            "projectRevision": revision,
            "purpose": purpose,
            "storyTime": story_time,
            "readingChapter": reading_chapter,
            "sceneOrder": scene_order,
            "povCharacterId": pov_character_id,
            "authorIntent": manifest.get("authorIntent") if task is not None else None,
            "facts": selected[:bounded_budget],
            "forbidden": forbidden[:bounded_budget],
            "hardConstraints": constraints,
            "acceptedArtifacts": accepted_artifacts,
            "planRevisionId": plan_revision_id,
            "commitments": commitments[:bounded_budget],
            "planningContext": planning_context,
            "sourceExcerpts": source_excerpts,
            "sourceFingerprint": source_fingerprint,
            "evidenceCoverage": {
                "required": required_evidence,
                "supportingCount": sum(
                    1
                    for excerpt in source_excerpts
                    if excerpt.get("evidenceClass") == "supporting"
                )
                + len(selected[:bounded_budget]),
            },
            "omissions": {
                "factCount": omitted,
                "message": f"因条目预算省略 {omitted} 条事实"
                if omitted
                else "未因条目预算省略事实",
            },
            "budget": bounded_budget,
            "watermark": {
                "projectRevision": revision,
                "indexedRevision": workspace["indexed_revision"],
                "indexTargetRevision": workspace["index_target_revision"],
            },
        }
        if purpose == "planner":
            snapshot["project"] = {
                "title": project["title"],
                "logline": project["logline"],
            }
        fitted = fit_context_snapshot(snapshot, byte_budget)
        if task is None:
            return fitted
        encoded_snapshot = _json(fitted)
        snapshot_id = _id("input-snapshot")
        content_hash = hashlib.sha256(encoded_snapshot.encode("utf-8")).hexdigest()
        created_at = now()
        connection.execute(
            "INSERT INTO task_input_snapshots(id, project_id, task_id, purpose, content_json, content_hash, byte_size, created_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                snapshot_id,
                project_id,
                task["id"],
                purpose,
                encoded_snapshot,
                content_hash,
                len(encoded_snapshot.encode("utf-8")),
                created_at,
            ),
        )
        connection.execute(
            "UPDATE task_runs SET input_snapshot_id = ? WHERE id = ?",
            (snapshot_id, task["id"]),
        )
        return {
            **fitted,
            "inputSnapshot": {
                "id": snapshot_id,
                "contentHash": content_hash,
                "byteSize": len(encoded_snapshot.encode("utf-8")),
                "createdAt": created_at,
                "replayed": False,
            },
        }
