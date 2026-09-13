import tempfile
import unittest
from pathlib import Path

from runtime.storage import (
    DEFAULT_PROJECT_ID,
    chapter_diffs,
    connect,
    create_project,
    list_projects,
    project_state,
    run_story_tests,
    search_project,
)


class StorageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.db_path = Path(self.directory.name) / "test.sqlite3"

    def tearDown(self) -> None:
        self.directory.cleanup()

    def test_seed_contains_canon_ir_and_novel_edition(self) -> None:
        state = project_state(self.db_path)
        self.assertEqual(state["project"]["id"], DEFAULT_PROJECT_ID)
        self.assertEqual(len(state["canon"]["characters"]), 3)
        self.assertEqual(len(state["canon"]["relationships"]), 1)
        self.assertEqual(len(state["canon"]["events"]), 1)
        self.assertEqual(len(state["canon"]["locations"]), 2)
        self.assertEqual(len(state["narrativeIr"]["scenes"][0]["beats"]), 4)
        self.assertEqual(
            state["novelEdition"]["chapters"][0]["passages"][0]["scene_id"], "scene-184"
        )

    def test_search_finds_domain_entities(self) -> None:
        results = search_project(self.db_path, DEFAULT_PROJECT_ID, "灯塔")
        self.assertEqual({item["kind"] for item in results}, {"character", "scene"})
        self.assertTrue(
            all(item["snippet"] and item["source"] == item["kind"] for item in results)
        )

    def test_search_retrieves_long_form_and_can_filter_kind(self) -> None:
        results = search_project(self.db_path, DEFAULT_PROJECT_ID, "四十七秒", limit=10)
        self.assertTrue(
            {item["kind"] for item in results}.issuperset(
                {"worldRule", "scene", "chapter"}
            )
        )
        rules = search_project(
            self.db_path, DEFAULT_PROJECT_ID, "四十七秒", limit=10, kind="worldRule"
        )
        self.assertEqual([item["kind"] for item in rules], ["worldRule"])
        self.assertLessEqual(
            len(search_project(self.db_path, DEFAULT_PROJECT_ID, "白", limit=2)), 2
        )

    def test_create_project_is_listed_and_keeps_content_isolated(self) -> None:
        created = create_project(
            self.db_path, "雾中的回声", "一名记录员追查一座消失的城市。"
        )
        self.assertNotEqual(created["project"]["id"], DEFAULT_PROJECT_ID)
        self.assertEqual(created["project"]["title"], "雾中的回声")
        self.assertEqual(
            created["project"]["logline"], "一名记录员追查一座消失的城市。"
        )
        self.assertEqual(created["canon"]["characters"], [])
        self.assertEqual(created["novelEdition"]["chapters"], [])
        self.assertEqual(
            {project["id"] for project in list_projects(self.db_path)},
            {DEFAULT_PROJECT_ID, created["project"]["id"]},
        )
        self.assertEqual(
            len(
                project_state(self.db_path, DEFAULT_PROJECT_ID)["novelEdition"][
                    "chapters"
                ]
            ),
            1,
        )

    def test_chapter_diffs_require_project_ownership(self) -> None:
        project_state(self.db_path)
        other = create_project(self.db_path, "另一部作品")
        self.assertIsInstance(
            chapter_diffs(self.db_path, DEFAULT_PROJECT_ID, "chapter-01"), list
        )
        with self.assertRaises(KeyError):
            chapter_diffs(self.db_path, other["project"]["id"], "chapter-01")

    def test_new_project_has_isolated_index_and_actionable_story_test(self) -> None:
        created = create_project(self.db_path, "雾中的回声")
        results = search_project(self.db_path, created["project"]["id"], "最后一盏灯")
        self.assertEqual(results, [])
        tests = run_story_tests(self.db_path, created["project"]["id"])
        self.assertTrue(tests["passed"])
        foundation = next(
            check for check in tests["checks"] if check["id"] == "canon-has-foundation"
        )
        self.assertFalse(foundation["passed"])
        self.assertEqual(foundation["severity"], "warning")
        self.assertIn("至少一个角色", foundation["details"])

    def test_story_tests_detect_broken_beat_order(self) -> None:
        project_state(self.db_path)
        with connect(self.db_path) as connection:
            connection.execute("UPDATE beats SET position = 3 WHERE id = 'beat-184-2'")
        tests = run_story_tests(self.db_path, DEFAULT_PROJECT_ID)
        self.assertFalse(tests["passed"])
        order_check = next(
            check for check in tests["checks"] if check["id"] == "beats-are-contiguous"
        )
        self.assertFalse(order_check["passed"])

    def test_create_project_rejects_invalid_metadata_without_writing(self) -> None:
        before = list_projects(self.db_path)
        with self.assertRaises(ValueError):
            create_project(self.db_path, "   ")
        with self.assertRaises(ValueError):
            create_project(self.db_path, "x" * 121)
        with self.assertRaises(ValueError):
            create_project(self.db_path, "有效作品", "x" * 501)
        self.assertEqual(list_projects(self.db_path), before)


if __name__ == "__main__":
    unittest.main()
