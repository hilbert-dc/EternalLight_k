import json
from copy import deepcopy
from typing import Any, Callable

from .errors import ConflictError, WorkspaceError


MAX_PAYLOAD_BYTES = 48 * 1024
MAX_NODES = 200
NODE_KINDS = {"book", "volume", "unit"}
CERTAINTIES = {"open", "tentative", "settled"}
PROTECTIONS = {"free", "preserve-intent", "preserve-arrangement"}


def assert_outline_protections(
    previous_payload: dict[str, Any],
    candidate_payload: dict[str, Any],
    actor: str,
) -> None:
    """Enforce author-set outline protections for generated candidates."""
    if actor != "candidate":
        return
    previous_nodes = {
        node.get("id"): node
        for node in previous_payload.get("nodes", [])
        if isinstance(node, dict) and node.get("id")
    }
    candidate_nodes = {
        node.get("id"): node
        for node in candidate_payload.get("nodes", [])
        if isinstance(node, dict) and node.get("id")
    }
    for node_id, previous in previous_nodes.items():
        protection = previous.get("protection", "free")
        if protection == "free":
            continue
        candidate = candidate_nodes.get(node_id)
        if candidate is None:
            raise ConflictError(
                "protected_outline_node_changed",
                "受保护的大纲节点不能被生成候选删除",
                details={"nodeId": node_id, "protection": protection},
                recovery="由作者显式调整保护级别后再生成候选",
            )
        protected_fields = (
            {"purpose", "protection"}
            if protection == "preserve-intent"
            else {"parentId", "position", "kind", "protection"}
        )
        changed = sorted(
            field
            for field in protected_fields
            if previous.get(field) != candidate.get(field)
        )
        if changed:
            raise ConflictError(
                "protected_outline_node_changed",
                "生成候选修改了大纲节点的受保护内容",
                details={
                    "nodeId": node_id,
                    "protection": protection,
                    "fields": changed,
                },
                recovery="由作者显式修改该节点，或先调整保护级别",
            )


def _text(value: Any, label: str, *, required: bool = False, limit: int = 4_000) -> str:
    if (
        not isinstance(value, str)
        or len(value) > limit
        or (required and not value.strip())
    ):
        raise WorkspaceError("invalid_planning_contract", f"{label}字段无效")
    return value


def _check_size(payload: dict[str, Any]) -> None:
    if len(json.dumps(payload, ensure_ascii=False).encode("utf-8")) > MAX_PAYLOAD_BYTES:
        raise WorkspaceError("artifact_too_large", "规划内容超过 48 KiB", 413)


def normalize_outline(
    payload: dict[str, Any], make_id: Callable[[str], str]
) -> dict[str, Any]:
    if set(payload) != {"nodes"}:
        raise WorkspaceError("invalid_outline_contract", "层级大纲包含未知字段")
    nodes = payload.get("nodes")
    if not isinstance(nodes, list) or not 1 <= len(nodes) <= MAX_NODES:
        raise WorkspaceError(
            "invalid_outline_contract", "层级大纲必须包含 1–200 个节点"
        )
    result = deepcopy(payload)
    client_ids: dict[str, str] = {}
    for node in result["nodes"]:
        if not isinstance(node, dict):
            raise WorkspaceError("invalid_outline_contract", "大纲节点必须是对象")
        client_key = node.get("clientKey")
        node_id = node.get("id")
        if client_key is not None:
            if (
                not isinstance(client_key, str)
                or not client_key.strip()
                or client_key in client_ids
                or node_id
            ):
                raise WorkspaceError(
                    "invalid_outline_contract", "clientKey 必须唯一且不能与 id 混用"
                )
            client_ids[client_key] = make_id("outline-node")
        elif not isinstance(node_id, str) or not node_id.strip() or len(node_id) > 160:
            raise WorkspaceError("invalid_outline_contract", "已有节点必须提供稳定 id")
    for node in result["nodes"]:
        if "clientKey" in node:
            node["id"] = client_ids[node.pop("clientKey")]
        if "parentClientKey" in node:
            parent_key = node.pop("parentClientKey")
            if parent_key not in client_ids or node.get("parentId") is not None:
                raise WorkspaceError(
                    "invalid_outline_contract", "parentClientKey 未知或与 parentId 混用"
                )
            node["parentId"] = client_ids[parent_key]
    validate_outline(result)
    _check_size(result)
    return result


def validate_outline(payload: dict[str, Any]) -> None:
    nodes = payload["nodes"]
    ids: dict[str, dict[str, Any]] = {}
    roots = []
    positions: set[tuple[str | None, int]] = set()
    allowed = {
        "id",
        "kind",
        "parentId",
        "position",
        "title",
        "publicTitle",
        "purpose",
        "summary",
        "dependencyKeys",
        "certainty",
        "protection",
        "writerGuidance",
    }
    for node in nodes:
        if set(node) - allowed:
            raise WorkspaceError("invalid_outline_contract", "大纲节点包含未知字段")
        node_id = node.get("id")
        if node_id in ids:
            raise WorkspaceError("invalid_outline_contract", "大纲节点 id 必须唯一")
        ids[node_id] = node
        kind = node.get("kind")
        if kind not in NODE_KINDS:
            raise WorkspaceError("invalid_outline_contract", "大纲节点类型无效")
        parent_id = node.get("parentId")
        position = node.get("position")
        if isinstance(position, bool) or not isinstance(position, int) or position < 0:
            raise WorkspaceError("invalid_outline_contract", "节点位置必须是非负整数")
        if (parent_id, position) in positions:
            raise WorkspaceError("invalid_outline_contract", "同级节点位置必须唯一")
        positions.add((parent_id, position))
        if parent_id is None:
            roots.append(node)
        for key in ("title", "publicTitle", "purpose", "summary", "writerGuidance"):
            _text(node.get(key, ""), key)
        dependencies = node.get("dependencyKeys", [])
        if (
            not isinstance(dependencies, list)
            or len(dependencies) > 50
            or not all(
                isinstance(item, str) and item.strip() and len(item) <= 160
                for item in dependencies
            )
        ):
            raise WorkspaceError("invalid_outline_contract", "节点关联事实无效")
        if (
            node.get("certainty", "open") not in CERTAINTIES
            or node.get("protection", "free") not in PROTECTIONS
        ):
            raise WorkspaceError("invalid_outline_contract", "节点成熟度或保护级别无效")
    if len(roots) != 1 or roots[0].get("kind") != "book":
        raise WorkspaceError(
            "invalid_outline_contract", "层级大纲必须恰有一个全书根节点"
        )
    for node in nodes:
        parent_id = node.get("parentId")
        if parent_id is None:
            continue
        parent = ids.get(parent_id)
        if parent is None:
            raise WorkspaceError("invalid_outline_contract", "大纲节点父级不存在")
        allowed_parent = {
            "volume": {"book"},
            "unit": {"book", "volume"},
            "book": set(),
        }[node["kind"]]
        if parent["kind"] not in allowed_parent:
            raise WorkspaceError("invalid_outline_contract", "大纲节点父子层级无效")
        seen = {node["id"]}
        cursor = parent
        while cursor is not None:
            if cursor["id"] in seen:
                raise WorkspaceError("invalid_outline_contract", "大纲节点不能形成循环")
            seen.add(cursor["id"])
            cursor = ids.get(cursor.get("parentId"))
    for parent_id in {node.get("parentId") for node in nodes}:
        values = sorted(
            node["position"] for node in nodes if node.get("parentId") == parent_id
        )
        if values != list(range(len(values))):
            raise WorkspaceError(
                "invalid_outline_contract", "同级节点位置必须从 0 连续排列"
            )


def outline_ancestors(payload: dict[str, Any], node_id: str) -> list[str]:
    nodes = {node["id"]: node for node in payload.get("nodes", [])}
    if node_id not in nodes:
        raise WorkspaceError("invalid_plan_contract", "章纲引用的大纲节点不存在")
    result: list[str] = []
    cursor = nodes[node_id]
    while cursor.get("parentId"):
        result.append(cursor["parentId"])
        cursor = nodes[cursor["parentId"]]
    return result


def normalize_plan(
    payload: dict[str, Any],
    outline_id: str,
    outline: dict[str, Any],
    make_id: Callable[[str], str],
) -> dict[str, Any]:
    if payload.get("sourceOutlineRevisionId") != outline_id:
        raise WorkspaceError("stale_planning_source", "章纲必须基于当前正式大纲", 409)
    result = deepcopy(payload)
    chapters = result.get("chapters")
    if not isinstance(chapters, list) or not 1 <= len(chapters) <= 5:
        raise WorkspaceError("invalid_plan_contract", "近期计划必须包含 1–5 章")
    seen_keys: set[str] = set()
    for chapter in chapters:
        key = chapter.pop("clientKey", None)
        if key is not None:
            if (
                not isinstance(key, str)
                or not key.strip()
                or key in seen_keys
                or chapter.get("chapterPlanId")
            ):
                raise WorkspaceError("invalid_plan_contract", "章纲 clientKey 必须唯一")
            seen_keys.add(key)
            chapter["chapterPlanId"] = make_id("chapter-plan")
        outline_ancestors(outline, chapter.get("primaryNodeId"))
    _check_size(result)
    return result
