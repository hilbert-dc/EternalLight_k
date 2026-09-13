import sqlite3
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.service import (
    accept_review,
    create_review,
    create_task,
    save_artifact,
    save_draft_checkpoint,
    undo_commit,
)
from runtime.storage import create_project


class FeedbackContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.db_path = Path(self.directory.name) / "workspace.sqlite3"
        state = create_project(self.db_path, "测试长篇", "一个秘密改变了远期计划")
        self.project_id = state["project"]["id"]

    def tearDown(self) -> None:
        self.directory.cleanup()

    def accept_outline(self, dependency_keys: list[str] | None = None) -> dict:
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
                        "title": "全书",
                        "purpose": "维持全书方向",
                        "summary": "当前创作基线",
                        "dependencyKeys": dependency_keys or [],
                        "certainty": "settled",
                        "protection": "free",
                    }
                ]
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "outline", outline["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "accept-feedback-contract-outline",
        )
        return outline

    def test_generated_outline_respects_author_node_protections(self) -> None:
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
                        "title": "全书",
                        "purpose": "守住救赎主题",
                        "summary": "",
                        "dependencyKeys": [],
                        "certainty": "settled",
                        "protection": "preserve-intent",
                    }
                ]
            },
            0,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "outline", outline["revisionId"]
            )["id"],
            0,
            "accept-protected-outline",
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateOutline",
            1,
            "protected-outline-agent",
        )
        changed = deepcopy(outline["payload"])
        changed["nodes"][0]["purpose"] = "改成复仇主题"
        with self.assertRaisesRegex(WorkspaceError, "受保护内容"):
            save_artifact(
                self.db_path,
                self.project_id,
                "outline",
                changed,
                1,
                "candidate",
                task["id"],
                "protected-outline-agent",
            )
        author_revision = save_artifact(
            self.db_path, self.project_id, "outline", changed, 1
        )
        self.assertEqual(
            author_revision["payload"]["nodes"][0]["purpose"], "改成复仇主题"
        )

    def test_empty_fact_review_requires_explicit_no_change_decision(self) -> None:
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "雨停了。",
                "sceneGroups": [{"id": "rain", "body": "雨停了。"}],
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "draft", draft["revisionId"]
        )
        with self.assertRaisesRegex(WorkspaceError, "必须明确确认"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                0,
                "missing-empty-fact-decision",
                {"sceneGroupIds": ["rain"]},
            )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "confirm-empty-fact-decision",
            {"sceneGroupIds": ["rain"], "noConfirmedChanges": True},
        )
        self.assertEqual(receipt["confirmedFacts"], [])

    def test_author_fact_link_creates_new_event_chain_and_undo_withdraws_it(
        self,
    ) -> None:
        outline = self.accept_outline()
        target_id = outline["payload"]["nodes"][0]["id"]

        def accept_linked_fact(revision: int, body: str, suffix: str) -> dict:
            draft = save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterNumber": 1,
                    "body": body,
                    "sceneGroups": [{"id": "manual", "body": body}],
                },
                revision,
            )
            review = create_review(
                self.db_path,
                self.project_id,
                "draft",
                draft["revisionId"],
                [
                    {
                        "sceneGroupId": "manual",
                        "factKind": "world",
                        "predicate": "unmatched-fact",
                        "value": suffix,
                        "epistemicState": "known",
                        "readingChapter": 1,
                        "sceneOrder": 1,
                        "evidence": body,
                    }
                ],
            )
            fact_id = review["factProposals"][0]["id"]
            return accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                revision,
                f"accept-manual-link-{suffix}",
                {
                    "sceneGroupIds": ["manual"],
                    "factDecisions": {fact_id: "confirmed"},
                    "factDependencyLinks": [
                        {
                            "factProposalId": fact_id,
                            "targetArtifactKind": "outline",
                            "targetRevisionId": outline["revisionId"],
                            "targetNodeId": target_id,
                        }
                    ],
                },
            )

        first = accept_linked_fact(1, "她决定离开。", "first")
        self.assertEqual(first["impacts"][0]["targetNodeId"], target_id)
        first_chain = first["impacts"][0]["impactChainId"]
        undo_commit(
            self.db_path,
            self.project_id,
            first["commitId"],
            2,
            "undo-manual-link-first",
        )
        with sqlite3.connect(self.db_path) as connection:
            connection.row_factory = sqlite3.Row
            self.assertEqual(
                connection.execute(
                    "SELECT COUNT(*) FROM author_fact_dependency_links"
                ).fetchone()[0],
                0,
            )
            self.assertEqual(
                connection.execute(
                    "SELECT state FROM planning_impacts WHERE impact_chain_id = ?",
                    (first_chain,),
                ).fetchone()[0],
                "withdrawn",
            )
        second = accept_linked_fact(3, "她再次决定离开。", "second")
        self.assertNotEqual(second["impacts"][0]["impactChainId"], first_chain)

    def test_false_positive_direction_is_audited_and_withdrawn_on_undo(self) -> None:
        outline = self.accept_outline(dependency_keys=["moon-is-red"])
        protected = deepcopy(outline["payload"])
        protected["nodes"][0]["protection"] = "preserve-intent"
        protected_revision = save_artifact(
            self.db_path, self.project_id, "outline", protected, 1
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path,
                self.project_id,
                "outline",
                protected_revision["revisionId"],
            )["id"],
            1,
            "accept-protected-red-moon",
        )
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "月亮泛红。",
                "sceneGroups": [{"id": "moon", "body": "月亮泛红。"}],
            },
            2,
        )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "moon",
                    "factKind": "world",
                    "predicate": "moon-is-red",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "月亮泛红。",
                }
            ],
        )
        preview = review["provisionalImpacts"][0]
        fact_id = review["factProposals"][0]["id"]
        with self.assertRaisesRegex(WorkspaceError, "必须填写作者说明"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                2,
                "false-positive-without-note",
                {
                    "factDecisions": {fact_id: "confirmed"},
                    "directionChangeDecisions": {preview["id"]: "false-positive"},
                },
            )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            2,
            "false-positive-with-note",
            {
                "factDecisions": {fact_id: "confirmed"},
                "directionChangeDecisions": {
                    preview["id"]: {
                        "decision": "false-positive",
                        "note": "红月本来就在规划允许范围内",
                    }
                },
            },
        )
        self.assertEqual(receipt["impacts"][0]["state"], "resolved-false-positive")
        undo_commit(
            self.db_path,
            self.project_id,
            receipt["commitId"],
            3,
            "undo-false-positive",
        )
        with sqlite3.connect(self.db_path) as connection:
            self.assertEqual(
                connection.execute(
                    "SELECT state FROM planning_impacts WHERE source_commit_id = ?",
                    (receipt["commitId"],),
                ).fetchone()[0],
                "withdrawn",
            )
