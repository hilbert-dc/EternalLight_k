import hashlib
from typing import Any
from uuid import uuid4

from .metrics import record

try:
    from ..storage import now
except ImportError:
    from storage import now


from .impact_lifecycle import (
    advance_planning_impacts,
    assert_open_impact_targets_preserved,
    link_planning_impacts,
    list_open_planning_impacts,
    planning_impact_page,
    resolve_planning_impact,
)
from .impact_review import (
    _planning_node,
    _planning_targets,
    _preview_id,
    preview_planning_impacts,
    validate_author_fact_dependency_links,
)


def _id(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex}"


def _chain_id(
    project_id: str,
    source_state_event_id: str,
    target_kind: str,
    target_id: str,
    dependency_key: str,
) -> str:
    digest = hashlib.sha256(
        f"{project_id}\0{source_state_event_id}\0{target_kind}\0{target_id}\0{dependency_key}".encode()
    ).hexdigest()[:32]
    return f"impact-chain-{digest}"


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


def record_planning_impacts(
    connection,
    project_id: str,
    commit_id: str,
    draft_revision_id: str,
    confirmed_facts: list[dict[str, Any]],
    accepted_chapter: int,
    direction_decisions: dict[str, Any] | None = None,
    manual_links: list[dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    if not confirmed_facts:
        return []
    source_chapter = connection.execute(
        "SELECT id, number, title FROM chapters WHERE project_id = ? AND number = ?",
        (project_id, accepted_chapter),
    ).fetchone()
    targets, plan_revision_id = _planning_targets(
        connection, project_id, accepted_chapter
    )
    target_identities = {
        (target["kind"], target["revisionId"], target["nodeId"]) for target in targets
    }
    for link in manual_links or []:
        identity = (
            link["targetArtifactKind"],
            link["targetRevisionId"],
            link["targetNodeId"],
        )
        if identity in target_identities:
            continue
        node = _planning_node(connection, project_id, *identity)
        if node is None:
            continue
        targets.append(
            {
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
        )
        target_identities.add(identity)
    manual_identities = {
        (
            link["factProposalId"],
            link["targetArtifactKind"],
            link["targetRevisionId"],
            link["targetNodeId"],
        )
        for link in manual_links or []
    }
    created: list[dict[str, Any]] = []
    for fact in confirmed_facts:
        predicate = fact["predicate"]
        evidence = fact.get("evidence", "")
        for target in targets:
            if not isinstance(target["nodeId"], str):
                continue
            auto_match = predicate in target["dependencyKeys"]
            manual_match = (
                fact["id"],
                target["kind"],
                target["revisionId"],
                target["nodeId"],
            ) in manual_identities
            matches = [predicate] if auto_match or manual_match else []
            for dependency_key in matches:
                chain_id = _chain_id(
                    project_id,
                    fact["eventId"],
                    target["kind"],
                    target["nodeId"],
                    dependency_key,
                )
                same_revision = connection.execute(
                    "SELECT * FROM planning_impacts WHERE project_id = ? AND impact_chain_id = ? AND target_revision_id = ?",
                    (project_id, chain_id, target["revisionId"]),
                ).fetchone()
                if same_revision:
                    if same_revision["state"] == "open":
                        created.append(
                            _impact_payload(dict(same_revision), predicate, evidence)
                        )
                    continue
                previous = None
                impact_id = _id("planning-impact")
                severity = "locked-conflict" if target["protected"] else "affected"
                preview_id = _preview_id(fact["id"], target, dependency_key)
                direction = (direction_decisions or {}).get(preview_id)
                state = "open"
                resolution_note = None
                resolved_at = None
                if severity == "locked-conflict" and direction:
                    decision = (
                        direction
                        if isinstance(direction, str)
                        else direction.get("decision")
                    )
                    if decision == "false-positive":
                        state = "resolved-false-positive"
                        resolution_note = (
                            direction.get("note")
                            if isinstance(direction, dict)
                            else None
                        )
                        resolved_at = now()
                row = {
                    "id": impact_id,
                    "impact_chain_id": chain_id,
                    "source_commit_id": commit_id,
                    "source_draft_revision_id": draft_revision_id,
                    "source_state_event_id": fact["eventId"],
                    "target_artifact_kind": target["kind"],
                    "target_revision_id": target["revisionId"],
                    "target_node_id": target["nodeId"],
                    "target_label": target["label"],
                    "dependency_key": dependency_key,
                    "severity": severity,
                    "state": state,
                    "source_chapter_number": accepted_chapter,
                    "source_chapter_title": source_chapter["title"]
                    if source_chapter
                    else None,
                    "source_chapter_id": source_chapter["id"]
                    if source_chapter
                    else None,
                }
                connection.execute(
                    "INSERT INTO planning_impacts(id, impact_chain_id, supersedes_impact_id, project_id, source_commit_id, source_draft_revision_id, source_state_event_id, source_planning_revision_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, severity, state, resolution_note, resolved_by_commit_id, created_by_commit_id, created_at, resolved_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        impact_id,
                        chain_id,
                        previous["id"] if previous else None,
                        project_id,
                        commit_id,
                        draft_revision_id,
                        fact["eventId"],
                        plan_revision_id,
                        target["kind"],
                        target["revisionId"],
                        target["nodeId"],
                        target["label"],
                        dependency_key,
                        severity,
                        state,
                        resolution_note,
                        commit_id if state == "resolved-false-positive" else None,
                        commit_id,
                        now(),
                        resolved_at,
                    ),
                )
                if auto_match:
                    record(connection, "feedback.autoMatch")
                if manual_match:
                    record(connection, "feedback.manualLink")
                created.append(_impact_payload(row, predicate, evidence))
    return created
