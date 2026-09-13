import hashlib
import json
from typing import Any

from .errors import ConflictError, WorkspaceError


def _load(value: str | None, fallback: Any) -> Any:
    return json.loads(value) if value else fallback


def _planning_targets(
    connection, project_id: str, accepted_chapter: int
) -> tuple[list[dict[str, Any]], str | None]:
    pointers = {
        row["artifact_kind"]: row["accepted_revision_id"]
        for row in connection.execute(
            "SELECT artifact_kind, accepted_revision_id FROM artifact_pointers WHERE project_id = ?",
            (project_id,),
        ).fetchall()
    }
    targets: list[dict[str, Any]] = []
    plan_revision_id = pointers.get("plan")
    if plan_revision_id:
        row = connection.execute(
            "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ?",
            (plan_revision_id, project_id),
        ).fetchone()
        plan = _load(row["payload_json"], {}) if row else {}
        if plan.get("sourceOutlineRevisionId") == pointers.get("outline"):
            for chapter in plan.get("chapters", []):
                chapter_number = chapter.get("chapterNumber")
                if (
                    isinstance(chapter_number, int)
                    and chapter_number > accepted_chapter
                ):
                    targets.append(
                        {
                            "kind": "plan",
                            "revisionId": plan_revision_id,
                            "nodeId": chapter.get("chapterPlanId"),
                            "label": chapter.get("publicTitle")
                            or chapter.get("title")
                            or f"第 {chapter_number} 章",
                            "dependencyKeys": chapter.get("dependencyKeys", []),
                            "protected": chapter.get("locked") is True,
                        }
                    )
    outline_revision_id = pointers.get("outline")
    if outline_revision_id:
        row = connection.execute(
            "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ?",
            (outline_revision_id, project_id),
        ).fetchone()
        outline = _load(row["payload_json"], {}) if row else {}
        for node in outline.get("nodes", []):
            targets.append(
                {
                    "kind": "outline",
                    "revisionId": outline_revision_id,
                    "nodeId": node.get("id"),
                    "label": node.get("publicTitle")
                    or node.get("title")
                    or node.get("kind", "大纲节点"),
                    "dependencyKeys": node.get("dependencyKeys", []),
                    "protected": node.get("protection", "free") != "free",
                }
            )
    return targets, plan_revision_id


def _preview_id(fact_id: str, target: dict[str, Any], dependency_key: str) -> str:
    digest = hashlib.sha256(
        f"{fact_id}\0{target['kind']}\0{target['revisionId']}\0{target['nodeId']}\0{dependency_key}".encode()
    ).hexdigest()[:32]
    return f"provisional-impact-{digest}"


def preview_planning_impacts(
    connection,
    project_id: str,
    facts: list[dict[str, Any]],
    accepted_chapter: int,
    manual_links: list[dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    targets, _ = _planning_targets(connection, project_id, accepted_chapter)
    target_by_identity = {
        (target["kind"], target["revisionId"], target["nodeId"]): target
        for target in targets
    }
    for link in manual_links or []:
        identity = (
            link["targetArtifactKind"],
            link["targetRevisionId"],
            link["targetNodeId"],
        )
        if identity in target_by_identity:
            continue
        node = _planning_node(connection, project_id, *identity)
        if node is None:
            continue
        target = {
            "kind": identity[0],
            "revisionId": identity[1],
            "nodeId": identity[2],
            "label": node.get("publicTitle")
            or node.get("title")
            or node.get("kind", "规划节点"),
            "dependencyKeys": node.get("dependencyKeys", []),
            "protected": node.get("locked") is True
            if identity[0] == "plan"
            else node.get("protection", "free") != "free",
        }
        targets.append(target)
        target_by_identity[identity] = target
    manual_by_fact: dict[str, list[dict[str, Any]]] = {}
    for link in manual_links or []:
        target = target_by_identity.get(
            (
                link["targetArtifactKind"],
                link["targetRevisionId"],
                link["targetNodeId"],
            )
        )
        if target is not None:
            manual_by_fact.setdefault(link["factProposalId"], []).append(target)
    previews: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for fact in facts:
        predicate = fact["predicate"]
        for target in targets:
            matched_by = (
                "dependency-key" if predicate in target["dependencyKeys"] else None
            )
            if target in manual_by_fact.get(fact["id"], []):
                matched_by = "author-link"
            if matched_by is None or not isinstance(target["nodeId"], str):
                continue
            identity = (fact["id"], target["kind"], target["nodeId"])
            if identity in seen:
                continue
            seen.add(identity)
            previews.append(
                {
                    "id": _preview_id(fact["id"], target, predicate),
                    "factProposalId": fact["id"],
                    "predicate": predicate,
                    "evidence": fact.get("evidence", ""),
                    "targetArtifactKind": target["kind"],
                    "targetRevisionId": target["revisionId"],
                    "targetNodeId": target["nodeId"],
                    "targetLabel": target["label"],
                    "dependencyKey": predicate,
                    "severity": "locked-conflict"
                    if target["protected"]
                    else "affected",
                    "matchedBy": matched_by,
                }
            )
    return previews


def validate_author_fact_dependency_links(
    connection,
    project_id: str,
    facts: list[dict[str, Any]],
    links: Any,
) -> list[dict[str, Any]]:
    if links is None:
        return []
    if not isinstance(links, list) or len(links) > 100:
        raise WorkspaceError("invalid_fact_dependency_link", "事实与规划连接无效")
    fact_ids = {fact["id"] for fact in facts}
    pointers = {
        row["artifact_kind"]: row["accepted_revision_id"]
        for row in connection.execute(
            "SELECT artifact_kind, accepted_revision_id FROM artifact_pointers WHERE project_id = ?",
            (project_id,),
        ).fetchall()
    }
    normalized: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for link in links:
        if not isinstance(link, dict) or set(link) != {
            "factProposalId",
            "targetArtifactKind",
            "targetRevisionId",
            "targetNodeId",
        }:
            raise WorkspaceError(
                "invalid_fact_dependency_link", "事实与规划连接字段无效"
            )
        fact_id = link["factProposalId"]
        kind = link["targetArtifactKind"]
        revision_id = link["targetRevisionId"]
        node_id = link["targetNodeId"]
        if (
            fact_id not in fact_ids
            or kind not in {"outline", "plan"}
            or pointers.get(kind) != revision_id
            or not all(
                isinstance(value, str) and value for value in (revision_id, node_id)
            )
            or _planning_node(connection, project_id, kind, revision_id, node_id)
            is None
        ):
            raise ConflictError(
                "invalid_fact_dependency_link",
                "事实只能连接到当前正式规划中的稳定节点",
            )
        identity = (fact_id, kind, node_id)
        if identity not in seen:
            seen.add(identity)
            normalized.append(dict(link))
    return normalized


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
