import hashlib
import json
import time
import tempfile
import unittest
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.service import (
    accept_review,
    create_review,
    create_task,
    get_context_snapshot,
    save_artifact,
    save_draft_checkpoint,
)
from runtime.server import authorize_candidate_capability, response_payload
from runtime.creative_workspace.working_baselines import set_working_baseline
from runtime.storage import create_project


class ExperienceContractTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.db = Path(self.directory.name) / "runtime.sqlite3"
        self.project = create_project(self.db, "新作", "潮汐")["project"]["id"]

    def test_regeneration_uses_current_accepted_brief_alongside_outline(self):
        brief = save_artifact(
            self.db, self.project, "brief", {"premise": "作者新方向：归乡"}, 0
        )
        accept_review(
            self.db,
            self.project,
            create_review(self.db, self.project, "brief", brief["revisionId"])["id"],
            0,
            "accept-brief",
        )
        outline = save_artifact(
            self.db,
            self.project,
            "outline",
            {
                "nodes": [
                    {
                        "clientKey": "book",
                        "kind": "book",
                        "position": 0,
                        "title": "旧大纲",
                        "purpose": "归乡",
                        "summary": "退潮",
                        "dependencyKeys": [],
                        "certainty": "settled",
                        "protection": "free",
                    }
                ]
            },
            1,
        )
        accept_review(
            self.db,
            self.project,
            create_review(self.db, self.project, "outline", outline["revisionId"])[
                "id"
            ],
            1,
            "accept-outline",
        )
        for operation in ("generateBrief", "generateOutline"):
            with self.subTest(operation=operation):
                task = create_task(self.db, self.project, operation, 2, operation)
                context = get_context_snapshot(
                    self.db,
                    self.project,
                    "planner",
                    2,
                    byte_budget=60_000,
                    actor="candidate",
                    task_id=task["id"],
                    thread_id=operation,
                )
                self.assertEqual(
                    context["acceptedArtifacts"]["brief"]["id"], brief["revisionId"]
                )
                if operation == "generateOutline":
                    self.assertEqual(
                        context["acceptedArtifacts"]["outline"]["id"],
                        outline["revisionId"],
                    )
                self.assertLessEqual(context["contextBudget"]["usedBytes"], 60_000)

    def test_thread_lookup_finds_old_tasks_and_excludes_other_projects(self):
        first = create_task(self.db, self.project, "advisory", 0, "old-thread")
        for index in range(31):
            create_task(self.db, self.project, "advisory", 0, f"thread-{index}")
        other = create_project(self.db, "另一作品", "")["project"]["id"]
        create_task(self.db, other, "advisory", 0, "foreign-thread")
        status, result = response_payload(
            f"/api/projects/{self.project}/tasks/lookup",
            "POST",
            {"threadIds": ["old-thread", "foreign-thread"]},
            str(self.db),
        )
        self.assertEqual(status, 200)
        self.assertEqual([task["id"] for task in result["tasks"]], [first["id"]])
        newer = create_task(self.db, self.project, "generateBrief", 0, "old-thread")
        _, result = response_payload(
            f"/api/projects/{self.project}/tasks/lookup",
            "POST",
            {"threadIds": ["old-thread", "old-thread"]},
            str(self.db),
        )
        self.assertEqual([task["id"] for task in result["tasks"]], [newer["id"]])
        self.assertEqual(result["tasks"][0]["operation"], "generateBrief")
        with self.assertRaises(WorkspaceError):
            response_payload(
                f"/api/projects/{self.project}/tasks/lookup",
                "POST",
                {"threadIds": ["old-thread"]},
                str(self.db),
                actor="candidate",
            )
        with self.assertRaises(WorkspaceError):
            response_payload(
                f"/api/projects/{self.project}/tasks/lookup",
                "POST",
                {"threadIds": ["x"] * 101},
                str(self.db),
            )

    def test_recovery_lookup_uses_exact_task_identity_and_project_scope(self):
        task = create_task(self.db, self.project, "advisory", 0, "shared-thread")
        newer = create_task(self.db, self.project, "generateBrief", 0, "shared-thread")
        other = create_project(self.db, "另一作品", "")["project"]["id"]
        foreign = create_task(self.db, other, "advisory", 0, "foreign-thread")

        status, result = response_payload(
            f"/api/projects/{self.project}/tasks/recovery-lookup",
            "POST",
            {"taskIds": [task["id"], newer["id"], foreign["id"]]},
            str(self.db),
        )

        self.assertEqual(status, 200)
        self.assertEqual(
            [entry["id"] for entry in result["tasks"]],
            [task["id"], newer["id"]],
        )
        self.assertEqual(result["tasks"][0]["status"], "queued")
        self.assertEqual(result["tasks"][0]["deliveryStatus"], "pending")
        self.assertEqual(result["tasks"][0]["outputKinds"], [])
        with self.assertRaises(WorkspaceError):
            response_payload(
                f"/api/projects/{self.project}/tasks/recovery-lookup",
                "POST",
                {"taskIds": [task["id"]]},
                str(self.db),
                actor="candidate",
            )

    def test_scoped_checkpoint_keeps_locked_accepted_tail_and_retry_identity(self):
        groups = [
            {"id": "start", "body": "原开头。"},
            {"id": "end", "body": "锁定结尾。", "locked": True},
        ]
        initial = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterNumber": 1,
                "body": "原开头。\n\n---\n\n锁定结尾。",
                "sceneGroups": groups,
            },
            0,
        )
        review = create_review(self.db, self.project, "draft", initial["revisionId"])
        accept_review(
            self.db,
            self.project,
            review["id"],
            0,
            "accept-draft",
            selections={"noConfirmedChanges": True},
        )
        task = create_task(
            self.db,
            self.project,
            "generateChapter",
            1,
            "scoped",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "scope": {"type": "throughScene", "endSceneOrder": 1},
            },
        )
        payload = {
            "chapterId": initial["chapterId"],
            "sceneOrder": 1,
            "body": "新开头。",
            "sceneGroups": [{"id": "start", "body": "新开头。"}],
        }
        args = (
            self.db,
            self.project,
            payload,
            1,
            "candidate",
            task["id"],
            "scoped",
            initial["revisionId"],
            "scoped-save",
            True,
        )
        saved = save_draft_checkpoint(*args)
        self.assertEqual(save_draft_checkpoint(*args), saved)
        self.assertEqual(saved["body"], "新开头。\n\n---\n\n锁定结尾。")
        self.assertEqual(
            saved["sceneGroups"][-1],
            {
                "id": "end",
                "body": "锁定结尾。",
                "locked": True,
                "baseAcceptedRevisionId": initial["revisionId"],
                "candidateDraftRevisionId": saved["revisionId"],
            },
        )

    def test_scoped_prefix_cannot_move_secret_tail_into_next_context(self):
        initial = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterNumber": 1,
                "body": "A\n\n---\n\nB\n\n---\n\nSECRET_CANARY",
                "sceneGroups": [
                    {"id": "a", "body": "A"},
                    {"id": "b", "body": "B"},
                    {"id": "secret", "body": "SECRET_CANARY"},
                ],
            },
            0,
        )
        set_working_baseline(
            self.db,
            self.project,
            "draft",
            initial["chapterId"],
            initial["revisionId"],
            0,
        )
        task = create_task(
            self.db,
            self.project,
            "generateChapter",
            0,
            "scope-canary",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "scope": {"type": "throughScene", "endSceneOrder": 2},
            },
        )
        with self.assertRaises(WorkspaceError) as failure:
            save_draft_checkpoint(
                self.db,
                self.project,
                {
                    "chapterId": initial["chapterId"],
                    "sceneOrder": 2,
                    "body": "Anew",
                    "sceneGroups": [{"id": "a", "body": "Anew"}],
                },
                0,
                "candidate",
                task["id"],
                "scope-canary",
                initial["revisionId"],
                "shrink-prefix",
                True,
            )
        self.assertEqual(failure.exception.code, "checkpoint_scope_exceeded")
        saved = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterId": initial["chapterId"],
                "sceneOrder": 2,
                "body": "Anew\n\n---\n\nBnew",
                "sceneGroups": [
                    {"id": "a", "body": "Anew"},
                    {"id": "b", "body": "Bnew"},
                ],
            },
            0,
            "candidate",
            task["id"],
            "scope-canary",
            initial["revisionId"],
            "retain-prefix",
            True,
        )
        context = get_context_snapshot(
            self.db,
            self.project,
            "writer",
            0,
            reading_chapter=1,
            scene_order=2,
            actor="candidate",
            task_id=task["id"],
            thread_id="scope-canary",
        )
        self.assertEqual(
            context["sourceExcerpts"][0]["draftRevisionId"], initial["revisionId"]
        )
        self.assertEqual(context["sourceExcerpts"][0]["body"], "A\n\n---\n\nB")
        self.assertNotIn("SECRET_CANARY", str(context))
        self.assertNotIn("secret", str(context["sourceExcerpts"][0]))

    def test_candidate_checkpoint_http_reply_and_retry_hide_retained_tail(self):
        initial = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterNumber": 1,
                "body": "Visible\n\n---\n\nHTTP_SECRET_CANARY",
                "sceneGroups": [
                    {"id": "a", "body": "Visible"},
                    {"id": "hidden", "body": "HTTP_SECRET_CANARY"},
                ],
            },
            0,
        )
        capability = "test-scoped-capability-123456789"
        task = create_task(
            self.db,
            self.project,
            "generateChapter",
            0,
            "http-scope",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "scope": {"type": "throughScene", "endSceneOrder": 1},
            },
            capability_hash=hashlib.sha256(capability.encode()).hexdigest(),
            capability_expires_at=int(time.time()) + 600,
        )
        request = {
            "taskId": task["id"],
            "threadId": "http-scope",
            "expectedRevision": 0,
            "expectedCandidateRevisionId": initial["revisionId"],
            "idempotencyKey": "http-scope-save",
            "payload": {
                "chapterId": initial["chapterId"],
                "sceneOrder": 1,
                "body": "Changed",
                "sceneGroups": [{"id": "a", "body": "Changed"}],
            },
        }
        args = (
            f"/api/projects/{self.project}/drafts/checkpoints",
            "POST",
            request,
            str(self.db),
        )
        authorize_candidate_capability(str(self.db), args[0], request, capability)
        status, first = response_payload(*args, actor="candidate")
        self.assertEqual(status, 201)
        authorize_candidate_capability(str(self.db), args[0], request, capability)
        self.assertEqual(response_payload(*args, actor="candidate"), (status, first))
        self.assertNotIn("HTTP_SECRET_CANARY", str(first))
        self.assertNotIn("body", first)
        self.assertNotIn("sceneGroups", first)
