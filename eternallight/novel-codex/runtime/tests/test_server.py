import tempfile
import hashlib
import sqlite3
import subprocess
import sys
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime.creative_workspace.errors import WorkspaceError
from runtime.server import (
    _execute_request,
    authorize_candidate_capability,
    response_payload,
)
from runtime.creative_workspace.operation_ledger import (
    is_committed_operation_replay,
    recovery_snapshot,
)


class CreativeWorkspaceRouteTests(unittest.TestCase):
    def test_domain_write_and_operation_receipt_share_one_transaction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            _, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "generateBrief",
                    "inputRevision": 0,
                    "threadId": "atomic-root",
                    "model": "test-model",
                    "target": {"type": "project"},
                    "authorIntent": "形成创作简报",
                    "capabilityHash": hashlib.sha256(b"atomic-capability").hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            payload = {
                "taskId": task["id"],
                "threadId": "atomic-root",
                "expectedRevision": 0,
                "expectedCandidateRevisionId": None,
                "idempotencyKey": "atomic-write",
                "payload": {
                    "premise": "原子提交",
                    "promise": "验证",
                    "stakes": "不能留下孤立候选",
                    "endingDirection": "回滚",
                },
            }
            headers = {
                "root_thread_id": "atomic-root",
                "caller_thread_id": "atomic-root",
                "parent_thread_id": None,
                "turn_id": "turn-atomic",
                "tool_call_id": "call-atomic",
                "tool_name": "save_brief",
            }
            with (
                patch(
                    "runtime.server.commit_operation",
                    side_effect=WorkspaceError(
                        "forced_receipt_failure", "强制回执失败"
                    ),
                ),
                self.assertRaisesRegex(WorkspaceError, "强制回执失败"),
            ):
                _execute_request(
                    db_path,
                    "/api/projects/project-last-lamp/artifacts/brief",
                    "POST",
                    payload,
                    "candidate",
                    headers,
                )
            with sqlite3.connect(db_path) as connection:
                candidate = connection.execute(
                    "SELECT candidate_revision_id FROM artifact_pointers WHERE project_id = ? AND artifact_kind = 'brief'",
                    ("project-last-lamp",),
                ).fetchone()[0]
                operation = connection.execute(
                    "SELECT state, error_code FROM runtime_operations WHERE tool_call_id = 'call-atomic'"
                ).fetchone()
            self.assertIsNone(candidate)
            self.assertEqual(operation, ("rejected", "forced_receipt_failure"))

    def test_runtime_operation_ledger_is_authoritative_for_candidate_writes(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            _, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "generateBrief",
                    "inputRevision": 0,
                    "threadId": "root-thread",
                    "model": "test-model",
                    "target": {"type": "project"},
                    "authorIntent": "形成创作简报",
                    "capabilityHash": hashlib.sha256(b"ledger-capability").hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            payload = {
                "taskId": task["id"],
                "threadId": "root-thread",
                "expectedRevision": 0,
                "expectedCandidateRevisionId": None,
                "idempotencyKey": "brief-ledger-test",
                "payload": {
                    "premise": "记忆是否值得保存",
                    "promise": "悬疑",
                    "stakes": "城市遗忘",
                    "endingDirection": "留下证词",
                },
            }
            headers = {
                "root_thread_id": "root-thread",
                "caller_thread_id": "root-thread",
                "parent_thread_id": None,
                "turn_id": "turn-1",
                "tool_call_id": "call-1",
                "tool_name": "save_brief",
            }
            status, saved = _execute_request(
                db_path,
                "/api/projects/project-last-lamp/artifacts/brief",
                "POST",
                payload,
                "candidate",
                headers,
            )
            self.assertEqual(status, 201)
            replay_status, replay = _execute_request(
                db_path,
                "/api/projects/project-last-lamp/artifacts/brief",
                "POST",
                payload,
                "candidate",
                headers,
            )
            self.assertEqual(replay_status, 201)
            self.assertEqual(replay, saved)
            with sqlite3.connect(db_path) as connection:
                connection.row_factory = sqlite3.Row
                rows = connection.execute(
                    "SELECT state, root_thread_id, caller_thread_id, tool_name, receipt_ref FROM runtime_operations"
                ).fetchall()
            self.assertRegex(rows[0]["receipt_ref"], r"^[0-9a-f]{64}$")
            self.assertEqual(
                [dict(row) for row in rows],
                [
                    {
                        "state": "committed",
                        "root_thread_id": "root-thread",
                        "caller_thread_id": "root-thread",
                        "tool_name": "save_brief",
                        "receipt_ref": rows[0]["receipt_ref"],
                    }
                ],
            )
            self.assertEqual(saved["artifactKind"], "brief")

            response_payload(
                f"/api/projects/project-last-lamp/tasks/{task['id']}/complete",
                "POST",
                {"threadId": "root-thread", "summary": "完成"},
                db_path,
            )
            self.assertTrue(
                is_committed_operation_replay(
                    db_path,
                    "/api/projects/project-last-lamp/artifacts/brief",
                    "POST",
                    payload,
                    headers,
                )
            )
            replay_status, replay = _execute_request(
                db_path,
                "/api/projects/project-last-lamp/artifacts/brief",
                "POST",
                payload,
                "candidate",
                headers,
            )
            self.assertEqual((replay_status, replay), (201, saved))

            with self.assertRaisesRegex(WorkspaceError, "根线程"):
                _execute_request(
                    db_path,
                    "/api/projects/project-last-lamp/artifacts/brief",
                    "POST",
                    {**payload, "idempotencyKey": "child-write"},
                    "candidate",
                    {
                        **headers,
                        "caller_thread_id": "child-thread",
                        "tool_call_id": "call-child",
                    },
                )

    def test_recovery_snapshot_revokes_authority_and_closes_proven_unexecuted_operations(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            _, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "generateBrief",
                    "inputRevision": 0,
                    "threadId": "snapshot-root",
                    "model": "test-model",
                    "target": {"type": "project"},
                    "authorIntent": "形成创作简报",
                    "capabilityHash": hashlib.sha256(
                        b"snapshot-capability"
                    ).hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            with sqlite3.connect(db_path) as connection:
                now = "2026-09-20T00:00:00.000+00:00"
                connection.execute(
                    "INSERT INTO runtime_operations(operation_id, run_id, project_id, root_thread_id, caller_thread_id, tool_call_id, tool_name, request_hash, state, authority_epoch, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'accepted', ?, ?, ?)",
                    (
                        "operation-pending",
                        task["id"],
                        "project-last-lamp",
                        "snapshot-root",
                        "snapshot-root",
                        "call-pending",
                        "save_brief",
                        "a" * 64,
                        hashlib.sha256(b"snapshot-capability").hexdigest(),
                        now,
                        now,
                    ),
                )
            result = recovery_snapshot(db_path, "project-last-lamp", [task["id"]])
            self.assertTrue(result["complete"])
            self.assertEqual(
                result["operations"][0]["state"], "cancelled-before-execution"
            )
            self.assertEqual(result["tasks"][0]["id"], task["id"])
            with sqlite3.connect(db_path) as connection:
                revoked = connection.execute(
                    "SELECT capability_revoked_at FROM task_runs WHERE id = ?",
                    (task["id"],),
                ).fetchone()[0]
            self.assertIsNotNone(revoked)

    def test_recovery_finds_commit_after_runtime_reply_but_before_host_journal(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            _, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "generateBrief",
                    "inputRevision": 0,
                    "threadId": "crash-root",
                    "model": "test-model",
                    "target": {"type": "project"},
                    "authorIntent": "形成创作简报",
                    "capabilityHash": hashlib.sha256(b"crash-capability").hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            fixture = Path(__file__).with_name("fixture_commit_then_crash.py")
            process = subprocess.run(
                [sys.executable, str(fixture), db_path, task["id"]],
                cwd=Path(__file__).parents[2],
                check=False,
            )
            self.assertEqual(process.returncode, 19)
            snapshot = recovery_snapshot(db_path, "project-last-lamp", [task["id"]])
            self.assertEqual(snapshot["operations"][0]["state"], "committed")
            self.assertRegex(
                snapshot["operations"][0]["receipt_ref"], r"^[a-f0-9]{64}$"
            )
            with sqlite3.connect(db_path) as connection:
                candidates = connection.execute(
                    "SELECT COUNT(*) FROM artifact_revisions WHERE project_id = ? AND artifact_kind = 'brief'",
                    ("project-last-lamp",),
                ).fetchone()[0]
            self.assertEqual(candidates, 1)

    def test_candidate_writes_require_explicit_candidate_cas(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            for path, payload in (
                (
                    "/api/projects/project-last-lamp/artifacts/brief",
                    {
                        "expectedRevision": 0,
                        "idempotencyKey": "missing-artifact-cas",
                        "payload": {"premise": "a"},
                    },
                ),
                (
                    "/api/projects/project-last-lamp/drafts/checkpoints",
                    {
                        "expectedRevision": 0,
                        "idempotencyKey": "missing-draft-cas",
                        "payload": {"chapterNumber": 1, "body": "a"},
                    },
                ),
            ):
                with self.assertRaisesRegex(
                    WorkspaceError, "expectedCandidateRevisionId 必须显式提供"
                ):
                    response_payload(path, "POST", payload, db_path)

    def test_candidate_capability_is_bound_to_task_project_thread_ttl_and_status(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            secret = "task-capability-secret-1234567890"
            capability_hash = hashlib.sha256(secret.encode()).hexdigest()
            status, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "advisory",
                    "inputRevision": 0,
                    "threadId": "thread-capability",
                    "model": "deepseek-flash",
                    "target": {"type": "project"},
                    "authorIntent": "讨论故事方向",
                    "capabilityHash": capability_hash,
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            self.assertEqual(status, 201)
            path = (
                "/api/projects/project-last-lamp/context?"
                f"taskId={task['id']}&threadId=thread-capability&projectRevision=0"
            )
            authorize_candidate_capability(db_path, path, None, secret)
            with self.assertRaisesRegex(WorkspaceError, "能力凭证无效"):
                authorize_candidate_capability(
                    db_path, path, None, "wrong-capability-secret-1234567890"
                )
            with self.assertRaisesRegex(WorkspaceError, "作品或线程不匹配"):
                authorize_candidate_capability(
                    db_path,
                    path.replace("thread-capability", "thread-other"),
                    None,
                    secret,
                )

            response_payload(
                f"/api/projects/project-last-lamp/tasks/{task['id']}/complete",
                "POST",
                {"threadId": "thread-capability", "summary": "讨论完成"},
                db_path,
            )
            with self.assertRaisesRegex(WorkspaceError, "已撤销"):
                authorize_candidate_capability(db_path, path, None, secret)

            with sqlite3.connect(db_path) as connection:
                connection.execute(
                    "UPDATE task_runs SET status = 'queued', capability_revoked_at = NULL, capability_expires_at = ? WHERE id = ?",
                    (int(time.time()) - 1, task["id"]),
                )
            with self.assertRaisesRegex(WorkspaceError, "已过期"):
                authorize_candidate_capability(db_path, path, None, secret)

    def test_candidate_review_and_author_accept_share_one_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            status, workspace = response_payload(
                "/api/projects/project-last-lamp/creative-workspace",
                "GET",
                None,
                db_path,
            )
            self.assertEqual(status, 200)
            self.assertEqual(workspace["revision"], 0)
            status, candidate = response_payload(
                "/api/projects/project-last-lamp/artifacts/brief",
                "POST",
                {
                    "expectedRevision": 0,
                    "expectedCandidateRevisionId": None,
                    "idempotencyKey": "save-route-brief",
                    "payload": {
                        "premise": "记忆是否值得保存",
                        "promise": "悬疑",
                        "stakes": "城市遗忘",
                        "endingDirection": "留下证词",
                    },
                },
                db_path,
            )
            self.assertEqual(status, 201)
            status, review = response_payload(
                "/api/projects/project-last-lamp/reviews",
                "POST",
                {
                    "artifactKind": "brief",
                    "revisionId": candidate["revisionId"],
                    "idempotencyKey": "review-route-brief",
                },
                db_path,
            )
            self.assertEqual(status, 201)
            status, receipt = response_payload(
                f"/api/projects/project-last-lamp/reviews/{review['id']}/accept",
                "POST",
                {"expectedRevision": 0, "idempotencyKey": "route-accept"},
                db_path,
            )
            self.assertEqual(status, 200)
            self.assertEqual(receipt["resultRevision"], 1)
            status, lock = response_payload(
                "/api/projects/project-last-lamp/artifacts/brief/lock",
                "PATCH",
                {"expectedRevision": 1, "locked": True},
                db_path,
            )
            self.assertEqual(status, 200)
            self.assertEqual(
                lock, {"artifactKind": "brief", "locked": True, "resultRevision": 2}
            )

            status, impacts = response_payload(
                "/api/projects/project-last-lamp/impacts?limit=10",
                "GET",
                None,
                db_path,
            )
            self.assertEqual(status, 200)
            self.assertEqual(
                impacts,
                {
                    "data": [],
                    "nextCursor": None,
                    "counts": {
                        "total": 0,
                        "open": 0,
                        "retained": 0,
                        "lockedConflict": 0,
                    },
                },
            )

    def test_candidate_cannot_move_task_status_through_author_route(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            with self.assertRaisesRegex(WorkspaceError, "只有作者"):
                response_payload(
                    "/api/projects/project-last-lamp/tasks/task-1",
                    "PATCH",
                    {"threadId": "thread-1", "status": "failed"},
                    db_path,
                    "candidate",
                )

    def test_author_startup_recovery_cancels_orphaned_tasks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            _, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "advisory",
                    "inputRevision": 0,
                    "threadId": "thread-orphan",
                    "model": "deepseek-flash",
                    "target": {"type": "project"},
                    "authorIntent": "继续未完成的讨论",
                    "capabilityHash": hashlib.sha256(b"orphan-capability").hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            status, recovered = response_payload(
                "/api/tasks/recover-orphans", "POST", {}, db_path
            )
            self.assertEqual(status, 200)
            self.assertEqual(recovered, {"recovered": 1, "taskIds": [task["id"]]})
            with self.assertRaisesRegex(WorkspaceError, "只有作者"):
                response_payload(
                    "/api/tasks/recover-orphans", "POST", {}, db_path, "candidate"
                )

    def test_task_creation_accepts_only_operation_owned_contract_fields(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = str(Path(directory) / "runtime.sqlite3")
            status, task = response_payload(
                "/api/projects/project-last-lamp/tasks",
                "POST",
                {
                    "operation": "generateOutline",
                    "inputRevision": 0,
                    "threadId": "thread-outline",
                    "model": "test-model",
                    "target": {"type": "artifact"},
                    "authorIntent": "生成新的大纲候选",
                    "capabilityHash": hashlib.sha256(b"outline-capability").hexdigest(),
                    "capabilityExpiresAt": int(time.time()) + 600,
                },
                db_path,
            )
            self.assertEqual(status, 201)
            self.assertEqual(
                {
                    "kind": task["kind"],
                    "purpose": task["purpose"],
                    "requiredOutputKind": task["requiredOutputKind"],
                },
                {
                    "kind": "outline",
                    "purpose": "planner",
                    "requiredOutputKind": "outline",
                },
            )
            with self.assertRaisesRegex(WorkspaceError, "不可控制的字段"):
                response_payload(
                    "/api/projects/project-last-lamp/tasks",
                    "POST",
                    {
                        "operation": "generateOutline",
                        "kind": "draft",
                        "purpose": "writer",
                        "requiredOutputKind": "draft",
                        "inputRevision": 0,
                        "threadId": "thread-escalation",
                    },
                    db_path,
                )


if __name__ == "__main__":
    unittest.main()
