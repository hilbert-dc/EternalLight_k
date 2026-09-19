"""Frozen, independent planning advice; reports never mutate creative artifacts."""

import hashlib
import json
import time
from uuid import uuid4

from .errors import ConflictError, NotFoundError, WorkspaceError
from .generation import ACTIVE_STATUSES, record_task_output, require_active_task

try:
    from ..storage import connect, initialize, now
except ImportError:
    from storage import connect, initialize, now


ROLE_QUESTIONS = {
    "structure": "检查目标、因果推进、冲突与转折是否成立；区分缺失证据与已发现问题。",
    "character": "检查人物动机、选择、代价与关系变化；不将未写明的动机当作事实。",
    "experience": "检查读者期待、信息释放、节奏与情绪变化；这是规划顾问，不模拟受限人物视角。",
}


def _json(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _text(value, limit=1600):
    if (
        not isinstance(value, str)
        or not value.strip()
        or len(value) > limit
        or "\x00" in value
    ):
        raise WorkspaceError("invalid_planning_review", "评审文本为空、过长或无效")
    return value


def _fields(value, required, optional=()):
    if (
        not isinstance(value, dict)
        or not set(required) <= set(value)
        or set(value) - set(required) - set(optional)
    ):
        raise WorkspaceError("invalid_planning_review", "评审字段缺失或包含未知字段")


def _run(connection, project_id, run_id, *, writable=False):
    row = connection.execute(
        "SELECT * FROM planning_review_runs WHERE id = ? AND project_id = ?",
        (run_id, project_id),
    ).fetchone()
    if row is None:
        raise NotFoundError("规划深审不存在")
    if writable and row["status"] != "open":
        raise ConflictError("planning_review_closed", "规划深审已结束，请新建评审")
    if writable:
        revision = connection.execute(
            "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
        ).fetchone()
        if revision is None or revision[0] != row["input_revision"]:
            raise ConflictError(
                "stale_planning_review", "作品正式版本已变化，请基于新版本重新评审"
            )
    return row


def _report(row):
    return {
        "id": row["id"],
        "runId": row["run_id"],
        "role": row["role"],
        "status": row["status"],
        "report": json.loads(row["report_json"]),
    }


def _view(connection, row):
    seats = connection.execute(
        "SELECT s.role, s.task_id, t.status FROM planning_review_seats s LEFT JOIN task_runs t ON t.id = s.task_id WHERE s.run_id = ?",
        (row["id"],),
    ).fetchall()
    reports = connection.execute(
        "SELECT * FROM planning_review_reports WHERE run_id = ? ORDER BY role",
        (row["id"],),
    ).fetchall()
    coverage_status = (
        "complete"
        if row["status"] == "complete"
        else "pending"
        if row["status"] == "open" and len(reports) < len(seats)
        else "pending"
        if row["status"] == "cancelled" and not reports
        else "partial"
    )
    return {
        "id": row["id"],
        "projectId": row["project_id"],
        "artifactKind": row["artifact_kind"],
        "revisionId": row["revision_id"],
        "targetNodeId": row["target_node_id"],
        "authorIntent": row["author_intent"],
        "roles": json.loads(row["roles_json"]),
        "sourcePacketHash": row["source_packet_hash"],
        "inputRevision": row["input_revision"],
        "status": {"open": "running", "complete": "completed"}.get(
            row["status"], row["status"]
        ),
        "coverageStatus": coverage_status,
        "checksByRole": json.loads(row["checks_json"]),
        "seats": [
            {
                "role": seat["role"],
                "taskId": seat["task_id"],
                "status": seat["status"] or "pending",
            }
            for seat in seats
        ],
        "reports": [_report(report) for report in reports],
        "synthesis": json.loads(row["synthesis_json"])
        if row["synthesis_json"]
        else None,
        "createdAt": row["created_at"],
        "updatedAt": row["updated_at"],
    }


def list_planning_reviews(connection, project_id):
    rows = connection.execute(
        "SELECT * FROM planning_review_runs WHERE project_id = ? ORDER BY created_at DESC, id DESC LIMIT 10",
        (project_id,),
    ).fetchall()
    return [_view(connection, row) for row in rows]


def get_planning_review(db_path, project_id, run_id):
    initialize(db_path)
    with connect(db_path) as connection:
        return _view(connection, _run(connection, project_id, run_id))


def create_planning_review(db_path, project_id, payload):
    _fields(
        payload,
        {"artifactKind", "revisionId", "authorIntent", "roles"},
        {"targetNodeId"},
    )
    kind, revision_id, roles = (
        payload["artifactKind"],
        _text(payload["revisionId"], 160),
        payload["roles"],
    )
    if (
        kind not in ("outline", "plan")
        or not isinstance(roles, list)
        or len(roles) not in (2, 3)
        or any(
            not isinstance(role, str) or role not in ROLE_QUESTIONS for role in roles
        )
        or len(set(roles)) != len(roles)
    ):
        raise WorkspaceError(
            "invalid_planning_review", "请选择规划产物和两至三个不同顾问"
        )
    intent = _text(payload["authorIntent"], 4000)
    target = payload.get("targetNodeId")
    if target is not None:
        _text(target, 160)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        revision = connection.execute(
            "SELECT payload_json FROM artifact_revisions WHERE id = ? AND project_id = ? AND artifact_kind = ?",
            (revision_id, project_id, kind),
        ).fetchone()
        workspace = connection.execute(
            "SELECT revision FROM project_workspace WHERE project_id = ?", (project_id,)
        ).fetchone()
        if revision is None or workspace is None:
            raise NotFoundError("指定的规划版本不存在于当前作品")
        artifact = json.loads(revision["payload_json"])
        nodes, id_key = (
            (artifact["nodes"], "id")
            if kind == "outline"
            else (artifact["chapters"], "chapterPlanId")
        )
        node_ids = {node[id_key] for node in nodes}
        if target is not None and target not in node_ids:
            raise WorkspaceError("invalid_review_scope", "评审节点不属于指定规划版本")
        selected = node_ids if target is None else {target}
        if kind == "outline" and target is not None:
            for _ in nodes:
                selected |= {
                    node["id"] for node in nodes if node.get("parentId") in selected
                }
        checks = {
            role: [
                f"{role}:{node[id_key]}" for node in nodes if node[id_key] in selected
            ]
            for role in roles
        }
        if any(len(items) > 120 for items in checks.values()):
            raise WorkspaceError(
                "planning_review_scope_too_large",
                "检查项超过 120 项，请选择更小的规划范围",
            )
        packet = {
            "artifactKind": kind,
            "revisionId": revision_id,
            "artifact": artifact,
            "scope": {"targetNodeId": target, "nodeIds": sorted(selected)},
            "authorIntent": intent,
        }
        packet_json = _json(packet)
        packet_hash = hashlib.sha256(packet_json.encode()).hexdigest()
        run_id, timestamp = f"planning-review-{uuid4().hex}", now()
        for role in roles:
            snapshot = _seat_input(
                project_id,
                workspace[0],
                run_id,
                role,
                packet,
                packet_hash,
                checks[role],
            )
            if len(_json(snapshot).encode()) > 60_000:
                raise WorkspaceError(
                    "planning_review_input_too_large",
                    "完整规划与检查清单超过输入预算，请缩小规划或评审范围",
                )
        connection.execute(
            "INSERT INTO planning_review_runs(id, project_id, artifact_kind, revision_id, target_node_id, author_intent, input_revision, roles_json, source_packet_json, source_packet_hash, checks_json, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?, ?)",
            (
                run_id,
                project_id,
                kind,
                revision_id,
                target,
                intent,
                workspace[0],
                _json(roles),
                packet_json,
                packet_hash,
                _json(checks),
                timestamp,
                timestamp,
            ),
        )
        connection.executemany(
            "INSERT INTO planning_review_seats(run_id, role) VALUES (?, ?)",
            [(run_id, role) for role in roles],
        )
        return _view(connection, _run(connection, project_id, run_id))


def _seat_input(project_id, revision, run_id, role, packet, packet_hash, checks):
    return {
        "projectId": project_id,
        "projectRevision": revision,
        "purpose": "planner",
        "reviewRunId": run_id,
        "role": role,
        "sourcePacket": packet,
        "sourcePacketHash": packet_hash,
        "checks": checks,
        "roleInstruction": ROLE_QUESTIONS[role],
        "authorIntent": packet["authorIntent"],
    }


def create_seat_task(db_path, project_id, run_id, role, payload):
    _fields(payload, {"threadId", "capabilityHash", "capabilityExpiresAt"}, {"model"})
    thread_id, capability = (
        _text(payload["threadId"], 160),
        _text(payload["capabilityHash"], 64),
    )
    expires, model = payload["capabilityExpiresAt"], payload.get("model")
    if (
        len(capability) != 64
        or any(char not in "0123456789abcdef" for char in capability)
        or isinstance(expires, bool)
        or not isinstance(expires, int)
        or not int(time.time()) < expires <= int(time.time()) + 3600
    ):
        raise WorkspaceError("invalid_task_capability", "任务能力凭证绑定无效")
    if model is not None:
        _text(model, 120)
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        run = _run(connection, project_id, run_id, writable=True)
        seat = connection.execute(
            "SELECT * FROM planning_review_seats WHERE run_id = ? AND role = ?",
            (run_id, role),
        ).fetchone()
        if seat is None:
            raise WorkspaceError("invalid_review_role", "顾问不属于本次评审")
        if connection.execute(
            "SELECT 1 FROM planning_review_reports WHERE run_id = ? AND role = ?",
            (run_id, role),
        ).fetchone():
            raise ConflictError("planning_review_report_exists", "该顾问已交付报告")
        if connection.execute(
            "SELECT 1 FROM planning_review_seats s JOIN task_runs t ON t.id = s.task_id WHERE s.run_id = ? AND t.status IN ('queued','running','waiting')",
            (run_id,),
        ).fetchone():
            raise ConflictError(
                "planning_review_seat_busy", "请先完成或停止当前顾问任务"
            )
        if connection.execute(
            "SELECT 1 FROM task_runs WHERE thread_id = ?", (thread_id,)
        ).fetchone():
            raise ConflictError(
                "planning_review_thread_reused", "每位顾问必须使用独立的新线程"
            )
        task_id, snapshot_id, timestamp = (
            f"task-{uuid4().hex}",
            f"input-{uuid4().hex}",
            now(),
        )
        packet = json.loads(run["source_packet_json"])
        snapshot_json = _json(
            _seat_input(
                project_id,
                run["input_revision"],
                run_id,
                role,
                packet,
                run["source_packet_hash"],
                json.loads(run["checks_json"])[role],
            )
        )
        target = {
            "type": "artifact",
            "artifactKind": run["artifact_kind"],
            "revisionId": run["revision_id"],
            "reviewRunId": run_id,
            "role": role,
        }
        manifest = {
            "operation": "reviewPlanningSeat",
            "projectId": project_id,
            "projectRevision": run["input_revision"],
            "target": target,
            "authorIntent": run["author_intent"],
            "sourcePacketHash": run["source_packet_hash"],
        }
        connection.execute(
            "INSERT INTO task_runs(id, project_id, thread_id, task_kind, purpose, target_json, required_output_kind, status, input_revision, model, operation, input_manifest_json, input_snapshot_id, capability_hash, capability_expires_at, created_at, updated_at) VALUES (?, ?, ?, 'planningReview', 'planner', ?, 'critique', 'queued', ?, ?, 'reviewPlanningSeat', ?, ?, ?, ?, ?, ?)",
            (
                task_id,
                project_id,
                thread_id,
                _json(target),
                run["input_revision"],
                model,
                _json(manifest),
                snapshot_id,
                capability,
                expires,
                timestamp,
                timestamp,
            ),
        )
        connection.execute(
            "INSERT INTO task_input_snapshots(id, project_id, task_id, purpose, content_json, content_hash, byte_size, created_at) VALUES (?, ?, ?, 'planner', ?, ?, ?, ?)",
            (
                snapshot_id,
                project_id,
                task_id,
                snapshot_json,
                hashlib.sha256(snapshot_json.encode()).hexdigest(),
                len(snapshot_json.encode()),
                timestamp,
            ),
        )
        connection.execute(
            "UPDATE planning_review_seats SET task_id = ? WHERE run_id = ? AND role = ?",
            (task_id, run_id, role),
        )
        connection.execute(
            "UPDATE planning_review_runs SET updated_at = ? WHERE id = ?",
            (timestamp, run_id),
        )
        return {
            "id": task_id,
            "projectId": project_id,
            "threadId": thread_id,
            "kind": "planningReview",
            "purpose": "planner",
            "target": target,
            "operation": "reviewPlanningSeat",
            "requiredOutputKind": "critique",
            "inputManifest": manifest,
            "inputRevision": run["input_revision"],
            "stage": "preparing",
            "status": "queued",
            "deliveryStatus": "pending",
        }


def _source_texts(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from _source_texts(item)
    elif isinstance(value, list):
        for item in value:
            yield from _source_texts(item)


def _validate_report(report, run, role):
    _fields(report, {"summary", "strengths", "findings", "coverage"})
    _text(report["summary"], 2000)
    source_texts = tuple(
        _source_texts(json.loads(run["source_packet_json"])["artifact"])
    )
    for key, limit, fields in [
        ("strengths", 3, {"quote", "reason"}),
        ("findings", 6, {"quote", "observation", "impact", "suggestion"}),
        ("coverage", 120, {"checkId", "status", "reason"}),
    ]:
        items = report[key]
        if not isinstance(items, list) or len(items) > limit:
            raise WorkspaceError("invalid_planning_review", "报告条目数量超出限制")
        for item in items:
            _fields(item, fields)
            for field in fields:
                if field != "suggestion" or item[field] != "":
                    _text(
                        item[field],
                        1200
                        if field == "quote"
                        else 200
                        if field == "checkId"
                        else 1600,
                    )
            if key != "coverage" and not any(
                item["quote"] in source for source in source_texts
            ):
                raise WorkspaceError(
                    "planning_review_quote_invalid", "报告引文必须出自冻结的规划原文"
                )
    expected = set(json.loads(run["checks_json"])[role])
    actual = [item["checkId"] for item in report["coverage"]]
    if (
        len(set(actual)) != len(actual)
        or set(actual) - expected
        or any(
            item["status"] not in {"checked", "unknown", "notChecked"}
            for item in report["coverage"]
        )
    ):
        raise WorkspaceError(
            "planning_review_coverage_invalid", "检查 ID 重复、越界或状态无效"
        )
    return (
        "complete"
        if set(actual) == expected
        and all(item["status"] == "checked" for item in report["coverage"])
        else "partial"
    )


def save_planning_report(db_path, project_id, run_id, payload):
    _fields(payload, {"taskId", "threadId", "expectedRevision", "report"})
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        run = _run(connection, project_id, run_id, writable=True)
        task = require_active_task(
            connection,
            project_id,
            _text(payload["taskId"], 160),
            _text(payload["threadId"], 160),
        )
        if (
            isinstance(payload["expectedRevision"], bool)
            or not isinstance(payload["expectedRevision"], int)
            or payload["expectedRevision"] != task["input_revision"]
        ):
            raise ConflictError("stale_task_revision", "报告版本与冻结任务不一致")
        if (
            task["capability_revoked_at"] is not None
            or not task["capability_expires_at"]
            or task["capability_expires_at"] <= int(time.time())
        ):
            raise WorkspaceError(
                "task_capability_revoked", "评审任务能力已撤销或到期", 401
            )
        seat = connection.execute(
            "SELECT role FROM planning_review_seats WHERE run_id = ? AND task_id = ?",
            (run_id, task["id"]),
        ).fetchone()
        if seat is None or task["operation"] != "reviewPlanningSeat":
            raise WorkspaceError(
                "planning_review_task_mismatch", "任务不能向其他评审或角色交付报告", 401
            )
        if (
            json.loads(task["input_manifest_json"]).get("sourcePacketHash")
            != run["source_packet_hash"]
        ):
            raise ConflictError(
                "planning_review_snapshot_mismatch", "任务与评审资料快照不一致"
            )
        status = _validate_report(payload["report"], run, seat["role"])
        report_json = _json(payload["report"])
        previous = connection.execute(
            "SELECT * FROM planning_review_reports WHERE task_id = ?", (task["id"],)
        ).fetchone()
        if previous:
            if previous["report_json"] != report_json:
                raise ConflictError(
                    "planning_review_report_exists",
                    "已交付报告不可改写，请发起新的评审",
                )
            return _report(previous)
        report_id, timestamp = f"planning-report-{uuid4().hex}", now()
        connection.execute(
            "INSERT INTO planning_review_reports(id, run_id, role, task_id, source_packet_hash, report_json, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                report_id,
                run_id,
                seat["role"],
                task["id"],
                run["source_packet_hash"],
                report_json,
                status,
                timestamp,
            ),
        )
        record_task_output(connection, task["id"], "critique", report_id, status)
        refs = [{"kind": "critique", "revisionId": report_id}]
        connection.execute(
            "UPDATE task_runs SET output_refs_json = ?, updated_at = ? WHERE id = ?",
            (_json(refs), timestamp, task["id"]),
        )
        connection.execute(
            "UPDATE planning_review_runs SET updated_at = ? WHERE id = ?",
            (timestamp, run_id),
        )
        return _report(
            connection.execute(
                "SELECT * FROM planning_review_reports WHERE id = ?", (report_id,)
            ).fetchone()
        )


def finish_planning_review(db_path, project_id, run_id, *, cancel=False):
    initialize(db_path)
    with connect(db_path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        run = _run(connection, project_id, run_id)
        if run["status"] != "open":
            return _view(connection, run)
        view, timestamp = _view(connection, run), now()
        active = [seat for seat in view["seats"] if seat["status"] in ACTIVE_STATUSES]
        if active and not cancel:
            raise ConflictError(
                "planning_review_seat_busy", "请先完成或停止当前顾问任务"
            )
        if cancel:
            for seat in active:
                connection.execute(
                    "UPDATE task_runs SET status = 'cancelled', stage = 'cancelled', delivery_status = 'incomplete', capability_revoked_at = ?, updated_at = ? WHERE id = ?",
                    (timestamp, timestamp, seat["taskId"]),
                )
            status, synthesis = "cancelled", None
        else:
            _run(connection, project_id, run_id, writable=True)
            reports = view["reports"]
            missing = [
                role
                for role in view["roles"]
                if role not in {report["role"] for report in reports}
            ]
            incomplete = (
                set(missing)
                | {
                    report["role"]
                    for report in reports
                    if report["status"] != "complete"
                }
                | {
                    seat["role"]
                    for seat in view["seats"]
                    if seat["status"] != "completed"
                }
            )
            status = "partial" if incomplete else "complete"
            synthesis = {
                "priorities": [
                    {
                        "role": report["role"],
                        "reportId": report["id"],
                        "findingIndex": index,
                    }
                    for report in reports
                    for index, _ in enumerate(report["report"]["findings"])
                ],
                "disagreements": [],
                "incompleteRoles": [
                    role for role in view["roles"] if role in incomplete
                ],
            }
        connection.execute(
            "UPDATE planning_review_runs SET status = ?, synthesis_json = ?, updated_at = ? WHERE id = ?",
            (
                status,
                _json(synthesis) if synthesis is not None else None,
                timestamp,
                run_id,
            ),
        )
        return _view(connection, _run(connection, project_id, run_id))


def valid_completed_report(connection, task, output):
    report = connection.execute(
        "SELECT r.id FROM planning_review_reports r JOIN planning_review_runs g ON g.id = r.run_id "
        "JOIN planning_review_seats s ON s.run_id = r.run_id AND s.role = r.role "
        "WHERE r.id = ? AND r.task_id = ? AND s.task_id = r.task_id AND g.project_id = ? "
        "AND g.status = 'open' AND g.input_revision = ? AND r.source_packet_hash = g.source_packet_hash",
        (output["revision_id"], task["id"], task["project_id"], task["input_revision"]),
    ).fetchone()
    return (
        report is not None
        and task["capability_revoked_at"] is None
        and isinstance(task["capability_expires_at"], int)
        and task["capability_expires_at"] > int(time.time())
    )
