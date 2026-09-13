import tempfile
import unittest
from pathlib import Path

from runtime.creative_workspace.errors import ConflictError, WorkspaceError
from runtime.creative_workspace.generation import create_task, update_task
from runtime.creative_workspace.service import (
    accept_review,
    create_review,
    get_context_snapshot,
    save_artifact,
    save_draft_checkpoint,
    workspace_state,
)
from runtime.creative_workspace.working_baselines import (
    clear_working_baseline,
    save_continuation_intent,
    set_working_baseline,
)
from runtime.creative_workspace.planning_bindings import materialize_chapter_from_plan
from runtime.storage import connect, create_project


class WorkingBaselineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.db_path = Path(self.directory.name) / "workspace.sqlite3"
        state = create_project(self.db_path, "工作版本", "测试冻结的创作依据")
        self.project_id = state["project"]["id"]

    def tearDown(self) -> None:
        self.directory.cleanup()

    def _chapter(self, number: int, body: str) -> dict:
        return save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": number,
                "title": f"第 {number} 章",
                "body": body,
                "sceneGroups": [{"id": f"scene-{number}", "body": body}],
            },
            0,
        )

    def test_selection_uses_independent_monotonic_cas(self) -> None:
        chapter = self._chapter(1, "作者工作稿")
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            0,
        )
        state = workspace_state(self.db_path, self.project_id)
        selection = state["chapters"][0]["workingSelection"]
        self.assertEqual(selection["revisionId"], chapter["revisionId"])

        cleared = clear_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            selection["selectionRevision"],
        )
        self.assertEqual(cleared["selectionRevision"], 2)
        with self.assertRaises(ConflictError):
            set_working_baseline(
                self.db_path,
                self.project_id,
                "draft",
                chapter["chapterId"],
                chapter["revisionId"],
                1,
            )
        restored = set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            2,
        )
        self.assertEqual(restored["selectionRevision"], 3)

    def test_writer_uses_selected_working_draft_not_new_agent_candidate(self) -> None:
        first = self._chapter(1, "作者选中的前章 B")
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            first["chapterId"],
            first["revisionId"],
            0,
        )
        rewrite_task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "rewrite-first",
            target={
                "type": "chapter",
                "chapterId": first["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        newer = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": first["chapterId"],
                "body": "未选中的 Agent 新稿 C",
                "sceneGroups": [{"id": "scene-1", "body": "未选中的 Agent 新稿 C"}],
            },
            0,
            "candidate",
            rewrite_task["id"],
            "rewrite-first",
        )
        second = self._chapter(2, "第二章目标")
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "write-second",
            target={
                "type": "chapter",
                "chapterId": second["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
            author_intent="延续第一章的压迫感",
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=2,
            scene_order=2_147_483_647,
            byte_budget=60_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="write-second",
        )
        reference = next(
            item
            for item in context["sourceExcerpts"]
            if item["relationship"] == "referenceDraft"
        )
        self.assertEqual(reference["draftRevisionId"], first["revisionId"])
        self.assertEqual(reference["body"], "作者选中的前章 B")
        self.assertNotEqual(reference["draftRevisionId"], newer["revisionId"])
        self.assertEqual(reference["evidenceClass"], "required")

    def test_task_replays_same_input_after_output_checkpoint(self) -> None:
        chapter = self._chapter(1, "不可变输入")
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "snapshot-replay",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
            author_intent="保持原叙述距离",
        )
        first = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=1,
            scene_order=2_147_483_647,
            byte_budget=60_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="snapshot-replay",
        )
        save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": chapter["chapterId"],
                "body": "新的输出检查点",
                "sceneGroups": [{"id": "scene-1", "body": "新的输出检查点"}],
            },
            0,
            "candidate",
            task["id"],
            "snapshot-replay",
        )
        replay = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=1,
            scene_order=2_147_483_647,
            byte_budget=4_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="snapshot-replay",
        )
        self.assertEqual(
            first["inputSnapshot"]["contentHash"],
            replay["inputSnapshot"]["contentHash"],
        )
        self.assertFalse(first["inputSnapshot"]["replayed"])
        self.assertTrue(replay["inputSnapshot"]["replayed"])
        self.assertIn("不可变输入", str(replay["sourceExcerpts"]))
        self.assertNotIn("新的输出检查点", str(replay["sourceExcerpts"]))

    def test_plan_candidate_preserves_working_outline_source(self) -> None:
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
                        "title": "工作大纲",
                        "purpose": "作为候选章纲来源",
                        "summary": "尚未正式采用",
                        "dependencyKeys": [],
                        "certainty": "tentative",
                        "protection": "free",
                    }
                ]
            },
            0,
        )
        set_working_baseline(
            self.db_path,
            self.project_id,
            "outline",
            "outline",
            outline["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generatePlan",
            0,
            "working-outline-plan",
            author_intent="规划第一章",
        )
        node_id = outline["payload"]["nodes"][0]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": "model-cannot-choose-source",
                "chapters": [
                    {
                        "clientKey": "chapter-one",
                        "primaryNodeId": node_id,
                        "chapterNumber": 1,
                        "goal": "建立压力",
                        "dependencyKeys": [],
                    }
                ],
            },
            0,
            "candidate",
            task["id"],
            "working-outline-plan",
        )
        self.assertEqual(plan["payload"]["sourceOutlineRevisionId"], outline["revisionId"])
        review = create_review(
            self.db_path, self.project_id, "plan", plan["revisionId"]
        )
        with self.assertRaisesRegex(ConflictError, "尚未正式采用"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                0,
                "cannot-accept-working-plan",
            )

    def test_required_target_body_cannot_be_silently_truncated(self) -> None:
        chapter = self._chapter(1, "界" * 30_000)
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "required-overflow",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
            author_intent="在完整原文上继续",
        )
        with self.assertRaisesRegex(WorkspaceError, "预算不足"):
            get_context_snapshot(
                self.db_path,
                self.project_id,
                "writer",
                0,
                reading_chapter=1,
                scene_order=2_147_483_647,
                byte_budget=64_000,
                actor="candidate",
                task_id=task["id"],
                thread_id="required-overflow",
            )
        with connect(self.db_path) as connection:
            row = connection.execute(
                "SELECT input_snapshot_id FROM task_runs WHERE id = ?", (task["id"],)
            ).fetchone()
            self.assertIsNone(row["input_snapshot_id"])

    def test_continuation_intent_survives_launch_failure(self) -> None:
        chapter = self._chapter(1, "待续正文")
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            0,
        )
        saved_intent = save_continuation_intent(
            self.db_path,
            self.project_id,
            "draft",
            chapter["chapterId"],
            chapter["revisionId"],
            "让人物在沉默中暴露立场",
        )
        self.assertEqual(saved_intent["status"], "ready")
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "failed-launch",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
            author_intent="让人物在沉默中暴露立场",
        )
        update_task(
            self.db_path,
            self.project_id,
            task["id"],
            "failed-launch",
            "failed",
            error="模型未配置",
        )
        restored = workspace_state(self.db_path, self.project_id)[
            "continuationIntents"
        ][0]
        self.assertEqual(restored["workingRevisionId"], chapter["revisionId"])
        self.assertEqual(restored["authorIntent"], "让人物在沉默中暴露立场")
        self.assertEqual(restored["status"], "failed")
        self.assertEqual(restored["taskId"], task["id"])

    def test_writer_uses_selected_working_plan_for_stable_chapter_binding(self) -> None:
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
                        "title": "正式大纲",
                        "purpose": "建立章节绑定",
                        "summary": "正式方向",
                        "dependencyKeys": [],
                        "certainty": "settled",
                        "protection": "free",
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
            "accept-bound-outline",
        )
        node_id = outline["payload"]["nodes"][0]["id"]
        formal_plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "bound-chapter",
                        "primaryNodeId": node_id,
                        "chapterNumber": 1,
                        "goal": "正式安排",
                        "dependencyKeys": [],
                        "writerGuidance": "按正式安排写",
                    }
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "plan", formal_plan["revisionId"]
            )["id"],
            1,
            "accept-bound-plan",
        )
        chapter_plan_id = formal_plan["payload"]["chapters"][0]["chapterPlanId"]
        chapter = materialize_chapter_from_plan(
            self.db_path,
            self.project_id,
            chapter_plan_id,
            2,
            "materialize-bound-chapter",
        )
        working_outline_payload = {
            "nodes": [
                {
                    **outline["payload"]["nodes"][0],
                    "summary": "工作方向",
                }
            ]
        }
        working_outline = save_artifact(
            self.db_path,
            self.project_id,
            "outline",
            working_outline_payload,
            3,
        )
        set_working_baseline(
            self.db_path,
            self.project_id,
            "outline",
            "outline",
            working_outline["revisionId"],
            0,
        )
        working_plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": working_outline["revisionId"],
                "chapters": [
                    {
                        "chapterPlanId": chapter_plan_id,
                        "primaryNodeId": node_id,
                        "chapterNumber": 1,
                        "goal": "工作安排",
                        "dependencyKeys": [],
                        "writerGuidance": "按工作安排写",
                    }
                ],
            },
            3,
        )
        set_working_baseline(
            self.db_path,
            self.project_id,
            "plan",
            "plan",
            working_plan["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            3,
            "working-plan-writer",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
            author_intent="按新的工作安排写",
        )
        self.assertEqual(
            task["inputManifest"]["planningBaseline"]["planRevisionId"],
            working_plan["revisionId"],
        )
        self.assertEqual(
            task["inputManifest"]["planningBaseline"]["lineageStatus"], "working"
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            3,
            reading_chapter=1,
            scene_order=2_147_483_647,
            actor="candidate",
            task_id=task["id"],
            thread_id="working-plan-writer",
        )
        self.assertEqual(context["planningContext"]["lineageStatus"], "working")
        self.assertEqual(
            context["planningContext"]["chapter"]["writerGuidance"],
            "按工作安排写",
        )

    def test_author_can_mark_a_stale_task_failed_without_reopening_writes(self) -> None:
        task = create_task(
            self.db_path,
            self.project_id,
            "advisory",
            0,
            "stale-advisory",
            author_intent="讨论旧版本",
        )
        brief = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "推进正式版本"},
            0,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "brief", brief["revisionId"]
            )["id"],
            0,
            "advance-before-failure",
        )
        result = update_task(
            self.db_path,
            self.project_id,
            task["id"],
            "stale-advisory",
            "failed",
            error="作品版本已推进",
        )
        self.assertEqual(result["status"], "failed")
        with self.assertRaises(ConflictError):
            update_task(
                self.db_path,
                self.project_id,
                task["id"],
                "stale-advisory",
                "running",
            )


if __name__ == "__main__":
    unittest.main()
