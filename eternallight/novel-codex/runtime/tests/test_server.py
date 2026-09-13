import tempfile
import hashlib
import sqlite3
import time
import unittest
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.server import authorize_candidate_capability, response_payload


class CreativeWorkspaceRouteTests(unittest.TestCase):
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
                    "model": "gpt-5.6-terra",
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
                    "model": "gpt-5.6-terra",
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
