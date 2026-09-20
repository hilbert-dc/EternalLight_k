import copy
import json
from typing import Any

from .errors import WorkspaceError


MIN_CONTEXT_BYTES = 4_000
MAX_CONTEXT_BYTES = 64_000


def _size(value: Any) -> int:
    return len(
        json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    )


def fit_context_snapshot(snapshot: dict[str, Any], byte_budget: int) -> dict[str, Any]:
    bounded = min(max(int(byte_budget), MIN_CONTEXT_BYTES), MAX_CONTEXT_BYTES)
    result = copy.deepcopy(snapshot)
    omitted = result.setdefault("omissions", {})
    omitted.setdefault("sourceExcerptCount", 0)
    omitted.setdefault("additionalFactCount", 0)
    result["contextBudget"] = {"maxBytes": bounded, "usedBytes": 0, "truncated": False}

    while True:
        # usedBytes itself affects the serialized size. Iterate until the value stabilizes.
        encoded_size = _size(result)
        if result["contextBudget"]["usedBytes"] != encoded_size:
            result["contextBudget"]["usedBytes"] = encoded_size
            continue
        if encoded_size <= bounded:
            break
        excerpts = result.get("sourceExcerpts")
        removable_excerpt = (
            next(
                (
                    index
                    for index in range(len(excerpts) - 1, -1, -1)
                    if excerpts[index].get("evidenceClass", "supporting")
                    == "supporting"
                ),
                None,
            )
            if isinstance(excerpts, list)
            else None
        )
        if removable_excerpt is not None:
            excerpts.pop(removable_excerpt)
            omitted["sourceExcerptCount"] = (
                int(omitted.get("sourceExcerptCount", 0)) + 1
            )
            result["contextBudget"]["truncated"] = True
            continue
        facts = result.get("facts")
        if isinstance(facts, list) and facts:
            facts.pop()
            omitted["additionalFactCount"] = (
                int(omitted.get("additionalFactCount", 0)) + 1
            )
            result["contextBudget"]["truncated"] = True
            continue
        else:
            raise WorkspaceError(
                "context_budget_too_small",
                "上下文预算不足以容纳当前硬约束",
                413,
                {"maxBytes": bounded, "requiredBytes": encoded_size},
                "提高上下文预算或缩小创作任务范围",
            )
    return result
