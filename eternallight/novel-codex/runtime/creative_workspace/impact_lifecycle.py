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


def _load(value: str | None, fallback: Any) -> Any:
    return json.loads(value) if value else fallback


def _id(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex}"


def _impact_payload(
    row: dict[str, Any], predicate: str, evidence: str
) -> dict[str, Any]:
    return {
        "id": row["id"],
        "impactChainId": row["impact_chain_id"],
        "supersedesImpactId": row.get("supersedes_impact_id"),
        "sourceCommitId": row["source_commit_id"],
        "sourceDraftRevisionId": row["source_draft_revision_id"],
        "sourceStateEventId": row["source_state_event_id"],
        "targetArtifactKind": row["target_artifact_kind"],
        "targetRevisionId": row["target_revision_id"],
        "targetNodeId": row["target_node_id"],
        "targetLabel": row["target_label"],
        "dependencyKey": row["dependency_key"],
        "severity": row["severity"],
        "state": row["state"],
        "resolutionNote": row.get("resolution_note"),
        "status": row["severity"],
        "reason": f"新确认事实“{predicate}”命中规划依赖“{row['dependency_key']}”",
        "evidence": evidence,
        "sourceChapterNumber": row.get("source_chapter_number"),
        "sourceChapterTitle": row.get("source_chapter_title"),
        "sourceChapterId": row.get("source_chapter_id"),
    }


def list_open_planning_impacts(connection, project_id: str) -> list[dict[str, Any]]:
    rows = connection.execute(
        "SELECT impact.*, state_event.predicate, state_event.evidence, chapter.id AS source_chapter_id, chapter.number AS source_chapter_number, chapter.title AS source_chapter_title FROM planning_impacts impact LEFT JOIN temporal_state_events state_event ON state_event.id = impact.source_state_event_id LEFT JOIN draft_revisions draft ON draft.id = impact.source_draft_revision_id LEFT JOIN chapters chapter ON chapter.id = draft.chapter_id WHERE impact.project_id = ? AND impact.state = 'open' ORDER BY CASE impact.severity WHEN 'locked-conflict' THEN 0 ELSE 1 END, impact.created_at DESC, impact.id LIMIT 50",
        (project_id,),
    ).fetchall()
    return [
        _impact_payload(
            dict(row),
            row["predicate"] or row["dependency_key"] or "已确认事实",
            row["evidence"] or "",
        )
        for row in rows
    ]


def planning_impact_page(
    db_path: str | Path, project_id: str, limit: int = 25, cursor: str | None = None
) -> dict[str, Any]:
    bounded_limit = min(max(limit, 1), 50)
    try:
        offset = int(cursor or "0")
    except ValueError as error:
        raise WorkspaceError("invalid_impact_cursor", "规划反馈 cursor 无效") from error
    if offset < 0:
        raise WorkspaceError("invalid_impact_cursor", "规划反馈 cursor 无效")
    initialize(db_path)
    with connect(db_path) as connection:
        counts = connection.execute(
            "SELECT COUNT(*) AS total, "
            "SUM(CASE WHEN state = 'open' THEN 1 ELSE 0 END) AS open_count, "
            "SUM(CASE WHEN state = 'resolved-kept' THEN 1 ELSE 0 END) AS retained_count, "
            "SUM(CASE WHEN state = 'open' AND severity = 'locked-conflict' THEN 1 ELSE 0 END) AS locked_count "
            "FROM planning_impacts WHERE project_id = ? AND state IN ('open', 'resolved-kept')",
            (project_id,),
        ).fetchone()
        rows = connection.execute(
            "SELECT impact.*, state_event.predicate, state_event.evidence, chapter.id AS source_chapter_id, chapter.number AS source_chapter_number, chapter.title AS source_chapter_title FROM planning_impacts impact "
            "LEFT JOIN temporal_state_events state_event ON state_event.id = impact.source_state_event_id "
            "LEFT JOIN draft_revisions draft ON draft.id = impact.source_draft_revision_id "
            "LEFT JOIN chapters chapter ON chapter.id = draft.chapter_id "
            "WHERE impact.project_id = ? AND impact.state IN ('open', 'resolved-kept') "
            "ORDER BY CASE impact.state WHEN 'open' THEN 0 ELSE 1 END, CASE impact.severity WHEN 'locked-conflict' THEN 0 ELSE 1 END, impact.created_at DESC, impact.id LIMIT ? OFFSET ?",
            (project_id, bounded_limit, offset),
        ).fetchall()
        data = [
            _impact_payload(
                dict(row),
                row["predicate"] or row["dependency_key"] or "已确认事实",
                row["evidence"] or "",
            )
            for row in rows
        ]
        total = counts["total"]
        next_offset = offset + len(data)
        return {
            "data": data,
            "nextCursor": str(next_offset) if next_offset < total else None,
            "counts": {
                "total": total,
                "open": counts["open_count"] or 0,
                "retained": counts["retained_count"] or 0,
                "lockedConflict": counts["locked_count"] or 0,
            },
        }


def _planning_node(
    connection, project_id: str, kind: str, revision_id: str | None, node_id: str | None
) -> dict[str, Any] | None:
    if not revision_id or not node_id or kind not in {"outline", "plan"}:
        return None
    row = connection.execute(
        "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ? AND artifact_kind = ?",
        (revision_id, project_id, kind),
    ).fetchone()
    if row is None:
        return None
    payload = _load(row["payload_json"], {})
    entries = (
        payload.get("nodes", []) if kind == "outline" else payload.get("chapters", [])
    )
    key = "id" if kind == "outline" else "chapterPlanId"
    return next((entry for entry in entries if entry.get(key) == node_id), None)


def link_planning_impacts(
    connection,
    project_id: str,
    kind: str,
    candidate_revision_id: str,
    candidate_payload: dict[str, Any],
    impact_ids: list[str],
) -> list[str]:
    if kind not in {"outline", "plan"}:
        if impact_ids:
            raise WorkspaceError(
                "invalid_impact_link", "只有大纲或章纲候选可以处理规划反馈"
            )
        return []
    unique_ids = list(dict.fromkeys(impact_ids))
    if len(unique_ids) > 50 or any(
        not isinstance(item, str) or not item for item in unique_ids
    ):
        raise WorkspaceError("invalid_impact_link", "规划反馈链接无效")
    linked: list[str] = []
    entries = (
        candidate_payload.get("nodes", [])
        if kind == "outline"
        else candidate_payload.get("chapters", [])
    )
    key = "id" if kind == "outline" else "chapterPlanId"
    next_nodes = {entry.get(key): entry for entry in entries if isinstance(entry, dict)}
    for impact_id in unique_ids:
        impact = connection.execute(
            "SELECT * FROM planning_impacts WHERE id = ? AND project_id = ?",
            (impact_id, project_id),
        ).fetchone()
        if impact is None:
            raise NotFoundError("规划反馈不存在")
        if impact["state"] != "open" or impact["target_artifact_kind"] != kind:
            raise ConflictError("invalid_impact_link", "候选与待处理反馈不匹配")
        previous_node = _planning_node(
            connection,
            project_id,
            kind,
            impact["target_revision_id"],
            impact["target_node_id"],
        )
        next_node = next_nodes.get(impact["target_node_id"])
        if previous_node is None or next_node is None:
            raise ConflictError(
                "impact_target_missing", "候选没有保留反馈对应的稳定规划节点"
            )
        if _json(previous_node) == _json(next_node):
            raise ConflictError(
                "impact_target_unchanged", "候选没有实际修改这条反馈对应的节点"
            )
        connection.execute(
            "INSERT INTO planning_impact_links(impact_id, candidate_revision_id, linked_by_author_at) VALUES (?, ?, ?) "
            "ON CONFLICT(impact_id) DO UPDATE SET candidate_revision_id = excluded.candidate_revision_id, linked_by_author_at = excluded.linked_by_author_at",
            (impact_id, candidate_revision_id, now()),
        )
        linked.append(impact_id)
    return linked


def advance_planning_impacts(
    connection,
    project_id: str,
    kind: str,
    previous_revision_id: str | None,
    candidate_revision_id: str,
    candidate_payload: dict[str, Any],
    commit_id: str,
) -> dict[str, list[str]]:
    """Move every open target to the newly accepted planning revision."""
    if kind not in {"outline", "plan"} or not previous_revision_id:
        return {"resolved": [], "migrated": []}
    entries = (
        candidate_payload.get("nodes", [])
        if kind == "outline"
        else candidate_payload.get("chapters", [])
    )
    key = "id" if kind == "outline" else "chapterPlanId"
    next_nodes = {entry.get(key): entry for entry in entries if isinstance(entry, dict)}
    rows = connection.execute(
        "SELECT impact.*, link.candidate_revision_id AS linked_candidate_revision_id "
        "FROM planning_impacts impact LEFT JOIN planning_impact_links link ON link.impact_id = impact.id "
        "WHERE impact.project_id = ? AND impact.target_artifact_kind = ? AND impact.state = 'open'",
        (project_id, kind),
    ).fetchall()
    resolved: list[str] = []
    migrated: list[str] = []
    for impact in rows:
        next_node = next_nodes.get(impact["target_node_id"])
        if next_node is None:
            raise ConflictError(
                "open_impact_target_removed",
                "新规划删除了仍有待处理反馈的节点",
            )
        if impact["linked_candidate_revision_id"] == candidate_revision_id:
            connection.execute(
                "UPDATE planning_impacts SET state = 'resolved-updated', resolution_note = '已由链接候选修改并采用', resolved_by_commit_id = ?, resolved_at = ? WHERE id = ?",
                (commit_id, now(), impact["id"]),
            )
            resolved.append(impact["id"])
            continue
        successor_id = _id("planning-impact")
        connection.execute(
            "UPDATE planning_impacts SET state = 'superseded', resolution_note = '目标规划版本已前进', resolved_by_commit_id = ?, resolved_at = ? WHERE id = ?",
            (commit_id, now(), impact["id"]),
        )
        connection.execute(
            "INSERT INTO planning_impacts(id, impact_chain_id, supersedes_impact_id, project_id, source_commit_id, source_draft_revision_id, source_state_event_id, source_planning_revision_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, severity, state, created_by_commit_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?, ?)",
            (
                successor_id,
                impact["impact_chain_id"],
                impact["id"],
                project_id,
                impact["source_commit_id"],
                impact["source_draft_revision_id"],
                impact["source_state_event_id"],
                impact["source_planning_revision_id"],
                kind,
                candidate_revision_id,
                impact["target_node_id"],
                next_node.get("publicTitle")
                or next_node.get("title")
                or impact["target_label"],
                impact["dependency_key"],
                impact["severity"],
                commit_id,
                now(),
            ),
        )
        migrated.append(successor_id)
    return {"resolved": resolved, "migrated": migrated}


def assert_open_impact_targets_preserved(
    connection,
    project_id: str,
    kind: str,
    previous_revision_id: str | None,
    next_payload: dict[str, Any],
) -> None:
    if kind not in {"outline", "plan"}:
        return
    rows = connection.execute(
        "SELECT id, target_node_id, target_label FROM planning_impacts "
        "WHERE project_id = ? AND target_artifact_kind = ? AND state = 'open'",
        (project_id, kind),
    ).fetchall()
    if not rows:
        return
    entries = (
        next_payload.get("nodes", [])
        if kind == "outline"
        else next_payload.get("chapters", [])
    )
    key = "id" if kind == "outline" else "chapterPlanId"
    next_ids = {entry.get(key) for entry in entries}
    missing = [
        {
            "impactId": row["id"],
            "targetNodeId": row["target_node_id"],
            "targetLabel": row["target_label"],
        }
        for row in rows
        if row["target_node_id"] not in next_ids
    ]
    if missing:
        raise ConflictError(
            "open_impact_target_removed",
            "新规划删除了仍有待处理反馈的节点",
            {"targets": missing},
            "保留稳定节点并修改内容，或先明确将反馈标为保留原规划",
        )


def resolve_planning_impact(
    db_path: str | Path,
    project_id: str,
    impact_id: str,
    expected_revision: int,
    resolution: str,
    note: str,
    idempotency_key: str,
) -> dict[str, Any]:
    if resolution not in {"kept", "false-positive"}:
        raise WorkspaceError("invalid_impact_resolution", "反馈处理方式无效")
    if resolution in {"kept", "false-positive"} and not note.strip():
        raise WorkspaceError(
            "impact_resolution_note_required", "保留原规划或标记误报时必须说明作者决定"
        )
    if len(note) > 2_000 or not idempotency_key.strip() or len(idempotency_key) > 160:
        raise WorkspaceError("invalid_impact_resolution", "反馈处理说明或幂等键无效")
    request_hash = hashlib.sha256(
        _json(
            {
                "impactId": impact_id,
                "expectedRevision": expected_revision,
                "resolution": resolution,
                "note": note,
            }
        ).encode()
    ).hexdigest()
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        existing = connection.execute(
            "SELECT request_hash, receipt_json FROM acceptance_commits WHERE project_id = ? AND operation_kind = 'resolve-impact' AND idempotency_key = ?",
            (project_id, idempotency_key),
        ).fetchone()
        if existing:
            if existing["request_hash"] != request_hash:
                raise ConflictError("idempotency_conflict", "幂等键已用于其他反馈决定")
            return _load(existing["receipt_json"], {})
        workspace = connection.execute(
            "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
        ).fetchone()
        if workspace is None:
            raise NotFoundError("作品工作区不存在")
        if workspace["revision"] != expected_revision:
            raise ConflictError(
                "stale_project_revision",
                "作品版本已变化",
                {"currentRevision": workspace["revision"]},
            )
        impact = connection.execute(
            "SELECT * FROM planning_impacts WHERE id = ? AND project_id = ?",
            (impact_id, project_id),
        ).fetchone()
        if impact is None:
            raise NotFoundError("规划反馈不存在")
        if impact["state"] != "open":
            raise ConflictError(
                "impact_already_resolved",
                "规划反馈已经处理",
                {"state": impact["state"]},
            )
        commit_id = _id("commit")
        result_revision = expected_revision + 1
        state = {
            "kept": "resolved-kept",
            "false-positive": "resolved-false-positive",
        }[resolution]
        receipt = {
            "commitId": commit_id,
            "operation": "resolve-impact",
            "projectId": project_id,
            "impactId": impact_id,
            "resolution": resolution,
            "note": note,
            "baseRevision": expected_revision,
            "resultRevision": result_revision,
        }
        connection.execute(
            "INSERT INTO acceptance_commits(id, project_id, operation_kind, idempotency_key, request_hash, base_revision, result_revision, receipt_json, created_at) VALUES (?, ?, 'resolve-impact', ?, ?, ?, ?, ?, ?)",
            (
                commit_id,
                project_id,
                idempotency_key,
                request_hash,
                expected_revision,
                result_revision,
                _json(receipt),
                now(),
            ),
        )
        connection.execute(
            "UPDATE planning_impacts SET state = ?, resolution_note = ?, resolved_by_commit_id = ?, resolved_at = ? WHERE id = ?",
            (state, note, commit_id, now(), impact_id),
        )
        connection.execute(
            "UPDATE project_workspace SET revision = ?, index_target_revision = ? WHERE project_id = ?",
            (result_revision, result_revision, project_id),
        )
        connection.execute(
            "UPDATE projects SET updated_at = ? WHERE id = ?", (now(), project_id)
        )
    return receipt
