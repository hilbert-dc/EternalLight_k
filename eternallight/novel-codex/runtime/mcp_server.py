"""Novel Codex candidate-only MCP stdio sidecar.

The sidecar deliberately contains no persistence or author operations. It translates
bounded MCP tool calls into authenticated requests to the local Novel Runtime.
"""

import ipaddress
import json
import os
import sys
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode, urlsplit
from urllib.request import Request, urlopen


PROTOCOL_VERSION = "2025-06-18"
SERVER_NAME = "novel-codex-workspace"
SERVER_VERSION = "0.1.0"
MAX_RUNTIME_RESPONSE_BYTES = 2 * 1024 * 1024
MAX_MODEL_CONTENT_ITEM_BYTES = 9_000
COMMON_PROPERTIES: dict[str, dict[str, Any]] = {
    "taskId": {"type": "string", "minLength": 1, "maxLength": 160},
    "projectId": {"type": "string", "minLength": 1, "maxLength": 160},
    "threadId": {"type": "string", "minLength": 1, "maxLength": 160},
    "expectedRevision": {"type": "integer", "minimum": 0},
}
COMMON_REQUIRED = ["taskId", "projectId", "threadId", "expectedRevision"]

PLANNING_REVIEW_REPORT = {
    "type": "object",
    "properties": {
        "summary": {"type": "string", "maxLength": 2000},
        "strengths": {
            "type": "array",
            "maxItems": 3,
            "items": {
                "type": "object",
                "properties": {
                    "quote": {"type": "string", "minLength": 1, "maxLength": 1200},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 1600},
                },
                "required": ["quote", "reason"],
                "additionalProperties": False,
            },
        },
        "findings": {
            "type": "array",
            "maxItems": 6,
            "items": {
                "type": "object",
                "properties": {
                    "quote": {"type": "string", "minLength": 1, "maxLength": 1200},
                    "observation": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 1600,
                    },
                    "impact": {"type": "string", "minLength": 1, "maxLength": 1600},
                    "suggestion": {"type": "string", "maxLength": 1600},
                },
                "required": ["quote", "observation", "impact", "suggestion"],
                "additionalProperties": False,
            },
        },
        "coverage": {
            "type": "array",
            "maxItems": 120,
            "items": {
                "type": "object",
                "properties": {
                    "checkId": {"type": "string", "minLength": 1, "maxLength": 200},
                    "status": {
                        "type": "string",
                        "enum": ["checked", "unknown", "notChecked"],
                    },
                    "reason": {"type": "string", "minLength": 1, "maxLength": 1600},
                },
                "required": ["checkId", "status", "reason"],
                "additionalProperties": False,
            },
        },
    },
    "required": ["summary", "strengths", "findings", "coverage"],
    "additionalProperties": False,
}

TEXT_FIELD = {"type": "string", "maxLength": 12000}
BRIEF_PAYLOAD = {
    "type": "object",
    "properties": {
        "premise": {"type": "string", "minLength": 1, "maxLength": 12000},
        "promise": TEXT_FIELD,
        "stakes": TEXT_FIELD,
        "endingDirection": TEXT_FIELD,
    },
    "required": ["premise"],
    "additionalProperties": False,
}
OUTLINE_NODE = {
    "type": "object",
    "properties": {
        "id": {"type": "string", "minLength": 1, "maxLength": 160},
        "clientKey": {"type": "string", "minLength": 1, "maxLength": 160},
        "parentId": {"type": ["string", "null"], "maxLength": 160},
        "parentClientKey": {"type": "string", "minLength": 1, "maxLength": 160},
        "kind": {"type": "string", "enum": ["book", "volume", "unit"]},
        "position": {"type": "integer", "minimum": 0},
        "title": {"type": "string", "maxLength": 4000},
        "publicTitle": {"type": "string", "maxLength": 4000},
        "purpose": {"type": "string", "maxLength": 4000},
        "summary": {"type": "string", "maxLength": 4000},
        "writerGuidance": {"type": "string", "maxLength": 4000},
        "dependencyKeys": {
            "type": "array",
            "items": {"type": "string", "minLength": 1, "maxLength": 160},
            "maxItems": 50,
        },
        "certainty": {"type": "string", "enum": ["open", "tentative", "settled"]},
        "protection": {
            "type": "string",
            "enum": ["free", "preserve-intent", "preserve-arrangement"],
        },
    },
    "required": [
        "kind",
        "position",
        "title",
        "purpose",
        "summary",
        "dependencyKeys",
        "certainty",
        "protection",
    ],
    "anyOf": [{"required": ["id"]}, {"required": ["clientKey"]}],
    "additionalProperties": False,
}
OUTLINE_PAYLOAD = {
    "type": "object",
    "properties": {
        "nodes": {
            "type": "array",
            "items": OUTLINE_NODE,
            "minItems": 1,
            "maxItems": 200,
        },
    },
    "required": ["nodes"],
    "additionalProperties": False,
}
SCENE_PLAN = {
    "type": "object",
    "properties": {
        "id": {"type": "string", "minLength": 1, "maxLength": 160},
        "title": {"type": "string", "maxLength": 200},
        "goal": {"type": "string", "minLength": 1, "maxLength": 4000},
        "conflict": {"type": "string", "maxLength": 4000},
        "beats": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "minLength": 1, "maxLength": 160},
                    "description": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 2000,
                    },
                    "locked": {"type": "boolean"},
                },
                "required": ["id", "description"],
                "additionalProperties": False,
            },
            "maxItems": 40,
        },
        "locked": {"type": "boolean"},
    },
    "required": ["id", "goal", "beats"],
    "additionalProperties": False,
}
CHAPTER_PLAN = {
    "type": "object",
    "properties": {
        "chapterPlanId": {"type": "string", "minLength": 1, "maxLength": 160},
        "clientKey": {"type": "string", "minLength": 1, "maxLength": 160},
        "primaryNodeId": {"type": "string", "minLength": 1, "maxLength": 160},
        "chapterNumber": {"type": "integer", "minimum": 1},
        "title": {"type": "string", "maxLength": 200},
        "publicTitle": {"type": "string", "maxLength": 200},
        "goal": {"type": "string", "maxLength": 4000},
        "conflict": {"type": "string", "maxLength": 4000},
        "writerGuidance": {"type": "string", "maxLength": 4000},
        "dependencyKeys": {
            "type": "array",
            "items": {"type": "string", "minLength": 1, "maxLength": 160},
            "maxItems": 100,
        },
        "publicCommitments": {
            "type": "array",
            "items": {"type": "string", "minLength": 1, "maxLength": 2000},
            "maxItems": 100,
        },
        "allowances": {
            "type": "array",
            "items": {"type": "string", "minLength": 1, "maxLength": 2000},
            "maxItems": 100,
        },
        "scenes": {"type": "array", "items": SCENE_PLAN, "maxItems": 30},
        "locked": {"type": "boolean"},
        "povCharacterId": {"type": ["string", "null"], "maxLength": 160},
        "storyTime": {"type": ["integer", "null"], "minimum": 0},
    },
    "required": [
        "primaryNodeId",
        "chapterNumber",
        "goal",
        "dependencyKeys",
        "publicCommitments",
        "allowances",
    ],
    "anyOf": [{"required": ["chapterPlanId"]}, {"required": ["clientKey"]}],
    "additionalProperties": False,
}
PLAN_PAYLOAD = {
    "type": "object",
    "properties": {
        "sourceOutlineRevisionId": {"type": "string", "minLength": 1, "maxLength": 160},
        "window": {"type": "integer", "minimum": 1, "maximum": 5},
        "chapters": {
            "type": "array",
            "items": CHAPTER_PLAN,
            "minItems": 1,
            "maxItems": 5,
        },
    },
    "required": ["sourceOutlineRevisionId", "chapters"],
    "additionalProperties": False,
}
SCENE_CHANGE_GROUP = {
    "type": "object",
    "properties": {
        "id": {"type": "string", "minLength": 1, "maxLength": 160},
        "sceneId": {"type": ["string", "null"], "maxLength": 160},
        "body": {"type": "string", "minLength": 1, "maxLength": 120000},
        "locked": {"type": "boolean"},
    },
    "required": ["id", "body"],
    "additionalProperties": False,
}
DRAFT_PAYLOAD = {
    "type": "object",
    "properties": {
        "chapterId": {"type": "string", "minLength": 1, "maxLength": 160},
        "chapterNumber": {"type": "integer", "minimum": 1},
        "title": {"type": "string", "maxLength": 200},
        "body": {"type": "string", "minLength": 1, "maxLength": 120000},
        "sceneId": {"type": ["string", "null"], "maxLength": 160},
        "sceneOrder": {"type": ["integer", "null"], "minimum": 0},
        "completeness": {
            "type": "string",
            "enum": ["partial", "sceneComplete", "chapterComplete"],
        },
        "sceneGroups": {
            "type": "array",
            "items": SCENE_CHANGE_GROUP,
            "minItems": 1,
            "maxItems": 100,
        },
    },
    "required": ["body", "sceneGroups"],
    "anyOf": [{"required": ["chapterId"]}, {"required": ["chapterNumber"]}],
    "additionalProperties": False,
}
FACT_PROPOSAL = {
    "type": "object",
    "properties": {
        "sceneGroupId": {"type": "string", "minLength": 1, "maxLength": 160},
        "sceneId": {"type": ["string", "null"], "maxLength": 160},
        "factKind": {
            "type": "string",
            "enum": ["world", "reader", "character", "author-secret"],
        },
        "subjectId": {"type": ["string", "null"], "maxLength": 160},
        "predicate": {"type": "string", "minLength": 1, "maxLength": 160},
        "value": {},
        "epistemicOwnerId": {"type": ["string", "null"], "maxLength": 160},
        "epistemicState": {
            "type": "string",
            "enum": ["unknown", "suspected", "known", "false"],
        },
        "storyTime": {"type": ["integer", "null"]},
        "readingChapter": {"type": "integer", "minimum": 1},
        "sceneOrder": {"type": "integer", "minimum": 0},
        "evidence": {"type": "string", "minLength": 1, "maxLength": 1000},
    },
    "required": [
        "sceneGroupId",
        "factKind",
        "predicate",
        "value",
        "epistemicState",
        "readingChapter",
        "sceneOrder",
        "evidence",
    ],
    "additionalProperties": False,
}


def _schema(
    properties: dict[str, dict[str, Any]],
    required: list[str],
) -> dict[str, Any]:
    return {
        "type": "object",
        "properties": {**COMMON_PROPERTIES, **properties},
        "required": [*COMMON_REQUIRED, *required],
        "additionalProperties": False,
    }


TOOLS: list[dict[str, Any]] = [
    {
        "name": "read_context",
        "description": (
            "Read a project-bound ContextSnapshot before planning or writing. "
            "The Runtime verifies the task, project, and thread binding."
        ),
        "inputSchema": _schema(
            {
                "purpose": {
                    "type": "string",
                    "enum": ["planner", "writer", "critic"],
                },
                "storyTime": {
                    "description": "Monotonic story-world time marker.",
                    "type": "integer",
                },
                "readingChapter": {"type": "integer", "minimum": 0},
                "sceneOrder": {"type": "integer", "minimum": 0},
                "povCharacterId": {"type": "string", "minLength": 1, "maxLength": 160},
                "budget": {"type": "integer", "minimum": 1, "maximum": 100000},
                "byteBudget": {"type": "integer", "minimum": 4000, "maximum": 60000},
            },
            ["purpose", "budget", "byteBudget"],
        ),
    },
    {
        "name": "save_brief",
        "description": "Save a reviewable Brief candidate without accepting it.",
        "inputSchema": _schema(
            {
                "payload": BRIEF_PAYLOAD,
                "expectedCandidateRevisionId": {
                    "type": ["string", "null"],
                    "maxLength": 160,
                },
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
            },
            ["payload", "expectedCandidateRevisionId", "idempotencyKey"],
        ),
    },
    {
        "name": "save_outline",
        "description": "Save a reviewable Outline candidate without accepting it.",
        "inputSchema": _schema(
            {
                "payload": OUTLINE_PAYLOAD,
                "expectedCandidateRevisionId": {
                    "type": ["string", "null"],
                    "maxLength": 160,
                },
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
            },
            ["payload", "expectedCandidateRevisionId", "idempotencyKey"],
        ),
    },
    {
        "name": "save_chapter_plans",
        "description": "Save reviewable ChapterPlan candidates without accepting them.",
        "inputSchema": _schema(
            {
                "payload": PLAN_PAYLOAD,
                "expectedCandidateRevisionId": {
                    "type": ["string", "null"],
                    "maxLength": 160,
                },
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
            },
            ["payload", "expectedCandidateRevisionId", "idempotencyKey"],
        ),
    },
    {
        "name": "save_draft_checkpoint",
        "description": "Save an immutable scene-level DraftRevision checkpoint. For a throughScene task, body and sceneGroups must contain only the authorized prefix; Runtime preserves the unseen suffix; when a suffix exists, retain the authorized prefix scene count.",
        "inputSchema": _schema(
            {
                "payload": DRAFT_PAYLOAD,
                "expectedCandidateRevisionId": {
                    "type": ["string", "null"],
                    "maxLength": 160,
                },
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
            },
            ["payload", "expectedCandidateRevisionId", "idempotencyKey"],
        ),
    },
    {
        "name": "submit_fact_proposals",
        "description": "Submit story fact proposals for author review; never confirm them.",
        "inputSchema": _schema(
            {
                "proposals": {
                    "type": "array",
                    "items": FACT_PROPOSAL,
                    "minItems": 1,
                    "maxItems": 200,
                },
                "revisionId": {"type": "string", "minLength": 1, "maxLength": 160},
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
            },
            ["revisionId", "proposals", "idempotencyKey"],
        ),
    },
    {
        "name": "submit_planning_review_report",
        "description": (
            "Save this seat's evidence-based planning review without changing the story. "
            "Use the frozen runId and coverage IDs from read_context. Runtime verifies "
            "the seat binding, source quotes, and coverage; findings are advisory."
        ),
        "inputSchema": _schema(
            {
                "runId": {"type": "string", "minLength": 1, "maxLength": 160},
                "report": PLANNING_REVIEW_REPORT,
            },
            ["runId", "report"],
        ),
    },
    {
        "name": "complete_task",
        "description": "Request task completion. Runtime verifies persisted outputs and ignores claimed references.",
        "inputSchema": _schema(
            {
                "summary": {"type": "string", "maxLength": 4000},
            },
            ["summary"],
        ),
    },
]

TOOL_BY_NAME = {tool["name"]: tool for tool in TOOLS}
TOOLS_BY_OPERATION = {
    "advisory": {"read_context"},
    "reviewPlanningSeat": {
        "read_context",
        "submit_planning_review_report",
        "complete_task",
    },
    "generateBrief": {"read_context", "save_brief", "complete_task"},
    "generateOutline": {"read_context", "save_outline", "complete_task"},
    "generatePlan": {"read_context", "save_chapter_plans", "complete_task"},
    "generateChapter": {
        "read_context",
        "save_draft_checkpoint",
        "submit_fact_proposals",
        "complete_task",
    },
}


def _allowed_tool_names() -> set[str]:
    return TOOLS_BY_OPERATION.get(os.environ.get("NOVEL_CODEX_OPERATION", ""), set())


def _exposed_tools() -> list[dict[str, Any]]:
    allowed = _allowed_tool_names()
    return [tool for tool in TOOLS if tool["name"] in allowed]


@dataclass(frozen=True)
class RuntimeResponse:
    status: int
    payload: Any


class CandidateRuntimeClient:
    def __init__(self) -> None:
        self._runtime_url = _runtime_url_from_environment()
        self._candidate_token = os.environ.get("NOVEL_CODEX_CANDIDATE_TOKEN", "")
        self._task_capability = os.environ.get("NOVEL_CODEX_TASK_CAPABILITY", "")

    def call_tool(self, name: str, arguments: dict[str, Any]) -> RuntimeResponse:
        if name not in _allowed_tool_names():
            raise ValueError("tool is not allowed for this generation operation")
        self._validate_arguments(name, arguments)
        project = quote(arguments["projectId"], safe="")
        task = quote(arguments["taskId"], safe="")

        if name == "read_context":
            query = {
                key: _query_value(value)
                for key, value in arguments.items()
                if key not in {"projectId"}
            }
            query["projectRevision"] = query.pop("expectedRevision")
            return self._request(
                "GET", f"/api/projects/{project}/context?{urlencode(query)}"
            )
        if name in {"save_brief", "save_outline", "save_chapter_plans"}:
            artifact_kind = {
                "save_brief": "brief",
                "save_outline": "outline",
                "save_chapter_plans": "plan",
            }[name]
            return self._request(
                "POST",
                f"/api/projects/{project}/artifacts/{artifact_kind}",
                self._body_without(arguments, "projectId"),
            )
        if name == "save_draft_checkpoint":
            return self._request(
                "POST",
                f"/api/projects/{project}/drafts/checkpoints",
                self._body_without(arguments, "projectId"),
            )
        if name == "submit_fact_proposals":
            return self._request(
                "POST",
                f"/api/projects/{project}/reviews",
                {
                    "taskId": arguments["taskId"],
                    "threadId": arguments["threadId"],
                    "expectedRevision": arguments["expectedRevision"],
                    "artifactKind": "draft",
                    "revisionId": arguments["revisionId"],
                    "proposals": arguments["proposals"],
                    "idempotencyKey": arguments["idempotencyKey"],
                },
            )
        if name == "submit_planning_review_report":
            run = quote(arguments["runId"], safe="")
            return self._request(
                "POST",
                f"/api/projects/{project}/planning-reviews/{run}/reports",
                self._body_without(arguments, "projectId", "runId"),
            )
        if name == "complete_task":
            return self._request(
                "POST",
                f"/api/projects/{project}/tasks/{task}/complete",
                {
                    "threadId": arguments["threadId"],
                    "expectedRevision": arguments["expectedRevision"],
                    "summary": arguments["summary"],
                },
            )
        raise ValueError(f"unknown tool: {name}")

    @staticmethod
    def _body_without(arguments: dict[str, Any], *keys: str) -> dict[str, Any]:
        excluded = set(keys)
        return {key: value for key, value in arguments.items() if key not in excluded}

    @staticmethod
    def _validate_arguments(name: str, arguments: dict[str, Any]) -> None:
        tool = TOOL_BY_NAME.get(name)
        if tool is None:
            raise ValueError(f"unknown tool: {name}")
        schema = tool["inputSchema"]
        unknown = sorted(set(arguments) - set(schema["properties"]))
        if unknown:
            raise ValueError(f"unexpected arguments: {', '.join(unknown)}")
        missing = [key for key in schema["required"] if key not in arguments]
        if missing:
            raise ValueError(f"missing required arguments: {', '.join(missing)}")
        for key in ("taskId", "projectId", "threadId"):
            value = arguments[key]
            if not isinstance(value, str) or not value.strip():
                raise ValueError(f"{key} must be a non-empty string")
        revision = arguments["expectedRevision"]
        if isinstance(revision, bool) or not isinstance(revision, int) or revision < 0:
            raise ValueError("expectedRevision must be a non-negative integer")
        if name == "read_context":
            if arguments["purpose"] not in {"planner", "writer", "critic"}:
                raise ValueError("purpose must be planner, writer, or critic")
            if arguments["purpose"] in {"writer", "critic"}:
                missing_position = [
                    key
                    for key in ("readingChapter", "sceneOrder")
                    if key not in arguments
                ]
                if missing_position:
                    raise ValueError(
                        "writer and critic context require readingChapter and sceneOrder"
                    )
            for key in ("budget", "byteBudget"):
                value = arguments[key]
                if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                    raise ValueError(f"{key} must be a non-negative integer")
            for key in ("storyTime", "readingChapter", "sceneOrder"):
                if key not in arguments:
                    continue
                value = arguments[key]
                if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                    raise ValueError(f"{key} must be a non-negative integer")
            if arguments["budget"] == 0 or arguments["budget"] > 100000:
                raise ValueError("budget must be between 1 and 100000")
            if arguments["byteBudget"] < 4000 or arguments["byteBudget"] > 60000:
                raise ValueError("byteBudget must be between 4000 and 60000")
        if name in {"save_brief", "save_outline", "save_chapter_plans"}:
            if not isinstance(arguments["payload"], dict):
                raise ValueError("payload must be an object")
        if name == "save_draft_checkpoint":
            if not isinstance(arguments["payload"], dict):
                raise ValueError("payload must be an object")
        if name == "submit_fact_proposals":
            if (
                not isinstance(arguments["revisionId"], str)
                or not arguments["revisionId"].strip()
            ):
                raise ValueError("revisionId must be a non-empty string")
            _validate_object_list(
                arguments["proposals"], "proposals", require_items=True
            )
        if name == "submit_planning_review_report":
            _validate_review_value(arguments, schema, "arguments")
        if name == "complete_task":
            if not isinstance(arguments["summary"], str):
                raise ValueError("summary must be a string")

    def _request(
        self, method: str, path: str, payload: dict[str, Any] | None = None
    ) -> RuntimeResponse:
        if not self._runtime_url:
            raise RuntimeError("NOVEL_CODEX_RUNTIME_URL is not configured")
        if not self._candidate_token:
            raise RuntimeError("NOVEL_CODEX_CANDIDATE_TOKEN is not configured")
        if not self._task_capability:
            raise RuntimeError("NOVEL_CODEX_TASK_CAPABILITY is not configured")
        body = None
        if payload is not None:
            body = json.dumps(
                payload, ensure_ascii=False, separators=(",", ":")
            ).encode("utf-8")
        request = Request(
            f"{self._runtime_url}{path}",
            data=body,
            method=method,
            headers={
                "Accept": "application/json",
                "Authorization": f"Bearer {self._candidate_token}",
                "Content-Type": "application/json",
                "X-Novel-Task-Capability": self._task_capability,
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                raw = response.read(MAX_RUNTIME_RESPONSE_BYTES + 1)
                if len(raw) > MAX_RUNTIME_RESPONSE_BYTES:
                    raise RuntimeError("Novel Runtime response exceeded the size limit")
                return RuntimeResponse(response.status, _decode_json(raw))
        except HTTPError as error:
            raw = error.read(MAX_RUNTIME_RESPONSE_BYTES + 1)
            payload = (
                _decode_json(raw)
                if len(raw) <= MAX_RUNTIME_RESPONSE_BYTES
                else {
                    "error": {
                        "code": "response_too_large",
                        "message": "Novel Runtime error response exceeded the size limit",
                    }
                }
            )
            return RuntimeResponse(error.code, payload)
        except URLError as error:
            raise RuntimeError(
                f"Novel Runtime is unavailable: {error.reason}"
            ) from error


def _decode_json(raw: bytes) -> Any:
    if not raw:
        return {}
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("Novel Runtime returned invalid JSON") from error


def _runtime_url_from_environment() -> str:
    value = os.environ.get("NOVEL_CODEX_RUNTIME_URL", "").rstrip("/")
    if not value:
        return ""
    parsed = urlsplit(value)
    try:
        is_loopback = parsed.hostname == "localhost" or (
            parsed.hostname is not None
            and ipaddress.ip_address(parsed.hostname).is_loopback
        )
    except ValueError:
        is_loopback = False
    if (
        parsed.scheme != "http"
        or not is_loopback
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or parsed.path not in {"", "/"}
    ):
        raise RuntimeError("NOVEL_CODEX_RUNTIME_URL must be a loopback HTTP origin")
    return value


def _query_value(value: Any) -> Any:
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    return value


def _validate_object_list(value: Any, name: str, require_items: bool = False) -> None:
    if (
        not isinstance(value, list)
        or (require_items and not value)
        or len(value) > 200
        or not all(isinstance(item, dict) for item in value)
    ):
        qualifier = "non-empty " if require_items else ""
        raise ValueError(
            f"{name} must be a {qualifier}object array with at most 200 items"
        )


def _validate_review_value(value: Any, schema: dict[str, Any], path: str) -> None:
    value_type = schema.get("type")
    if value_type == "object":
        if not isinstance(value, dict):
            raise ValueError(f"{path} must be an object")
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False and set(value) - set(properties):
            raise ValueError(f"{path} contains unexpected fields")
        if set(schema.get("required", [])) - set(value):
            raise ValueError(f"{path} is missing required fields")
        for key, item in value.items():
            if key in properties:
                _validate_review_value(item, properties[key], f"{path}.{key}")
    elif value_type == "array":
        if not isinstance(value, list):
            raise ValueError(f"{path} must be an array")
        if not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", 200):
            raise ValueError(f"{path} has an invalid item count")
        for index, item in enumerate(value):
            _validate_review_value(item, schema.get("items", {}), f"{path}[{index}]")
    elif value_type == "string":
        if not isinstance(value, str):
            raise ValueError(f"{path} must be a string")
        if "\x00" in value:
            raise ValueError(f"{path} contains a null byte")
        if (
            not schema.get("minLength", 0)
            <= len(value)
            <= schema.get("maxLength", 4000)
        ):
            raise ValueError(f"{path} has an invalid length")
        if schema.get("minLength", 0) > 0 and not value.strip():
            raise ValueError(f"{path} must not be blank")
    elif value_type == "integer":
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError(f"{path} must be an integer")
        if value < schema.get("minimum", 0):
            raise ValueError(f"{path} is below the minimum")
    if "enum" in schema and value not in schema["enum"]:
        raise ValueError(f"{path} has an invalid value")


def _jsonrpc_result(request_id: Any, result: Any) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _jsonrpc_error(
    request_id: Any, code: int, message: str, data: Any | None = None
) -> dict[str, Any]:
    error: dict[str, Any] = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    return {"jsonrpc": "2.0", "id": request_id, "error": error}


def _content_chunks(text: str) -> list[str]:
    chunks: list[str] = []
    current: list[str] = []
    current_bytes = 0
    for character in text:
        character_bytes = len(character.encode("utf-8"))
        if current and current_bytes + character_bytes > MAX_MODEL_CONTENT_ITEM_BYTES:
            chunks.append("".join(current))
            current = []
            current_bytes = 0
        current.append(character)
        current_bytes += character_bytes
    if current or not chunks:
        chunks.append("".join(current))
    return chunks


def _model_safe_write_result(name: str, payload: Any) -> Any:
    if not isinstance(payload, dict):
        return payload
    if name == "submit_planning_review_report":
        if "error" in payload:
            error = payload["error"]
            return (
                {
                    "error": {
                        "code": str(error.get("code", "review_report_failed"))[:160],
                        "message": str(
                            error.get("message", "Planning review report failed")
                        )[:500],
                    }
                }
                if isinstance(error, dict)
                else {"error": "Planning review report failed"}
            )
        return {
            "reportId": payload.get("id"),
            "role": payload.get("role"),
            "coverageStatus": payload.get("status"),
        }
    if name in {
        "save_draft_checkpoint",
        "save_brief",
        "save_outline",
        "save_chapter_plans",
    }:
        return {
            key: value
            for key, value in payload.items()
            if key not in {"body", "sceneGroups", "payload"}
        }
    if name == "submit_fact_proposals":
        keys = (
            "id",
            "artifactKind",
            "artifactRevisionId",
            "projectRevision",
            "status",
        )
        return {
            **{key: payload.get(key) for key in keys},
            "factProposalCount": len(payload.get("factProposals", [])),
            "provisionalImpactCount": len(payload.get("provisionalImpacts", [])),
        }
    if name == "complete_task":
        refs = payload.get("outputRefs", [])
        return {
            key: payload.get(key) for key in ("id", "status", "stage", "deliveryStatus")
        } | {
            "outputRefCount": len(refs),
            "latestOutputRef": refs[-1] if refs else None,
        }
    return payload


def _tool_result(response: RuntimeResponse, name: str = "") -> dict[str, Any]:
    payload = _model_safe_write_result(name, response.payload)
    text = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    result: dict[str, Any] = {
        "content": [{"type": "text", "text": chunk} for chunk in _content_chunks(text)],
    }
    if response.status < 200 or response.status >= 300:
        result["isError"] = True
    return result


def handle_message(
    message: Any, client: CandidateRuntimeClient
) -> dict[str, Any] | None:
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
        return _jsonrpc_error(None, -32600, "Invalid Request")
    method = message.get("method")
    request_id = message.get("id")
    if not isinstance(method, str):
        return _jsonrpc_error(request_id, -32600, "Invalid Request")
    if "id" not in message:
        return None

    if method == "initialize":
        params = message.get("params")
        if not isinstance(params, dict):
            return _jsonrpc_error(request_id, -32602, "Invalid params")
        return _jsonrpc_result(
            request_id,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": (
                    "Candidate-only Novel workspace tools. All calls are bound to a "
                    "TaskRun, project, thread, and expected revision."
                ),
            },
        )
    if method == "ping":
        return _jsonrpc_result(request_id, {})
    if method == "tools/list":
        return _jsonrpc_result(request_id, {"tools": _exposed_tools()})
    if method == "tools/call":
        params = message.get("params")
        if not isinstance(params, dict):
            return _jsonrpc_error(request_id, -32602, "Invalid params")
        name = params.get("name")
        arguments = params.get("arguments", {})
        if not isinstance(name, str) or not isinstance(arguments, dict):
            return _jsonrpc_error(request_id, -32602, "Invalid params")
        try:
            return _jsonrpc_result(
                request_id, _tool_result(client.call_tool(name, arguments), name)
            )
        except ValueError as error:
            return _jsonrpc_error(request_id, -32602, "Invalid params", str(error))
        except RuntimeError as error:
            result = {
                "content": [{"type": "text", "text": str(error)}],
                "isError": True,
            }
            return _jsonrpc_result(request_id, result)
    return _jsonrpc_error(request_id, -32601, "Method not found")


def run() -> None:
    client = CandidateRuntimeClient()
    for line in sys.stdin:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            response = _jsonrpc_error(None, -32700, "Parse error")
        else:
            try:
                response = handle_message(message, client)
            except Exception:
                response = _jsonrpc_error(message.get("id"), -32603, "Internal error")
        if response is not None:
            sys.stdout.write(
                json.dumps(response, ensure_ascii=False, separators=(",", ":"))
            )
            sys.stdout.write("\n")
            sys.stdout.flush()


if __name__ == "__main__":
    run()
