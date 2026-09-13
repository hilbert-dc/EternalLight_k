import json
import sqlite3
import tempfile
import unittest
from uuid import uuid4
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.metrics import METRICS, local_metrics
from runtime.creative_workspace.service import (
    accept_review,
    create_review,
    save_artifact,
    save_draft_checkpoint,
)
from runtime.creative_workspace.planning_bindings import materialize_standalone_chapter
from runtime.creative_workspace.impacts import resolve_planning_impact
from runtime.creative_workspace.generation import cancel_task, create_task
from runtime.server import response_payload
from runtime.storage import create_project


class LocalMetricsTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.db_path = Path(self.directory.name) / "metrics.sqlite3"
        self.secret = "SECRET-METRICS-CANARY"
        self.project_id = create_project(self.db_path, self.secret, self.secret)[
            "project"
        ]["id"]

    def summary(self):
        return {
            row["metric"]: (row["count"], row["totalSeconds"])
            for row in local_metrics(self.db_path)["metrics"]
        }

    def test_candidate_acceptance_retry_rollback_timing_and_anonymity(self):
        first = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": self.secret},
            0,
            idempotency_key="save",
        )
        save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": self.secret},
            0,
            idempotency_key="save",
        )
        second = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": self.secret + " changed"},
            0,
        )
        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE artifact_revisions SET created_at = '2020-01-01T00:00:00+00:00' WHERE id = ?",
                (second["revisionId"],),
            )
        review = create_review(
            self.db_path, self.project_id, "brief", second["revisionId"]
        )
        accept_review(self.db_path, self.project_id, review["id"], 0, "adopt")
        accept_review(self.db_path, self.project_id, review["id"], 0, "adopt")
        save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": self.secret + " copied"},
            1,
        )
        before_failure = self.summary()
        with self.assertRaises(WorkspaceError):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "invalid link"},
                1,
                linked_impact_ids=["missing"],
            )
        self.assertEqual(self.summary(), before_failure)
        self.assertEqual(
            (
                before_failure["brief.saved"],
                before_failure["brief.changedSave"],
                before_failure["brief.staleResave"],
            ),
            ((3, 0), (2, 0), (1, 0)),
        )
        self.assertEqual(before_failure["brief.adopted"][0], 1)
        self.assertGreater(before_failure["brief.adopted"][1], 60)
        status, result = response_payload(
            "/api/metrics", "GET", None, str(self.db_path)
        )
        self.assertEqual(status, 200)
        self.assertEqual(set(result), {"scope", "metrics"})
        self.assertEqual(len(result["metrics"]), len(METRICS))
        for value in (self.secret, self.project_id, first["revisionId"], "2020-01-01"):
            self.assertNotIn(value, json.dumps(result))
        with sqlite3.connect(self.db_path) as connection:
            columns = [
                row[1]
                for row in connection.execute(
                    "PRAGMA table_info(local_product_metrics)"
                )
            ]
            self.assertEqual(columns, ["metric", "count", "total_seconds"])
        with self.assertRaises(WorkspaceError):
            response_payload(
                "/api/metrics", "GET", None, str(self.db_path), actor="candidate"
            )

    def test_feedback_manual_and_auto_matches_and_idempotent_resolution(self):
        outline = save_artifact(
            self.db_path,
            self.project_id,
            "outline",
            {
                "nodes": [
                    {
                        "clientKey": "book",
                        "kind": "book",
                        "position": 0,
                        "title": self.secret,
                        "dependencyKeys": ["trust"],
                        "protection": "free",
                    }
                ]
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "outline", outline["revisionId"]
        )
        accept_review(self.db_path, self.project_id, review["id"], 0, "outline")
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": self.secret,
                "sceneGroups": [{"id": "scene", "body": self.secret}],
            },
            1,
        )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "scene",
                    "factKind": "world",
                    "predicate": key,
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": self.secret,
                }
                for key in ("trust", "manual")
            ],
        )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            1,
            "draft",
            {
                "sceneGroupIds": ["scene"],
                "factDecisions": {
                    fact["id"]: "confirmed" for fact in review["factProposals"]
                },
                "factDependencyLinks": [
                    {
                        "factProposalId": review["factProposals"][1]["id"],
                        "targetArtifactKind": "outline",
                        "targetRevisionId": outline["revisionId"],
                        "targetNodeId": outline["payload"]["nodes"][0]["id"],
                    }
                ],
            },
        )
        self.assertEqual(len(receipt["impacts"]), 2)
        for index, resolution in enumerate(("kept", "false-positive")):
            impact_id = receipt["impacts"][index]["id"]
            resolve_planning_impact(
                self.db_path,
                self.project_id,
                impact_id,
                2 + index,
                resolution,
                self.secret,
                resolution,
            )
            resolve_planning_impact(
                self.db_path,
                self.project_id,
                impact_id,
                2 + index,
                resolution,
                self.secret,
                resolution,
            )
        metrics = self.summary()
        self.assertEqual(
            [
                metrics[key][0]
                for key in (
                    "feedback.autoMatch",
                    "feedback.manualLink",
                    "feedback.kept",
                    "feedback.falsePositive",
                    "localOnly.adopted",
                )
            ],
            [1, 1, 1, 1, 1],
        )
        self.assertNotIn(self.secret, json.dumps(local_metrics(self.db_path)))

    def test_local_only_and_explicit_abandonment(self):
        materialize_standalone_chapter(
            self.db_path, self.project_id, self.secret, 0, "chapter"
        )
        materialize_standalone_chapter(
            self.db_path, self.project_id, self.secret, 0, "chapter"
        )
        action = {"stage": "plan", "idempotencyKey": str(uuid4())}
        response_payload("/api/metrics/abandon", "POST", action, str(self.db_path))
        response_payload("/api/metrics/abandon", "POST", action, str(self.db_path))
        with self.assertRaises(WorkspaceError):
            response_payload(
                "/api/metrics/abandon",
                "POST",
                {"stage": self.secret},
                str(self.db_path),
            )
        with self.assertRaises(WorkspaceError):
            response_payload(
                "/api/metrics/abandon",
                "POST",
                {"stage": "plan"},
                str(self.db_path),
                actor="candidate",
            )
        self.assertEqual(
            (self.summary()["localOnly.created"], self.summary()["plan.abandoned"]),
            ((1, 0), (1, 0)),
        )

    def test_partial_adoption_is_not_counted_as_author_save_or_rework(self):
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "A\n\n---\n\nB",
                "sceneGroups": [{"id": "a", "body": "A"}, {"id": "b", "body": "B"}],
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "draft", draft["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "partial",
            {"sceneGroupIds": ["a"], "noConfirmedChanges": True},
        )
        metrics = self.summary()
        self.assertEqual(
            [
                metrics[key][0]
                for key in (
                    "draft.saved",
                    "draft.changedSave",
                    "draft.staleResave",
                    "draft.adopted",
                )
            ],
            [1, 0, 0, 1],
        )

    def test_task_cancel_is_timed_once_and_never_inferred_as_abandonment(self):
        task = create_task(
            self.db_path, self.project_id, "generateBrief", 0, "private-thread-id"
        )
        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE task_runs SET created_at = '2020-01-01T00:00:00+00:00' WHERE id = ?",
                (task["id"],),
            )
        cancel_task(self.db_path, self.project_id, task["id"])
        cancel_task(self.db_path, self.project_id, task["id"])
        metrics = self.summary()
        self.assertEqual(metrics["generation.cancelled"][0], 1)
        self.assertGreater(metrics["generation.cancelled"][1], 60)
        self.assertEqual(metrics["brief.abandoned"], (0, 0))
        self.assertNotIn("private-thread-id", json.dumps(local_metrics(self.db_path)))
