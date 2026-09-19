import copy
import hashlib
import json
import tempfile
import time
import unittest
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.generation import (
    complete_task,
    recover_orphaned_tasks,
    update_task,
)
from runtime.creative_workspace.service import (
    get_context_snapshot,
    save_artifact,
    workspace_state,
)
from runtime.creative_workspace.working_baselines import set_working_baseline
from runtime.server import authorize_candidate_capability, response_payload
from runtime.storage import connect, create_project


class PlanningReviewTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.db = Path(self.directory.name) / "review.sqlite3"
        self.project = create_project(self.db, "规划深审")["project"]["id"]
        self.outline = save_artifact(
            self.db,
            self.project,
            "outline",
            {
                "nodes": [
                    {
                        "clientKey": "book",
                        "kind": "book",
                        "position": 0,
                        "title": "失去名字的人",
                        "purpose": "追问身份",
                        "summary": "她为了救人交出名字。",
                        "dependencyKeys": [],
                        "certainty": "tentative",
                        "protection": "free",
                    }
                ]
            },
            0,
        )

    def request(self, suffix, body=None, *, actor="author", method="POST"):
        return response_payload(
            f"/api/projects/{self.project}/{suffix}", method, body, str(self.db), actor
        )[1]

    def run_review(self, **changes):
        return self.request(
            "planning-reviews",
            {
                "artifactKind": "outline",
                "revisionId": self.outline["revisionId"],
                "authorIntent": "关注动机与因果，不替作者决定",
                "roles": ["structure", "experience"],
                **changes,
            },
        )

    def seat(self, run, role, *, thread=None):
        return self.request(
            f"planning-reviews/{run['id']}/seats/{role}/tasks",
            {
                "threadId": thread or f"thread-{role}",
                "model": "test-model",
                "capabilityHash": hashlib.sha256((role * 10).encode()).hexdigest(),
                "capabilityExpiresAt": int(time.time()) + 600,
            },
        )

    def report(self, run, role):
        return {
            "summary": "此处选择有代价，但其因果值得进一步检查。",
            "strengths": [{"quote": "交出名字", "reason": "代价具体"}],
            "findings": [
                {
                    "quote": "为了救人",
                    "observation": "动机尚未展开",
                    "impact": "选择说服力存在不确定",
                    "suggestion": "",
                }
            ],
            "coverage": [
                {"checkId": check, "status": "checked", "reason": "已检查规划原文"}
                for check in run["checksByRole"][role]
            ],
        }

    def submit(self, run, task, report):
        return self.request(
            f"planning-reviews/{run['id']}/reports",
            {
                "taskId": task["id"],
                "threadId": task["threadId"],
                "expectedRevision": task["inputRevision"],
                "report": report,
            },
            actor="candidate",
        )

    def finish_task(self, task):
        return complete_task(
            self.db, self.project, task["id"], task["threadId"], "顾问报告已交付"
        )

    def context(self, task):
        return get_context_snapshot(
            self.db,
            self.project,
            purpose="planner",
            project_revision=0,
            actor="candidate",
            task_id=task["id"],
            thread_id=task["threadId"],
        )

    def test_unaccepted_planning_frozen_independent_seats_and_deterministic_synthesis(
        self,
    ):
        run = self.run_review()
        self.assertEqual((run["status"], run["coverageStatus"]), ("running", "pending"))
        first = self.seat(run, "structure")
        source = self.context(first)
        self.assertEqual(source["checks"], run["checksByRole"]["structure"])
        saved = self.submit(run, first, self.report(run, "structure"))
        self.assertEqual(saved, self.submit(run, first, self.report(run, "structure")))
        completed = self.finish_task(first)
        self.assertEqual(
            completed["outputRefs"], [{"kind": "critique", "revisionId": saved["id"]}]
        )
        changed = copy.deepcopy(self.outline["payload"])
        changed["nodes"][0]["summary"] = "新的候选改变一切。"
        save_artifact(self.db, self.project, "outline", changed, 0)
        second = self.seat(run, "experience")
        second_source = self.context(second)
        self.assertEqual(source["sourcePacket"], second_source["sourcePacket"])
        self.assertEqual(source["sourcePacketHash"], second_source["sourcePacketHash"])
        self.assertNotIn(saved["id"], json.dumps(second_source, ensure_ascii=False))
        self.assertNotEqual(
            source["inputSnapshot"]["contentHash"],
            second_source["inputSnapshot"]["contentHash"],
        )
        second_report = self.submit(run, second, self.report(run, "experience"))
        self.finish_task(second)
        result = self.request(f"planning-reviews/{run['id']}/finalize", {})
        self.assertEqual(
            (result["status"], result["coverageStatus"]), ("completed", "complete")
        )
        self.assertEqual(
            result["synthesis"],
            {
                "priorities": [
                    {
                        "role": "experience",
                        "reportId": second_report["id"],
                        "findingIndex": 0,
                    },
                    {"role": "structure", "reportId": saved["id"], "findingIndex": 0},
                ],
                "disagreements": [],
                "incompleteRoles": [],
            },
        )
        self.assertEqual(
            result, self.request(f"planning-reviews/{run['id']}/finalize", {})
        )
        workspace = workspace_state(self.db, self.project)
        self.assertEqual(workspace["chapters"], [])
        self.assertIsNone(workspace["artifacts"]["outline"]["accepted"])
        self.assertEqual(workspace["planningReviews"], [result])

    def test_missing_coverage_is_partial_but_valid_delivery(self):
        run, report_status = self.run_review(), "partial"
        task = self.seat(run, "structure")
        report = self.report(run, "structure")
        report["coverage"] = []
        self.assertEqual(self.submit(run, task, report)["status"], report_status)
        self.assertEqual(self.finish_task(task)["status"], "completed")
        finalized = self.request(f"planning-reviews/{run['id']}/finalize", {})
        self.assertEqual(finalized["status"], "partial")
        self.assertEqual(finalized["coverageStatus"], "partial")
        self.assertEqual(finalized["synthesis"]["incompleteRoles"], run["roles"])

    def test_report_checks_evidence_coverage_and_immutability(self):
        run = self.run_review()
        task = self.seat(run, "structure")
        for mutation in ("quote", "duplicate", "extra", "oversize", "summary"):
            with self.subTest(mutation=mutation):
                report = self.report(run, "structure")
                if mutation == "quote":
                    report["findings"][0]["quote"] = "并不存在的证据"
                elif mutation == "duplicate":
                    report["coverage"] *= 2
                elif mutation == "extra":
                    report["coverage"][0]["checkId"] = "other-role:invented"
                elif mutation == "oversize":
                    report["strengths"] *= 4
                else:
                    report["summary"] = "x" * 2001
                with self.assertRaises(WorkspaceError):
                    self.submit(run, task, report)
        report = self.report(run, "structure")
        report["coverage"][0]["status"] = "unknown"
        self.assertEqual(self.submit(run, task, report)["status"], "partial")
        with self.assertRaisesRegex(WorkspaceError, "不可改写"):
            self.submit(run, task, self.report(run, "structure"))
        with connect(self.db) as connection:
            self.assertEqual(
                connection.execute(
                    "SELECT COUNT(*) FROM task_outputs WHERE task_id = ?", (task["id"],)
                ).fetchone()[0],
                1,
            )

    def test_permissions_single_seat_and_no_candidate_writes(self):
        run = self.run_review()
        task = self.seat(run, "structure")
        with self.assertRaisesRegex(WorkspaceError, "当前顾问"):
            self.seat(run, "experience")
        with self.assertRaisesRegex(WorkspaceError, "当前顾问"):
            self.request(f"planning-reviews/{run['id']}/finalize", {})
        with self.assertRaises(WorkspaceError):
            save_artifact(
                self.db,
                self.project,
                "outline",
                self.outline["payload"],
                0,
                "candidate",
                task["id"],
                task["threadId"],
            )
        other_run = self.run_review()
        with self.assertRaisesRegex(WorkspaceError, "其他评审"):
            self.submit(other_run, task, self.report(other_run, "structure"))
        with self.assertRaises(WorkspaceError):
            self.request(
                f"planning-reviews/{run['id']}", actor="candidate", method="GET"
            )
        with self.assertRaises(WorkspaceError):
            self.request(f"planning-reviews/{run['id']}/reports", {})
        self.submit(run, task, self.report(run, "structure"))
        self.finish_task(task)
        with self.assertRaisesRegex(WorkspaceError, "独立的新线程"):
            self.seat(run, "experience", thread=task["threadId"])

    def test_cancel_revokes_read_write_complete_and_preserves_report(self):
        run = self.run_review()
        task = self.seat(run, "structure")
        report = self.submit(run, task, self.report(run, "structure"))
        body = {
            "taskId": task["id"],
            "threadId": task["threadId"],
            "expectedRevision": 0,
        }
        url = f"/api/projects/{self.project}/planning-reviews/{run['id']}/reports"
        authorize_candidate_capability(str(self.db), url, body, "structure" * 10)
        result = self.request(f"planning-reviews/{run['id']}/cancel", {})
        self.assertEqual(result["reports"], [report])
        self.assertEqual(result["status"], "cancelled")
        self.assertEqual(result["coverageStatus"], "partial")
        for action in (
            lambda: self.context(task),
            lambda: self.submit(run, task, self.report(run, "structure")),
            lambda: self.finish_task(task),
            lambda: self.seat(run, "experience"),
            lambda: update_task(
                self.db, self.project, task["id"], task["threadId"], "running"
            ),
            lambda: authorize_candidate_capability(
                str(self.db), url, body, "structure" * 10
            ),
        ):
            with self.assertRaises(WorkspaceError):
                action()

    def test_missing_delivery_recovery_and_stale_project(self):
        run = self.run_review()
        task = self.seat(run, "structure")
        with self.assertRaises(WorkspaceError):
            self.finish_task(task)
        retried = self.seat(run, "structure", thread="retry-structure")
        recover_orphaned_tasks(self.db)
        restored = self.request(f"planning-reviews/{run['id']}", method="GET")
        self.assertEqual(
            next(
                seat["status"]
                for seat in restored["seats"]
                if seat["role"] == "structure"
            ),
            "cancelled",
        )
        with self.assertRaises(WorkspaceError):
            self.context(retried)
        retry_again = self.seat(run, "structure", thread="retry-again")
        with connect(self.db) as connection:
            connection.execute(
                "UPDATE project_workspace SET revision = 1 WHERE project_id = ?",
                (self.project,),
            )
        for action in (
            lambda: self.context(retry_again),
            lambda: self.submit(run, retry_again, self.report(run, "structure")),
            lambda: self.finish_task(retry_again),
            lambda: self.request(f"planning-reviews/{run['id']}/finalize", {}),
        ):
            with self.assertRaises(WorkspaceError):
                action()
        self.assertEqual(
            self.request(f"planning-reviews/{run['id']}/cancel", {})["status"],
            "cancelled",
        )

    def test_unaccepted_plan_scope_and_input_budget_preflight(self):
        set_working_baseline(
            self.db, self.project, "outline", "outline", self.outline["revisionId"], 0
        )
        plan = save_artifact(
            self.db,
            self.project,
            "plan",
            {
                "sourceOutlineRevisionId": self.outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "first",
                        "primaryNodeId": self.outline["payload"]["nodes"][0]["id"],
                        "chapterNumber": 1,
                        "goal": "迫使她交出名字",
                        "dependencyKeys": [],
                    }
                ],
            },
            0,
        )
        run = self.run_review(
            artifactKind="plan",
            revisionId=plan["revisionId"],
            targetNodeId=plan["payload"]["chapters"][0]["chapterPlanId"],
        )
        self.assertEqual(len(run["checksByRole"]["structure"]), 1)
        with self.assertRaisesRegex(WorkspaceError, "不属于"):
            self.run_review(targetNodeId="invented")
        with connect(self.db) as connection:
            large = copy.deepcopy(self.outline["payload"])
            large["nodes"][0]["summary"] = "大" * 30_000
            connection.execute(
                "UPDATE artifact_revisions SET payload_json = ? WHERE id = ?",
                (json.dumps(large), self.outline["revisionId"]),
            )
        with self.assertRaisesRegex(WorkspaceError, "输入预算"):
            self.run_review()
        with connect(self.db) as connection:
            self.assertEqual(
                connection.execute(
                    "SELECT COUNT(*) FROM planning_review_runs"
                ).fetchone()[0],
                1,
            )

    def test_expiry_and_task_hash_do_not_allow_late_report_delivery(self):
        run = self.run_review()
        task = self.seat(run, "structure")
        with connect(self.db) as connection:
            connection.execute(
                "UPDATE task_runs SET input_manifest_json = '{}' WHERE id = ?",
                (task["id"],),
            )
        with self.assertRaisesRegex(WorkspaceError, "快照不一致"):
            self.submit(run, task, self.report(run, "structure"))
        with connect(self.db) as connection:
            connection.execute(
                "UPDATE task_runs SET input_manifest_json = ?, capability_expires_at = ? WHERE id = ?",
                (json.dumps(task["inputManifest"]), int(time.time()) - 1, task["id"]),
            )
        with self.assertRaisesRegex(WorkspaceError, "到期"):
            self.submit(run, task, self.report(run, "structure"))
        result = self.request(f"planning-reviews/{run['id']}/cancel", {})
        self.assertEqual(
            (result["status"], result["coverageStatus"]), ("cancelled", "pending")
        )


if __name__ == "__main__":
    unittest.main()
