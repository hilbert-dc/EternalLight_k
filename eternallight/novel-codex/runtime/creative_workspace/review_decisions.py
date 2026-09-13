import json
from typing import Any

from .errors import ConflictError, WorkspaceError
from .impacts import (
    preview_planning_impacts,
    validate_author_fact_dependency_links,
)


def _load(value: str | None, fallback: Any) -> Any:
    return json.loads(value) if value else fallback


def validate_draft_acceptance(
    connection,
    project_id: str,
    draft,
    review,
    selections: dict[str, Any],
    fact_rows: list[Any],
    fact_payloads: list[dict[str, Any]],
    accepted_chapter: int,
) -> dict[str, Any]:
    if not fact_rows and selections.get("noConfirmedChanges") is not True:
        raise WorkspaceError(
            "no_confirmed_changes_decision_required",
            "没有候选事实时，必须明确确认本章没有要确认的变化",
        )
    decisions = selections.get("factDecisions", {})
    if not isinstance(decisions, dict):
        raise WorkspaceError("fact_decision_required", "事实决定必须是对象")
    candidate_group_ids = {
        group.get("id")
        for group in _load(draft["scene_groups_json"], [])
        if group.get("id")
    }
    if "sceneGroupIds" not in selections:
        selected_group_ids = candidate_group_ids
    else:
        selected_value = selections["sceneGroupIds"]
        if not isinstance(selected_value, list):
            raise WorkspaceError(
                "invalid_scene_selection", "所选场景必须是场景 ID 数组"
            )
        if not selected_value:
            raise WorkspaceError(
                "scene_selection_required", "采用正文时至少选择一个场景"
            )
        selected_group_ids = set(selected_value)
    manual_links = validate_author_fact_dependency_links(
        connection,
        project_id,
        fact_payloads,
        selections.get("factDependencyLinks"),
    )
    stored_review_selections = _load(review["selections_json"], {})
    frozen_previews = stored_review_selections.get("provisionalImpacts", [])
    manual_previews = preview_planning_impacts(
        connection,
        project_id,
        fact_payloads,
        accepted_chapter,
        manual_links,
    )
    previews = {
        preview["id"]: preview
        for preview in [*frozen_previews, *manual_previews]
        if isinstance(preview, dict) and isinstance(preview.get("id"), str)
    }
    direction_decisions = selections.get("directionChangeDecisions", {})
    if not isinstance(direction_decisions, dict):
        raise WorkspaceError(
            "direction_change_decision_required", "方向变化决定必须是对象"
        )
    confirmed_fact_ids = {
        row["id"]
        for row in fact_rows
        if row["scene_group_id"] in selected_group_ids
        and decisions.get(row["id"], row["review_decision"]) == "confirmed"
    }
    groups_by_id = {
        group.get("id"): group
        for group in _load(draft["scene_groups_json"], [])
        if isinstance(group, dict)
    }
    unanchored_fact_ids = [
        row["id"]
        for row in fact_rows
        if row["id"] in confirmed_fact_ids
        and row["fact_kind"] != "author-secret"
        and row["evidence"].strip()
        not in groups_by_id.get(row["scene_group_id"], {}).get("body", "")
    ]
    if unanchored_fact_ids:
        raise WorkspaceError(
            "review_checks_failed",
            "已确认事实的证据不在所选场景正文中",
            details={
                "checkIds": ["fact-evidence-anchor"],
                "factIds": unanchored_fact_ids,
            },
        )
    for link in manual_links:
        if link["factProposalId"] not in confirmed_fact_ids:
            raise WorkspaceError(
                "fact_dependency_link_not_confirmed",
                "只有已确认的事实可以连接到规划节点",
            )
    for preview in previews.values():
        if (
            preview.get("factProposalId") not in confirmed_fact_ids
            or preview.get("severity") != "locked-conflict"
        ):
            continue
        direction = direction_decisions.get(preview["id"])
        decision = (
            direction
            if isinstance(direction, str)
            else direction.get("decision")
            if isinstance(direction, dict)
            else None
        )
        if decision not in {
            "return-draft",
            "accept-and-open-impact",
            "false-positive",
        }:
            raise WorkspaceError(
                "direction_change_decision_required",
                "锁定规划冲突必须由作者明确决定",
                details={"provisionalImpactId": preview["id"]},
            )
        if decision == "return-draft":
            raise ConflictError(
                "draft_returned_for_revision",
                "作者决定退回草稿，正文尚未采用",
                details={"provisionalImpactId": preview["id"]},
            )
        if decision == "false-positive" and (
            not isinstance(direction, dict)
            or not isinstance(direction.get("note"), str)
            or not direction["note"].strip()
            or len(direction["note"]) > 2_000
        ):
            raise WorkspaceError(
                "direction_change_note_required", "标记误报必须填写作者说明"
            )
    return {
        "decisions": decisions,
        "selectedGroupIds": selected_group_ids,
        "manualLinks": manual_links,
        "directionDecisions": direction_decisions,
    }
