import tempfile
import unittest
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.service import (
    accept_review,
    create_review,
    save_artifact,
    save_draft_checkpoint,
)
from runtime.server import response_payload
from runtime.storage import connect, create_project


class AuthorExportTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.db = Path(self.directory.name) / "export.sqlite3"
        self.project = create_project(
            self.db, "PRIVATE_TITLE_CANARY", "PRIVATE_LOGLINE_CANARY"
        )["project"]["id"]

    def export(self, scopes, format_name="markdown", actor="author"):
        return response_payload(
            f"/api/projects/{self.project}/export",
            "POST",
            {"scopes": scopes, "format": format_name},
            str(self.db),
            actor,
        )[1]

    def adopt(self, kind, revision, expected):
        review = create_review(self.db, self.project, kind, revision)
        return accept_review(
            self.db,
            self.project,
            review["id"],
            expected,
            revision,
            selections={"noConfirmedChanges": True},
        )

    def test_export_uses_accepted_pointer_excludes_candidate_and_private_metadata(self):
        saved = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterNumber": 1,
                "body": "正式文字",
                "sceneGroups": [{"id": "one", "body": "正式文字"}],
            },
            0,
        )
        self.adopt("draft", saved["revisionId"], 0)
        candidate = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterId": saved["chapterId"],
                "body": "CANDIDATE_CANARY",
                "sceneGroups": [{"id": "one", "body": "CANDIDATE_CANARY"}],
            },
            1,
        )
        with connect(self.db) as connection:
            connection.execute(
                "UPDATE chapters SET title='PRIVATE_CHAPTER_CANARY' WHERE id=?",
                (saved["chapterId"],),
            )
        result = self.export(["manuscript"])
        self.assertIn("正式文字", result["content"])
        self.assertIn(saved["revisionId"], result["content"])
        self.assertNotIn("CANARY", str(result))
        self.assertEqual(result["projectRevision"], 1)
        selected = self.export(["candidates"], "txt")
        self.assertIn("CANDIDATE_CANARY", selected["content"])
        self.assertNotIn("正式文字", selected["content"])
        self.adopt("draft", candidate["revisionId"], 1)
        self.assertIn("CANDIDATE_CANARY", self.export(["manuscript"])["content"])
        with self.assertRaises(WorkspaceError):
            self.export(["candidates"])

    def test_public_planning_is_projection_and_full_planning_requires_private(self):
        brief = save_artifact(
            self.db, self.project, "brief", {"premise": "PRIVATE_BRIEF_CANARY"}, 0
        )
        self.adopt("brief", brief["revisionId"], 0)
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
                        "title": "PRIVATE_TITLE_CANARY",
                        "purpose": "PRIVATE_PURPOSE_CANARY",
                        "summary": "PRIVATE_SUMMARY_CANARY",
                        "publicTitle": "公开篇名",
                        "writerGuidance": "公开指引",
                        "dependencyKeys": [],
                        "certainty": "settled",
                        "protection": "free",
                    }
                ]
            },
            1,
        )
        self.adopt("outline", outline["revisionId"], 1)
        public = self.export(["planning"])
        self.assertIn("公开指引", public["content"])
        self.assertNotIn("CANARY", str(public))
        private = self.export(["private"])
        self.assertIn("PRIVATE_BRIEF_CANARY", private["content"])
        self.assertIn("PRIVATE_SUMMARY_CANARY", private["content"])

    def test_author_only_validation_empty_and_project_isolation(self):
        for scopes, format_name, actor in [
            (["manuscript"], "txt", "candidate"),
            ([], "txt", "author"),
            (["private", "private"], "txt", "author"),
            (["invalid"], "txt", "author"),
            (["manuscript"], "json", "author"),
            (["manuscript"], "txt", "author"),
        ]:
            with (
                self.subTest(scopes=scopes, actor=actor, format=format_name),
                self.assertRaises(WorkspaceError),
            ):
                self.export(scopes, format_name, actor)
        other = create_project(self.db, "其他作品")["project"]["id"]
        save_draft_checkpoint(
            self.db,
            other,
            {
                "chapterNumber": 1,
                "body": "OTHER_PROJECT_CANARY",
                "sceneGroups": [{"id": "other", "body": "OTHER_PROJECT_CANARY"}],
            },
            0,
        )
        with self.assertRaises(WorkspaceError):
            self.export(["candidates", "private"])

    def test_secret_facts_require_explicit_private_selection(self):
        draft = save_draft_checkpoint(
            self.db,
            self.project,
            {
                "chapterNumber": 1,
                "body": "他走进塔楼。",
                "sceneGroups": [{"id": "tower", "body": "他走进塔楼。"}],
            },
            0,
        )
        review = create_review(
            self.db,
            self.project,
            "draft",
            draft["revisionId"],
            [
                {
                    "factKind": "author-secret",
                    "predicate": "PRIVATE_FACT_CANARY",
                    "value": "PRIVATE_VALUE_CANARY",
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "PRIVATE_EVIDENCE_CANARY",
                }
            ],
        )
        decisions = {fact["id"]: "confirmed" for fact in review["factProposals"]}
        accept_review(
            self.db,
            self.project,
            review["id"],
            0,
            "secret",
            selections={"factDecisions": decisions},
        )
        self.assertNotIn("CANARY", str(self.export(["manuscript"])))
        private = self.export(["private"])
        for text in (
            "PRIVATE_FACT_CANARY",
            "PRIVATE_VALUE_CANARY",
            "PRIVATE_EVIDENCE_CANARY",
        ):
            self.assertIn(text, private["content"])
