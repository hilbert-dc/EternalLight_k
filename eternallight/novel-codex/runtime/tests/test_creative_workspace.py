import tempfile
import unittest
import sqlite3
import time
from copy import deepcopy
from pathlib import Path

from runtime.creative_workspace.errors import WorkspaceError
from runtime.creative_workspace.service import (
    accept_review,
    cancel_task,
    complete_task,
    create_review,
    create_task,
    get_context_snapshot,
    recover_orphaned_tasks,
    save_artifact,
    save_draft_checkpoint,
    set_artifact_lock,
    undo_commit,
    workspace_state,
)
from runtime.creative_workspace.planning_bindings import (
    associate_standalone_chapter_to_plan,
    materialize_chapter_from_plan,
    materialize_standalone_chapter,
)
from runtime.creative_workspace.impacts import (
    planning_impact_page,
    resolve_planning_impact,
)
from runtime.creative_workspace.working_baselines import set_working_baseline
from runtime.storage import create_project, initialize, recover_search_indexes


class CreativeWorkspaceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.db_path = Path(self.directory.name) / "workspace.sqlite3"
        state = create_project(self.db_path, "测试长篇", "一个秘密改变了远期计划")
        self.project_id = state["project"]["id"]

    def tearDown(self) -> None:
        self.directory.cleanup()

    def accept_outline(
        self, expected_revision: int = 0, dependency_keys: list[str] | None = None
    ) -> dict:
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
            expected_revision,
        )
        review = create_review(
            self.db_path, self.project_id, "outline", outline["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            expected_revision,
            f"accept-outline-{expected_revision}",
        )
        return outline

    def test_artifact_candidate_requires_review_before_becoming_formal(self) -> None:
        task = create_task(
            self.db_path,
            self.project_id,
            "generateOutline",
            0,
            "thread-outline",
            "test-model",
        )
        candidate = save_artifact(
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
                        "purpose": "建立困境并走向真相反转",
                        "summary": "支付代价后留下余波",
                        "dependencyKeys": [],
                        "certainty": "tentative",
                        "protection": "free",
                    }
                ]
            },
            0,
            "candidate",
            task["id"],
            "thread-outline",
        )
        before = workspace_state(self.db_path, self.project_id)
        self.assertIsNone(before["artifacts"]["outline"]["accepted"])
        self.assertEqual(
            before["artifacts"]["outline"]["candidate"]["id"], candidate["revisionId"]
        )

        review = create_review(
            self.db_path, self.project_id, "outline", candidate["revisionId"]
        )
        receipt = accept_review(
            self.db_path, self.project_id, review["id"], 0, "accept-outline"
        )
        replayed = accept_review(
            self.db_path, self.project_id, review["id"], 0, "accept-outline"
        )
        after = workspace_state(self.db_path, self.project_id)

        self.assertEqual(receipt, replayed)
        self.assertEqual(receipt["resultRevision"], 1)
        self.assertEqual(
            after["artifacts"]["outline"]["accepted"]["id"], candidate["revisionId"]
        )

    def test_accept_replay_returns_receipt_before_rechecking_story_tests(self) -> None:
        candidate = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "一份不可重复提交的承诺"},
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "brief", candidate["revisionId"]
        )
        receipt = accept_review(
            self.db_path, self.project_id, review["id"], 0, "stable-replay"
        )
        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "INSERT INTO scenes(id, project_id, title, location, time, summary, status) VALUES (?, ?, ?, '', '', '', 'draft')",
                ("broken-scene", self.project_id, "故意破坏 Story Tests"),
            )
        replayed = accept_review(
            self.db_path, self.project_id, review["id"], 0, "stable-replay"
        )
        self.assertEqual(replayed, receipt)
        self.assertNotIn("indexStatus", receipt)
        self.assertEqual(receipt["indexTargetRevision"], receipt["resultRevision"])

    def test_locked_formal_artifact_rejects_candidate_until_author_unlocks(
        self,
    ) -> None:
        candidate = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "锁定方向"}, 0
        )
        review = create_review(
            self.db_path, self.project_id, "brief", candidate["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, review["id"], 0, "accept-before-lock"
        )
        locked = set_artifact_lock(self.db_path, self.project_id, "brief", True, 1)
        self.assertEqual(
            locked, {"artifactKind": "brief", "locked": True, "resultRevision": 2}
        )
        task = create_task(
            self.db_path, self.project_id, "generateBrief", 2, "thread-lock"
        )
        with self.assertRaisesRegex(WorkspaceError, "产物已锁定"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "偷偷覆盖"},
                2,
                "candidate",
                task["id"],
                "thread-lock",
            )
        set_artifact_lock(self.db_path, self.project_id, "brief", False, 2)
        next_task = create_task(
            self.db_path, self.project_id, "generateBrief", 3, "thread-unlock"
        )
        saved = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "明确解锁后修订"},
            3,
            "candidate",
            next_task["id"],
            "thread-unlock",
        )
        self.assertEqual(saved["artifactKind"], "brief")

    def test_lock_after_accept_prevents_undo_and_preserves_state(self) -> None:
        candidate = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "正式命题"}, 0
        )
        review = create_review(
            self.db_path, self.project_id, "brief", candidate["revisionId"]
        )
        receipt = accept_review(
            self.db_path, self.project_id, review["id"], 0, "accept-before-undo-lock"
        )
        set_artifact_lock(self.db_path, self.project_id, "brief", True, 1)

        with self.assertRaisesRegex(WorkspaceError, "之后已有其他正式变更"):
            undo_commit(
                self.db_path, self.project_id, receipt["commitId"], 2, "undo-after-lock"
            )

        current = workspace_state(self.db_path, self.project_id)
        self.assertEqual(current["revision"], 2)
        self.assertTrue(current["artifacts"]["brief"]["locked"])
        self.assertEqual(
            current["artifacts"]["brief"]["accepted"]["id"], candidate["revisionId"]
        )

    def test_stale_index_survives_lock_and_is_rebuilt_on_startup_recovery(self) -> None:
        candidate = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "索引恢复"}, 0
        )
        review = create_review(
            self.db_path, self.project_id, "brief", candidate["revisionId"]
        )
        accept_review(self.db_path, self.project_id, review["id"], 0, "accept-index")
        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE project_workspace SET indexed_revision = 0, index_target_revision = 1 WHERE project_id = ?",
                (self.project_id,),
            )

        set_artifact_lock(self.db_path, self.project_id, "brief", True, 1)
        with sqlite3.connect(self.db_path) as connection:
            indexed_revision, target_revision = connection.execute(
                "SELECT indexed_revision, index_target_revision FROM project_workspace WHERE project_id = ?",
                (self.project_id,),
            ).fetchone()
        self.assertEqual((indexed_revision, target_revision), (0, 2))

        recover_search_indexes(self.db_path)
        current = workspace_state(self.db_path, self.project_id)
        self.assertEqual(
            current["index"],
            {"targetRevision": 2, "indexedRevision": 2, "status": "ready"},
        )

    def test_nested_plan_locks_are_authoritative_and_require_unlock_only_revision(
        self,
    ) -> None:
        outline = self.accept_outline()
        root_id = outline["payload"]["nodes"][0]["id"]
        initial = {
            "sourceOutlineRevisionId": outline["revisionId"],
            "chapters": [
                {
                    "clientKey": "chapter-plan-open",
                    "primaryNodeId": root_id,
                    "chapterNumber": 1,
                    "goal": "允许调整的章节目标",
                    "dependencyKeys": [],
                    "publicCommitments": [],
                    "allowances": [],
                    "scenes": [
                        {
                            "id": "scene-open",
                            "goal": "允许调整的场景",
                            "beats": [
                                {
                                    "id": "beat-locked",
                                    "description": "不可改的 Beat",
                                    "locked": True,
                                },
                                {"id": "beat-open", "description": "可调整 Beat"},
                            ],
                        },
                        {
                            "id": "scene-locked",
                            "goal": "不可改的场景",
                            "beats": [],
                            "locked": True,
                        },
                    ],
                },
                {
                    "clientKey": "chapter-plan-locked",
                    "primaryNodeId": root_id,
                    "chapterNumber": 2,
                    "goal": "不可改的章节",
                    "dependencyKeys": [],
                    "publicCommitments": [],
                    "allowances": [],
                    "scenes": [],
                    "locked": True,
                },
            ],
        }
        first = save_artifact(self.db_path, self.project_id, "plan", initial, 1)
        initial = first["payload"]
        review = create_review(
            self.db_path, self.project_id, "plan", first["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, review["id"], 1, "accept-locked-plan"
        )
        task = create_task(
            self.db_path, self.project_id, "generatePlan", 2, "thread-plan-lock"
        )

        for mutate in (
            lambda payload: payload["chapters"][0]["scenes"][0]["beats"][0].update(
                description="被 Agent 改写"
            ),
            lambda payload: payload["chapters"][0]["scenes"][1].update(
                goal="被 Agent 改写"
            ),
            lambda payload: payload["chapters"][1].update(goal="被 Agent 改写"),
            lambda payload: payload["chapters"][0]["scenes"][0]["beats"].reverse(),
            lambda payload: payload["chapters"][0].update(scenes=[]),
        ):
            changed = deepcopy(initial)
            mutate(changed)
            with self.assertRaisesRegex(WorkspaceError, "不能被修改|不能被移动"):
                save_artifact(
                    self.db_path,
                    self.project_id,
                    "plan",
                    changed,
                    2,
                    "candidate",
                    task["id"],
                    "thread-plan-lock",
                )

        allowed = deepcopy(initial)
        allowed["chapters"][0]["goal"] = "已调整的开放章节目标"
        allowed["chapters"][0]["scenes"][0]["beats"][1]["description"] = (
            "已调整的开放 Beat"
        )
        save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            allowed,
            2,
            "candidate",
            task["id"],
            "thread-plan-lock",
        )

        unlock_only = deepcopy(initial)
        unlock_only["chapters"][0]["scenes"][0]["beats"][0]["locked"] = False
        unlocked = save_artifact(self.db_path, self.project_id, "plan", unlock_only, 2)
        unlock_review = create_review(
            self.db_path, self.project_id, "plan", unlocked["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, unlock_review["id"], 2, "accept-beat-unlock"
        )
        changed_after_unlock = deepcopy(unlock_only)
        changed_after_unlock["chapters"][0]["scenes"][0]["beats"][0]["description"] = (
            "作者解锁后改写"
        )
        self.assertEqual(
            save_artifact(
                self.db_path, self.project_id, "plan", changed_after_unlock, 3
            )["artifactKind"],
            "plan",
        )

    def test_draft_scene_lock_requires_an_unlock_only_revision(self) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "被保护的场景。",
                "sceneGroups": [
                    {"id": "protected-scene", "body": "被保护的场景。", "locked": True}
                ],
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "draft", initial["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "accept-protected-scene",
            {
                "sceneGroupIds": ["protected-scene"],
                "noConfirmedChanges": True,
            },
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            1,
            "thread-draft-lock",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        with self.assertRaisesRegex(WorkspaceError, "不能被修改"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterId": initial["chapterId"],
                    "body": "Agent 试图改写。",
                    "sceneGroups": [
                        {
                            "id": "protected-scene",
                            "body": "Agent 试图改写。",
                            "locked": True,
                        }
                    ],
                },
                1,
                "candidate",
                task["id"],
                "thread-draft-lock",
            )

        unlocked = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "被保护的场景。",
                "sceneGroups": [
                    {"id": "protected-scene", "body": "被保护的场景。", "locked": False}
                ],
            },
            1,
        )
        unlock_review = create_review(
            self.db_path, self.project_id, "draft", unlocked["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            unlock_review["id"],
            1,
            "accept-scene-unlock",
            {
                "sceneGroupIds": ["protected-scene"],
                "noConfirmedChanges": True,
            },
        )
        changed = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "解锁后由作者改写。",
                "sceneGroups": [
                    {
                        "id": "protected-scene",
                        "body": "解锁后由作者改写。",
                        "locked": False,
                    }
                ],
            },
            2,
        )
        self.assertEqual(changed["body"], "解锁后由作者改写。")

    def test_plan_scene_and_beat_contract_uses_stable_objects(self) -> None:
        outline = self.accept_outline()
        root_id = outline["payload"]["nodes"][0]["id"]
        payload = {
            "sourceOutlineRevisionId": outline["revisionId"],
            "chapters": [
                {
                    "clientKey": "chapter-plan-1",
                    "primaryNodeId": root_id,
                    "chapterNumber": 1,
                    "title": "试探",
                    "goal": "主角发现入口",
                    "conflict": "不能暴露身份",
                    "dependencyKeys": [],
                    "publicCommitments": [],
                    "allowances": [],
                    "locked": True,
                    "scenes": [
                        {
                            "id": "scene-plan-1",
                            "title": "雨夜入口",
                            "goal": "确认入口存在",
                            "beats": [
                                {
                                    "id": "beat-plan-1",
                                    "description": "主角看见门缝里的光",
                                }
                            ],
                            "locked": True,
                        }
                    ],
                }
            ],
        }
        saved = save_artifact(self.db_path, self.project_id, "plan", payload, 1)
        self.assertEqual(
            saved["payload"]["chapters"][0]["scenes"], payload["chapters"][0]["scenes"]
        )
        with self.assertRaisesRegex(
            WorkspaceError, "Beat 必须是 id/description/locked 对象"
        ):
            save_artifact(
                self.db_path,
                self.project_id,
                "plan",
                {
                    "sourceOutlineRevisionId": outline["revisionId"],
                    "chapters": [
                        {
                            "clientKey": "chapter-plan-invalid",
                            "primaryNodeId": root_id,
                            "chapterNumber": 1,
                            "goal": "错误 Beat",
                            "dependencyKeys": [],
                            "publicCommitments": [],
                            "allowances": [],
                            "scenes": [
                                {
                                    "id": "scene-plan-2",
                                    "goal": "检查",
                                    "beats": ["字符串 Beat"],
                                }
                            ],
                        }
                    ],
                },
                1,
            )
        duplicate_ids = deepcopy(saved["payload"])
        duplicate_ids["chapters"][0]["scenes"].append(
            {"id": "scene-plan-1", "goal": "重复场景", "beats": []}
        )
        with self.assertRaisesRegex(WorkspaceError, "唯一稳定 ID"):
            save_artifact(self.db_path, self.project_id, "plan", duplicate_ids, 1)

    def test_draft_acceptance_commits_facts_and_only_relevant_plan_impacts(
        self,
    ) -> None:
        outline = self.accept_outline()
        root_id = outline["payload"]["nodes"][0]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "chapter-plan-2",
                        "primaryNodeId": root_id,
                        "chapterNumber": 2,
                        "title": "处理钥匙",
                        "goal": "处理钥匙",
                        "dependencyKeys": ["has-old-key"],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                    {
                        "clientKey": "chapter-plan-3",
                        "primaryNodeId": root_id,
                        "chapterNumber": 3,
                        "title": "不相关支线",
                        "goal": "不相关支线",
                        "dependencyKeys": ["different-fact"],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                    {
                        "clientKey": "chapter-plan-4",
                        "primaryNodeId": root_id,
                        "chapterNumber": 4,
                        "title": "锁定的伏笔",
                        "goal": "锁定的伏笔",
                        "dependencyKeys": ["has-old-key"],
                        "publicCommitments": [],
                        "allowances": [],
                        "locked": True,
                    },
                ],
            },
            1,
        )
        plan_review = create_review(
            self.db_path, self.project_id, "plan", plan["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, plan_review["id"], 1, "accept-plan"
        )

        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "title": "钥匙",
                "body": "林夕从潮水里捞出一把旧钥匙。",
                "sceneGroups": [
                    {"id": "key-scene", "body": "林夕从潮水里捞出一把旧钥匙。"}
                ],
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
                    "factKind": "world",
                    "subjectId": "linxi",
                    "predicate": "has-old-key",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "林夕从潮水里捞出一把旧钥匙。",
                },
                {
                    "factKind": "author-secret",
                    "predicate": "key-opens-white-tower",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "作者设定：钥匙属于白塔。",
                },
            ],
        )
        decisions = {
            proposal["id"]: "confirmed" for proposal in review["factProposals"]
        }
        self.assertEqual(
            [impact["targetLabel"] for impact in review["provisionalImpacts"]],
            ["处理钥匙", "锁定的伏笔"],
        )
        base_selections = {
            "sceneGroupIds": ["key-scene"],
            "factDecisions": decisions,
        }
        with self.assertRaisesRegex(WorkspaceError, "必须由作者明确决定"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                2,
                "accept-draft-without-direction-decision",
                base_selections,
            )
        locked_preview = next(
            impact
            for impact in review["provisionalImpacts"]
            if impact["severity"] == "locked-conflict"
        )
        with self.assertRaisesRegex(WorkspaceError, "决定退回草稿"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                2,
                "return-draft-for-direction-conflict",
                {
                    **base_selections,
                    "directionChangeDecisions": {locked_preview["id"]: "return-draft"},
                },
            )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            2,
            "accept-draft",
            {
                **base_selections,
                "directionChangeDecisions": {
                    impact["id"]: "accept-and-open-impact"
                    for impact in review["provisionalImpacts"]
                    if impact["severity"] == "locked-conflict"
                },
            },
        )

        self.assertEqual(
            [impact["targetLabel"] for impact in receipt["impacts"]],
            ["处理钥匙", "锁定的伏笔"],
        )
        self.assertEqual(
            [impact["status"] for impact in receipt["impacts"]],
            ["affected", "locked-conflict"],
        )
        writer = get_context_snapshot(
            self.db_path, self.project_id, "writer", 3, reading_chapter=1, scene_order=1
        )
        critic = get_context_snapshot(
            self.db_path, self.project_id, "critic", 3, reading_chapter=1, scene_order=1
        )
        self.assertEqual(
            [fact["predicate"] for fact in writer["facts"]], ["has-old-key"]
        )
        self.assertEqual(
            {fact["predicate"] for fact in critic["facts"]},
            {"has-old-key", "key-opens-white-tower"},
        )
        self.assertEqual(
            writer["forbidden"][0], {"policy": "不要让当前视角获得尚未公开的信息"}
        )

        undone = undo_commit(
            self.db_path, self.project_id, receipt["commitId"], 3, "undo-draft"
        )
        self.assertEqual(undone["resultRevision"], 4)
        current = get_context_snapshot(
            self.db_path, self.project_id, "critic", 4, reading_chapter=1, scene_order=1
        )
        self.assertEqual(current["facts"], [])

    def test_replanning_requires_explicit_impact_resolution(self) -> None:
        outline = self.accept_outline()
        root_id = outline["payload"]["nodes"][0]["id"]
        initial_plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "chapter-plan-impact",
                        "primaryNodeId": root_id,
                        "chapterNumber": 2,
                        "title": "钥匙",
                        "goal": "等待钥匙",
                        "dependencyKeys": ["has-key"],
                        "publicCommitments": [],
                        "allowances": [],
                    }
                ],
            },
            1,
        )
        plan_review = create_review(
            self.db_path, self.project_id, "plan", initial_plan["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, plan_review["id"], 1, "accept-impact-plan"
        )
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "她拿到了钥匙。",
                "sceneGroups": [{"id": "key-found", "body": "她拿到了钥匙。"}],
            },
            2,
        )
        draft_review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "key-found",
                    "factKind": "world",
                    "predicate": "has-key",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "她拿到了钥匙。",
                }
            ],
        )
        fact_id = draft_review["factProposals"][0]["id"]
        accept_review(
            self.db_path,
            self.project_id,
            draft_review["id"],
            2,
            "accept-impact-fact",
            {"sceneGroupIds": ["key-found"], "factDecisions": {fact_id: "confirmed"}},
        )
        impact = next(
            item
            for item in workspace_state(self.db_path, self.project_id)["impacts"]
            if item["status"] == "affected"
        )
        revised_payload = deepcopy(initial_plan["payload"])
        revised_payload["chapters"][0]["goal"] = "使用钥匙"
        revised_plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            revised_payload,
            3,
            linked_impact_ids=[impact["id"]],
        )
        revised_review = create_review(
            self.db_path, self.project_id, "plan", revised_plan["revisionId"]
        )
        accepted_plan = accept_review(
            self.db_path,
            self.project_id,
            revised_review["id"],
            3,
            "accept-impact-replan",
        )
        self.assertEqual(accepted_plan["resolvedImpactIds"], [impact["id"]])
        self.assertNotIn(
            impact["id"],
            {
                item["id"]
                for item in workspace_state(self.db_path, self.project_id)["impacts"]
            },
        )
        undo_commit(
            self.db_path,
            self.project_id,
            accepted_plan["commitId"],
            4,
            "undo-linked-replan",
        )
        self.assertEqual(
            workspace_state(self.db_path, self.project_id)["impacts"][0]["id"],
            impact["id"],
        )

    def test_candidate_tools_are_bound_to_task_project_and_thread(self) -> None:
        task = create_task(
            self.db_path, self.project_id, "generateBrief", 0, "thread-a"
        )
        with self.assertRaisesRegex(WorkspaceError, "任务不属于当前作品或线程"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "错误线程"},
                0,
                "candidate",
                task["id"],
                "thread-b",
            )
        cancel_task(self.db_path, self.project_id, task["id"])
        with self.assertRaisesRegex(WorkspaceError, "任务已结束"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "取消后写入"},
                0,
                "candidate",
                task["id"],
                "thread-a",
            )

    def test_startup_recovery_cancels_active_tasks_and_revokes_capabilities(
        self,
    ) -> None:
        task = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-orphan",
            capability_hash="a" * 64,
            capability_expires_at=int(time.time()) + 1_800,
        )
        recovered = recover_orphaned_tasks(self.db_path)
        self.assertEqual(recovered, {"recovered": 1, "taskIds": [task["id"]]})
        persisted = next(
            item
            for item in workspace_state(self.db_path, self.project_id)["tasks"]
            if item["id"] == task["id"]
        )
        self.assertEqual(persisted["status"], "cancelled")
        self.assertEqual(persisted["error"], "应用重启后已回收孤儿任务")
        with sqlite3.connect(self.db_path) as connection:
            revoked_at = connection.execute(
                "SELECT capability_revoked_at FROM task_runs WHERE id = ?",
                (task["id"],),
            ).fetchone()[0]
        self.assertIsNotNone(revoked_at)

    def test_writer_task_is_bound_to_explicit_chapter_and_requires_persisted_output(
        self,
    ) -> None:
        with self.assertRaisesRegex(WorkspaceError, "必须绑定一个已经创建的明确章节"):
            create_task(
                self.db_path,
                self.project_id,
                "generateChapter",
                0,
                "thread-ambiguous-writer",
            )
        chapter_three = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 3,
                "title": "第三章",
                "body": "旧的第三章。",
                "sceneGroups": [{"id": "scene-3", "body": "旧的第三章。"}],
            },
            0,
        )
        chapter_ten = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 10,
                "title": "第十章",
                "body": "第十章保持不变。",
                "sceneGroups": [{"id": "scene-10", "body": "第十章保持不变。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-third",
            target={
                "type": "chapter",
                "chapterId": chapter_three["chapterId"],
                "chapterNumber": 3,
                "scope": {"type": "wholeChapter"},
            },
        )

        with self.assertRaisesRegex(WorkspaceError, "生成任务目标不一致"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterId": chapter_ten["chapterId"],
                    "body": "错误覆盖。",
                    "sceneGroups": [{"id": "scene-10", "body": "错误覆盖。"}],
                },
                0,
                "candidate",
                task["id"],
                "thread-third",
            )

        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": chapter_three["chapterId"],
                "body": "新的第三章。",
                "sceneGroups": [{"id": "scene-3", "body": "新的第三章。"}],
                "completeness": "chapterComplete",
            },
            0,
            "candidate",
            task["id"],
            "thread-third",
        )
        completed = complete_task(
            self.db_path, self.project_id, task["id"], "thread-third", "第三章已写完"
        )
        self.assertEqual(completed["deliveryStatus"], "ready_for_review")
        self.assertEqual(
            completed["outputRefs"],
            [
                {
                    "kind": "draft",
                    "revisionId": draft["revisionId"],
                    "chapterId": chapter_three["chapterId"],
                }
            ],
        )
        self.assertEqual(
            complete_task(
                self.db_path, self.project_id, task["id"], "thread-third", "重复完成"
            ),
            completed,
        )
        current = workspace_state(self.db_path, self.project_id)
        tenth = next(
            chapter
            for chapter in current["chapters"]
            if chapter["id"] == chapter_ten["chapterId"]
        )
        self.assertEqual(tenth["candidate"]["body"], "第十章保持不变。")

    def test_task_cannot_complete_after_its_candidate_is_superseded(self) -> None:
        first = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-first",
            target={"type": "artifact"},
        )
        first_candidate = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "第一版"},
            0,
            "candidate",
            first["id"],
            "thread-first",
            None,
            "same-write-key",
            True,
        )
        second = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-second",
            target={"type": "artifact"},
        )
        second_candidate = save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "第二版"},
            0,
            "candidate",
            second["id"],
            "thread-second",
            first_candidate["revisionId"],
            "same-write-key",
            True,
        )

        with self.assertRaisesRegex(WorkspaceError, "已被更新版本替换"):
            complete_task(self.db_path, self.project_id, first["id"], "thread-first")
        self.assertEqual(
            workspace_state(self.db_path, self.project_id)["artifacts"]["brief"][
                "candidate"
            ]["id"],
            second_candidate["revisionId"],
        )

    def test_candidate_idempotency_is_scoped_to_task(self) -> None:
        first = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-first",
            target={"type": "artifact"},
        )
        second = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-second",
            target={"type": "artifact"},
        )
        save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "第一版"},
            0,
            "candidate",
            first["id"],
            "thread-first",
            None,
            "shared-key",
            True,
        )

        with self.assertRaisesRegex(WorkspaceError, "候选版本已变化"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "第一版"},
                0,
                "candidate",
                second["id"],
                "thread-second",
                None,
                "shared-key",
                True,
            )
        with sqlite3.connect(self.db_path) as connection:
            self.assertEqual(
                connection.execute(
                    "SELECT COUNT(*) FROM task_outputs WHERE task_id = ?",
                    (second["id"],),
                ).fetchone()[0],
                0,
            )

    def test_task_cannot_complete_from_partial_or_claimed_output(self) -> None:
        chapter = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 2,
                "body": "已有内容。",
                "sceneGroups": [{"id": "scene-2", "body": "已有内容。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-partial",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
        )
        save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": chapter["chapterId"],
                "body": "只有片段。",
                "sceneGroups": [{"id": "scene-2", "body": "只有片段。"}],
                "completeness": "partial",
            },
            0,
            "candidate",
            task["id"],
            "thread-partial",
        )
        with self.assertRaisesRegex(WorkspaceError, "没有保存目标章节的有效草稿"):
            complete_task(self.db_path, self.project_id, task["id"], "thread-partial")
        persisted = next(
            item
            for item in workspace_state(self.db_path, self.project_id)["tasks"]
            if item["id"] == task["id"]
        )
        self.assertEqual(
            (persisted["status"], persisted["delivery_status"]),
            ("failed", "incomplete"),
        )

    def test_candidate_context_enforces_task_purpose_and_byte_budget(self) -> None:
        with self.assertRaisesRegex(WorkspaceError, "operation 无效"):
            create_task(
                self.db_path,
                self.project_id,
                "outline",
                0,
                "thread-invalid-purpose",
            )
        chapter = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 2,
                "body": "目标章节。",
                "sceneGroups": [{"id": "scene-target", "body": "目标章节。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-context",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
        )
        with self.assertRaisesRegex(WorkspaceError, "不能读取其他用途"):
            get_context_snapshot(
                self.db_path,
                self.project_id,
                "planner",
                0,
                actor="candidate",
                task_id=task["id"],
                thread_id="thread-context",
            )
        with self.assertRaisesRegex(WorkspaceError, "未授权"):
            save_artifact(
                self.db_path,
                self.project_id,
                "outline",
                {
                    "nodes": [
                        {
                            "clientKey": "book",
                            "kind": "book",
                            "position": 0,
                            "title": "误写",
                            "purpose": "误写",
                            "summary": "误写",
                            "dependencyKeys": [],
                            "certainty": "open",
                            "protection": "free",
                        }
                    ]
                },
                0,
                "candidate",
                task["id"],
                "thread-context",
            )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=2,
            scene_order=2_147_483_647,
            byte_budget=4_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-context",
        )
        self.assertLessEqual(context["contextBudget"]["usedBytes"], 4_000)
        self.assertEqual(context["readingChapter"], 2)
        with self.assertRaisesRegex(WorkspaceError, "冻结目标不一致"):
            get_context_snapshot(
                self.db_path,
                self.project_id,
                "writer",
                0,
                reading_chapter=2,
                scene_order=0,
                actor="candidate",
                task_id=task["id"],
                thread_id="thread-context",
            )

    def test_writer_context_uses_only_public_planning_projection(self) -> None:
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
                        "title": "秘密大纲标题 CANARY",
                        "publicTitle": "潮汐",
                        "purpose": "秘密目的 CANARY",
                        "summary": "秘密摘要 CANARY",
                        "dependencyKeys": [],
                        "certainty": "settled",
                        "protection": "free",
                        "writerGuidance": "保持潮湿与克制",
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
            "accept-public-outline",
        )
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "one",
                        "primaryNodeId": outline["payload"]["nodes"][0]["id"],
                        "chapterNumber": 1,
                        "title": "秘密章名 CANARY",
                        "publicTitle": "归潮",
                        "goal": "秘密目标 CANARY",
                        "conflict": "秘密冲突 CANARY",
                        "dependencyKeys": [],
                        "writerGuidance": "只写可见的归途",
                        "publicCommitments": ["钥匙保持未知"],
                        "allowances": [],
                    }
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(self.db_path, self.project_id, "plan", plan["revisionId"])[
                "id"
            ],
            1,
            "accept-public-plan",
        )
        chapter = materialize_chapter_from_plan(
            self.db_path,
            self.project_id,
            plan["payload"]["chapters"][0]["chapterPlanId"],
            2,
            "materialize-public-plan",
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            3,
            "thread-public-writer",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
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
            thread_id="thread-public-writer",
        )
        serialized = str(context)
        self.assertNotIn("CANARY", serialized)
        self.assertEqual(
            context["planningContext"]["chapter"]["writerGuidance"], "只写可见的归途"
        )
        self.assertEqual(
            context["planningContext"]["outlinePath"][0]["writerGuidance"],
            "保持潮湿与克制",
        )

    def test_writer_context_excludes_the_private_project_logline(self) -> None:
        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE projects SET logline = ? WHERE id = ?",
                ("PRIVATE_ENDING_CANARY：主角其实早已死亡", self.project_id),
            )

        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=1,
            scene_order=1,
        )

        self.assertNotIn("project", context)
        self.assertNotIn("PRIVATE_ENDING_CANARY", str(context))

    def test_private_plan_title_is_not_exposed_by_materialized_chapter_history(
        self,
    ) -> None:
        outline = self.accept_outline()
        node_id = outline["payload"]["nodes"][0]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "private-one",
                        "primaryNodeId": node_id,
                        "chapterNumber": 1,
                        "title": "PRIVATE PLAN TITLE CANARY",
                        "goal": "内部目标",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                    {
                        "clientKey": "private-two",
                        "primaryNodeId": node_id,
                        "chapterNumber": 2,
                        "title": "PRIVATE TARGET TITLE CANARY",
                        "goal": "内部目标",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(self.db_path, self.project_id, "plan", plan["revisionId"])[
                "id"
            ],
            1,
            "accept-private-title-plan",
        )
        first = materialize_chapter_from_plan(
            self.db_path,
            self.project_id,
            plan["payload"]["chapters"][0]["chapterPlanId"],
            2,
            "materialize-private-title-one",
        )
        first_draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": first["chapterId"],
                "body": "第一章正文。",
                "sceneGroups": [{"id": "first", "body": "第一章正文。"}],
            },
            3,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "draft", first_draft["revisionId"]
            )["id"],
            3,
            "accept-private-title-source",
            {"sceneGroupIds": ["first"], "noConfirmedChanges": True},
        )
        second = materialize_chapter_from_plan(
            self.db_path,
            self.project_id,
            plan["payload"]["chapters"][1]["chapterPlanId"],
            4,
            "materialize-private-title-two",
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            5,
            "thread-private-title",
            target={
                "type": "chapter",
                "chapterId": second["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            5,
            reading_chapter=2,
            scene_order=2_147_483_647,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-private-title",
        )

        self.assertNotIn("CANARY", str(context))
        self.assertEqual(context["sourceExcerpts"][0]["chapterTitle"], "第 1 章")

    def test_through_scene_context_hides_later_target_groups(self) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "第一场。\n\n---\n\n第二场。\n\n---\n\n第三场秘密。",
                "sceneGroups": [
                    {
                        "id": "scene-one",
                        "sceneId": "materialized-one",
                        "body": "第一场。",
                    },
                    {"id": "scene-two", "body": "第二场。"},
                    {"id": "scene-three", "body": "第三场秘密。"},
                ],
            },
            0,
        )
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            initial["chapterId"],
            initial["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-context-scope",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "throughScene", "endSceneOrder": 2},
            },
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            0,
            reading_chapter=1,
            scene_order=2,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-context-scope",
        )

        target_excerpt = context["sourceExcerpts"][0]
        self.assertEqual(target_excerpt["body"], "第一场。\n\n---\n\n第二场。")
        self.assertTrue(target_excerpt["truncated"])
        self.assertNotIn("第三场秘密", str(context))
        self.assertEqual(
            target_excerpt["sceneGroups"],
            [
                {
                    "id": "scene-one",
                    "sceneId": "materialized-one",
                    "sceneOrder": 1,
                    "locked": False,
                },
                {"id": "scene-two", "sceneId": None, "sceneOrder": 2, "locked": False},
            ],
        )
        saved = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": target_excerpt["body"],
                "sceneOrder": 2,
                "sceneGroups": [
                    {"id": group["id"], "sceneId": group["sceneId"], "body": body}
                    for group, body in zip(
                        target_excerpt["sceneGroups"],
                        target_excerpt["body"].split("\n\n---\n\n"),
                    )
                ],
            },
            0,
            "candidate",
            task["id"],
            "thread-context-scope",
            initial["revisionId"],
            "scope-context-roundtrip",
            True,
        )
        self.assertEqual(
            [group["id"] for group in saved["sceneGroups"]],
            ["scene-one", "scene-two", "scene-three"],
        )
        self.assertEqual(saved["sceneGroups"][0]["sceneId"], "materialized-one")
        self.assertTrue(saved["body"].endswith("第三场秘密。"))

    def test_candidate_context_requires_exact_task_revision(self) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "正文。",
                "sceneGroups": [{"id": "scene-one", "body": "正文。"}],
            },
            0,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "draft", initial["revisionId"]
            )["id"],
            0,
            "accept-context-revision-source",
            {"sceneGroupIds": ["scene-one"], "noConfirmedChanges": True},
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            1,
            "thread-exact-context-revision",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        for requested_revision in (None, 0, 2):
            with self.subTest(requested_revision=requested_revision):
                with self.assertRaisesRegex(WorkspaceError, "冻结版本不一致"):
                    get_context_snapshot(
                        self.db_path,
                        self.project_id,
                        "writer",
                        requested_revision,
                        reading_chapter=1,
                        scene_order=2_147_483_647,
                        actor="candidate",
                        task_id=task["id"],
                        thread_id="thread-exact-context-revision",
                    )

    def test_advisory_planner_reads_its_project_target_at_frozen_revision(
        self,
    ) -> None:
        task = create_task(
            self.db_path,
            self.project_id,
            "advisory",
            0,
            "thread-advisory-context",
            target={"type": "project"},
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "planner",
            0,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-advisory-context",
        )

        self.assertEqual(context["projectId"], self.project_id)
        self.assertEqual(context["projectRevision"], task["inputRevision"])
        self.assertEqual(task["target"], {"type": "project"})

    def test_generate_brief_context_preserves_the_bounded_project_idea(self) -> None:
        task = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-brief-project-idea",
        )

        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "planner",
            0,
            byte_budget=4_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-brief-project-idea",
        )

        self.assertEqual(
            context["project"],
            {
                "title": "测试长篇",
                "logline": "一个秘密改变了远期计划",
            },
        )
        self.assertLessEqual(context["contextBudget"]["usedBytes"], 4_000)

    def test_generate_outline_regeneration_reads_the_accepted_stable_outline(
        self,
    ) -> None:
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
                        "title": "潮汐之书",
                        "purpose": "守住归乡的核心意图",
                        "summary": "旧城随潮汐重现",
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
            "accept-outline-before-regeneration",
        )
        accepted_outline = workspace_state(self.db_path, self.project_id)["artifacts"][
            "outline"
        ]["accepted"]
        task = create_task(
            self.db_path,
            self.project_id,
            "generateOutline",
            1,
            "thread-outline-regeneration-context",
        )

        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "planner",
            1,
            byte_budget=60_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-outline-regeneration-context",
        )

        self.assertIsNone(context["acceptedArtifacts"]["brief"])
        self.assertEqual(context["acceptedArtifacts"]["outline"], accepted_outline)
        self.assertEqual(
            context["acceptedArtifacts"]["outline"]["payload"]["nodes"][0][
                "protection"
            ],
            "preserve-intent",
        )

    def test_generate_plan_context_reads_only_recent_accepted_prose(self) -> None:
        self.accept_outline()
        accepted_body = "旧潮。" * 2_000 + "最后，钥匙沉入井底。"
        accepted_draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "title": "退潮",
                "body": accepted_body,
                "sceneGroups": [{"id": "ebb", "body": accepted_body}],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path,
                self.project_id,
                "draft",
                accepted_draft["revisionId"],
            )["id"],
            1,
            "accept-prose-before-planning",
            {"sceneGroupIds": ["ebb"], "noConfirmedChanges": True},
        )
        save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 2,
                "title": "未采用章节",
                "body": "UNACCEPTED_PROSE_CANARY",
                "sceneGroups": [
                    {"id": "unaccepted", "body": "UNACCEPTED_PROSE_CANARY"}
                ],
            },
            2,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generatePlan",
            2,
            "thread-rolling-plan-context",
        )

        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "planner",
            2,
            byte_budget=60_000,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-rolling-plan-context",
        )

        self.assertEqual(
            context["sourceExcerpts"],
            [
                {
                    "chapterId": accepted_draft["chapterId"],
                    "chapterNumber": 1,
                    "chapterTitle": "退潮",
                    "draftRevisionId": accepted_draft["revisionId"],
                    "source": "author",
                    "relationship": "acceptedDraft",
                    "excerptRange": "tail",
                    "body": accepted_body[-6_000:],
                    "truncated": True,
                }
            ],
        )
        self.assertNotIn("UNACCEPTED_PROSE_CANARY", str(context))
        self.assertLessEqual(context["contextBudget"]["usedBytes"], 60_000)

    def test_multiple_writer_checkpoints_advance_task_candidate_head(self) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "开头。",
                "sceneGroups": [{"id": "scene-one", "body": "开头。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-checkpoints",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        first = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "第一检查点。",
                "sceneGroups": [{"id": "scene-one", "body": "第一检查点。"}],
                "completeness": "partial",
            },
            0,
            "candidate",
            task["id"],
            "thread-checkpoints",
            initial["revisionId"],
            "checkpoint-one",
            True,
        )
        second = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "第二检查点。",
                "sceneGroups": [{"id": "scene-one", "body": "第二检查点。"}],
                "completeness": "chapterComplete",
            },
            0,
            "candidate",
            task["id"],
            "thread-checkpoints",
            first["revisionId"],
            "checkpoint-two",
            True,
        )
        persisted = next(
            item
            for item in workspace_state(self.db_path, self.project_id)["tasks"]
            if item["id"] == task["id"]
        )
        self.assertEqual(persisted["candidate_head_revision_id"], second["revisionId"])

    def test_writer_checkpoint_cannot_exceed_explicit_scene_scope(self) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "开头。",
                "sceneGroups": [{"id": "scene-one", "body": "开头。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-scoped-writer",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "throughScene", "endSceneOrder": 2},
            },
        )
        with self.assertRaisesRegex(WorkspaceError, "超出任务授权"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterId": initial["chapterId"],
                    "body": "越界。",
                    "sceneOrder": 3,
                    "sceneGroups": [{"id": "scene-one", "body": "越界。"}],
                },
                0,
                "candidate",
                task["id"],
                "thread-scoped-writer",
                initial["revisionId"],
                "out-of-scope",
                True,
            )
        saved = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "范围内。",
                "sceneOrder": 2,
                "sceneGroups": [{"id": "scene-one", "body": "范围内。"}],
            },
            0,
            "candidate",
            task["id"],
            "thread-scoped-writer",
            initial["revisionId"],
            "in-scope",
            True,
        )
        self.assertEqual(saved["body"], "范围内。")

    def test_writer_checkpoint_preserves_groups_beyond_explicit_scene_scope(
        self,
    ) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "一。\n\n---\n\n二。\n\n---\n\n三。",
                "sceneGroups": [
                    {"id": "one", "body": "一。"},
                    {"id": "two", "body": "二。"},
                    {"id": "three", "body": "三。"},
                ],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-preserve-scope",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "throughScene", "endSceneOrder": 2},
            },
        )
        with self.assertRaisesRegex(WorkspaceError, "范围外"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterId": initial["chapterId"],
                    "body": "新一。\n\n---\n\n新二。\n\n---\n\n越界修改。",
                    "sceneOrder": 1,
                    "sceneGroups": [
                        {"id": "one", "body": "新一。"},
                        {"id": "two", "body": "新二。"},
                        {"id": "three", "body": "越界修改。"},
                    ],
                },
                0,
                "candidate",
                task["id"],
                "thread-preserve-scope",
                initial["revisionId"],
                "modify-outside-scope",
                True,
            )
        with self.assertRaisesRegex(WorkspaceError, "范围外"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterId": initial["chapterId"],
                    "body": "新一。\n\n---\n\n新二。\n\n---\n\n三。\n\n---\n\n新增。",
                    "sceneOrder": 2,
                    "sceneGroups": [
                        {"id": "one", "body": "新一。"},
                        {"id": "two", "body": "新二。"},
                        {"id": "three", "body": "三。"},
                        {"id": "four", "body": "新增。"},
                    ],
                },
                0,
                "candidate",
                task["id"],
                "thread-preserve-scope",
                initial["revisionId"],
                "add-outside-scope",
                True,
            )
        saved = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "新一。\n\n---\n\n新二。",
                "sceneOrder": 2,
                "sceneGroups": [
                    {"id": "one", "body": "新一。"},
                    {"id": "two", "body": "新二。"},
                ],
            },
            0,
            "candidate",
            task["id"],
            "thread-preserve-scope",
            initial["revisionId"],
            "preserve-outside-scope",
            True,
        )
        self.assertEqual(
            [group["body"] for group in saved["sceneGroups"]],
            ["新一。", "新二。", "三。"],
        )

        self.assertEqual(saved["body"], "新一。\n\n---\n\n新二。\n\n---\n\n三。")
        self.assertEqual(saved["sceneGroups"][-1]["id"], "three")
        self.assertEqual(
            saved["sceneGroups"][-1]["candidateDraftRevisionId"], saved["revisionId"]
        )

    def test_fact_proposal_review_retry_is_idempotent(self) -> None:
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "她收起钥匙。",
                "sceneGroups": [{"id": "key", "body": "她收起钥匙。"}],
            },
            0,
        )
        proposals = [
            {
                "sceneGroupId": "key",
                "factKind": "world",
                "predicate": "has-key",
                "value": True,
                "epistemicState": "known",
                "readingChapter": 1,
                "sceneOrder": 1,
                "evidence": "她收起钥匙。",
            }
        ]
        first = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            proposals,
            idempotency_key="review-key",
        )
        replay = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            proposals,
            idempotency_key="review-key",
        )
        self.assertEqual(replay, first)
        self.assertEqual(
            len(workspace_state(self.db_path, self.project_id)["factProposals"]), 1
        )

    def test_large_accepted_brief_fits_supported_planner_context(self) -> None:
        candidate = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "龙" * 9_000}, 0
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "brief", candidate["revisionId"]
            )["id"],
            0,
            "accept-large-brief",
        )
        context = get_context_snapshot(
            self.db_path, self.project_id, "planner", 1, byte_budget=32_000
        )
        self.assertLessEqual(context["contextBudget"]["usedBytes"], 32_000)
        self.assertEqual(
            context["acceptedArtifacts"]["brief"]["payload"]["premise"], "龙" * 9_000
        )

    def test_complete_legal_brief_fits_bounded_planner_context(self) -> None:
        payload = {
            "premise": "p" * 10_000,
            "promise": "r" * 10_000,
            "stakes": "s" * 10_000,
            "endingDirection": "e" * 10_000,
        }
        candidate = save_artifact(self.db_path, self.project_id, "brief", payload, 0)
        accept_review(
            self.db_path,
            self.project_id,
            create_review(
                self.db_path, self.project_id, "brief", candidate["revisionId"]
            )["id"],
            0,
            "accept-complete-large-brief",
        )

        context = get_context_snapshot(
            self.db_path, self.project_id, "planner", 1, byte_budget=60_000
        )

        self.assertLessEqual(context["contextBudget"]["usedBytes"], 60_000)
        self.assertEqual(context["acceptedArtifacts"]["brief"]["payload"], payload)

    def test_writer_context_cites_accepted_source_revision(self) -> None:
        source = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "title": "前章",
                "body": "父亲把沾着盐的钥匙放回抽屉。",
                "sceneGroups": [
                    {"id": "source-scene", "body": "父亲把沾着盐的钥匙放回抽屉。"}
                ],
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "draft", source["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "accept-context-source",
            {"sceneGroupIds": ["source-scene"], "noConfirmedChanges": True},
        )
        target = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 2,
                "title": "晚餐",
                "body": "待重写。",
                "sceneGroups": [{"id": "target-scene", "body": "待重写。"}],
            },
            1,
        )
        set_working_baseline(
            self.db_path,
            self.project_id,
            "draft",
            target["chapterId"],
            target["revisionId"],
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            1,
            "thread-cited-context",
            target={
                "type": "chapter",
                "chapterId": target["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            1,
            reading_chapter=2,
            scene_order=2_147_483_647,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-cited-context",
        )
        self.assertEqual(
            context["sourceExcerpts"],
            [
                {
                    "chapterId": target["chapterId"],
                    "chapterNumber": 2,
                    "chapterTitle": "晚餐",
                    "draftRevisionId": target["revisionId"],
                    "source": "author",
                    "relationship": "targetDraft",
                    "sourceRole": "workingSelected",
                    "evidenceClass": "required",
                    "sceneGroups": [
                        {
                            "id": "target-scene",
                            "sceneId": None,
                            "sceneOrder": 1,
                            "locked": False,
                        }
                    ],
                    "body": "待重写。",
                    "truncated": False,
                },
                {
                    "chapterId": source["chapterId"],
                    "chapterNumber": 1,
                    "chapterTitle": "前章",
                    "draftRevisionId": source["revisionId"],
                    "source": "author",
                    "body": "父亲把沾着盐的钥匙放回抽屉。",
                    "truncated": False,
                },
            ],
        )

    def test_review_rejects_fact_evidence_outside_its_scene(self) -> None:
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "她把钥匙放在桌上。",
                "sceneGroups": [{"id": "key-scene", "body": "她把钥匙放在桌上。"}],
            },
            0,
        )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "key-scene",
                    "factKind": "world",
                    "predicate": "door-open",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "门已经打开。",
                }
            ],
        )
        anchored = next(
            check for check in review["checks"] if check["id"] == "fact-evidence-anchor"
        )
        semantic = next(
            check for check in review["checks"] if check["id"] == "semantic-review"
        )
        self.assertEqual((anchored["passed"], anchored["blocking"]), (False, True))
        self.assertEqual((semantic["passed"], semantic["blocking"]), (False, False))
        fact_id = review["factProposals"][0]["id"]
        with self.assertRaises(WorkspaceError) as raised:
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                0,
                "confirm-unanchored-fact",
                {
                    "sceneGroupIds": ["key-scene"],
                    "factDecisions": {fact_id: "confirmed"},
                },
            )
        self.assertEqual(
            (raised.exception.code, raised.exception.details),
            (
                "review_checks_failed",
                {
                    "checkIds": ["fact-evidence-anchor"],
                    "factIds": [fact_id],
                },
            ),
        )

        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "reject-unanchored-fact",
            {
                "sceneGroupIds": ["key-scene"],
                "factDecisions": {fact_id: "rejected"},
            },
        )

        self.assertEqual(receipt["resultRevision"], 1)
        self.assertEqual(
            get_context_snapshot(
                self.db_path,
                self.project_id,
                "critic",
                1,
                reading_chapter=1,
                scene_order=1,
            )["facts"],
            [],
        )

    def test_stale_candidate_cannot_be_accepted_after_another_formal_change(
        self,
    ) -> None:
        brief = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "旧版本命题"}, 0
        )
        brief_review = create_review(
            self.db_path, self.project_id, "brief", brief["revisionId"]
        )
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
                        "purpose": "从开始走向选择",
                        "summary": "转折之后留下余波",
                        "dependencyKeys": [],
                        "certainty": "tentative",
                        "protection": "free",
                    }
                ]
            },
            0,
        )
        outline_review = create_review(
            self.db_path, self.project_id, "outline", outline["revisionId"]
        )
        accept_review(
            self.db_path,
            self.project_id,
            outline_review["id"],
            0,
            "accept-outline-first",
        )

        with self.assertRaisesRegex(WorkspaceError, "审阅基于旧版本"):
            accept_review(
                self.db_path,
                self.project_id,
                brief_review["id"],
                0,
                "accept-stale-brief",
            )

    def test_partial_scene_acceptance_rejects_facts_from_unselected_scenes(
        self,
    ) -> None:
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "title": "两场",
                "body": "第一场确认钥匙。\n\n---\n\n第二场揭示密门。",
                "sceneGroups": [
                    {"id": "scene-a", "body": "第一场确认钥匙。"},
                    {"id": "scene-b", "body": "第二场揭示密门。"},
                ],
            },
            0,
        )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "scene-a",
                    "factKind": "world",
                    "predicate": "has-key",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "第一场确认钥匙。",
                },
                {
                    "sceneGroupId": "scene-b",
                    "factKind": "world",
                    "predicate": "door-open",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 2,
                    "evidence": "第二场揭示密门。",
                },
            ],
        )
        decisions = {
            proposal["id"]: "confirmed" for proposal in review["factProposals"]
        }
        with self.assertRaisesRegex(WorkspaceError, "不能确认未采用场景"):
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                0,
                "invalid-partial",
                {"sceneGroupIds": ["scene-a"], "factDecisions": decisions},
            )

        first_fact = next(
            proposal
            for proposal in review["factProposals"]
            if proposal["sceneGroupId"] == "scene-a"
        )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            0,
            "valid-partial",
            {
                "sceneGroupIds": ["scene-a"],
                "factDecisions": {first_fact["id"]: "confirmed"},
            },
        )
        self.assertEqual(
            [fact["predicate"] for fact in receipt["confirmedFacts"]], ["has-key"]
        )
        accepted = workspace_state(self.db_path, self.project_id)["chapters"][0][
            "accepted"
        ]
        self.assertEqual(accepted["body"], "第一场确认钥匙。")

    def test_explicit_empty_scene_selection_never_falls_back_to_all_scenes(
        self,
    ) -> None:
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "第一场。\n\n---\n\n第二场。",
                "sceneGroups": [
                    {"id": "scene-a", "body": "第一场。"},
                    {"id": "scene-b", "body": "第二场。"},
                ],
            },
            0,
        )
        review = create_review(
            self.db_path, self.project_id, "draft", draft["revisionId"]
        )

        with self.assertRaises(WorkspaceError) as raised:
            accept_review(
                self.db_path,
                self.project_id,
                review["id"],
                0,
                "reject-empty-scene-selection",
                {"sceneGroupIds": [], "noConfirmedChanges": True},
            )

        self.assertEqual(raised.exception.code, "scene_selection_required")
        current = workspace_state(self.db_path, self.project_id)
        self.assertEqual(current["revision"], 0)
        self.assertIsNone(current["chapters"][0]["accepted"])

    def test_draft_body_must_match_scene_change_groups(self) -> None:
        with self.assertRaisesRegex(WorkspaceError, "正文与场景变更组内容不一致"):
            save_draft_checkpoint(
                self.db_path,
                self.project_id,
                {
                    "chapterNumber": 1,
                    "body": "正文 A",
                    "sceneGroups": [{"id": "scene-a", "body": "正文 B"}],
                },
                0,
            )

    def test_writer_context_excludes_future_plan_and_full_artifacts(self) -> None:
        outline = self.accept_outline()
        root_id = outline["payload"]["nodes"][0]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "window": 2,
                "chapters": [
                    {
                        "clientKey": "chapter-plan-context-1",
                        "primaryNodeId": root_id,
                        "chapterNumber": 1,
                        "title": "现在",
                        "goal": "发现线索",
                        "conflict": "隐瞒",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                    {
                        "clientKey": "chapter-plan-context-2",
                        "primaryNodeId": root_id,
                        "chapterNumber": 2,
                        "title": "未来",
                        "goal": "揭示秘密",
                        "conflict": "背叛",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                ],
            },
            1,
        )
        review = create_review(
            self.db_path, self.project_id, "plan", plan["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, review["id"], 1, "accept-context-plan"
        )
        context = get_context_snapshot(
            self.db_path, self.project_id, "writer", 2, reading_chapter=1, scene_order=1
        )
        self.assertEqual(
            context["acceptedArtifacts"], {"brief": None, "outline": None, "plan": None}
        )
        self.assertEqual(context["commitments"], [])

    def test_hierarchical_outline_plan_and_lazy_chapter_materialization(self) -> None:
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
                        "publicTitle": "全书",
                        "purpose": "关系修复",
                        "summary": "",
                        "dependencyKeys": ["daughter-trust"],
                        "certainty": "settled",
                        "protection": "preserve-intent",
                        "writerGuidance": "围绕信任变化推进",
                    },
                    {
                        "clientKey": "unit",
                        "parentClientKey": "book",
                        "kind": "unit",
                        "position": 0,
                        "title": "晚餐单元",
                        "purpose": "试探",
                        "summary": "",
                        "dependencyKeys": ["daughter-trust"],
                        "certainty": "tentative",
                        "protection": "free",
                        "writerGuidance": "克制揭示",
                    },
                ]
            },
            0,
            idempotency_key="outline-v2",
        )
        self.assertNotIn("clientKey", outline["payload"]["nodes"][0])
        outline_review = create_review(
            self.db_path, self.project_id, "outline", outline["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, outline_review["id"], 0, "accept-outline-v2"
        )
        unit_id = outline["payload"]["nodes"][1]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "chapter-one",
                        "chapterNumber": 1,
                        "primaryNodeId": unit_id,
                        "title": "父女晚餐",
                        "publicTitle": "晚餐",
                        "goal": "女儿开始试探",
                        "conflict": "父亲隐瞒",
                        "dependencyKeys": ["daughter-trust"],
                        "allowances": [],
                        "writerGuidance": "用动作表现试探",
                        "publicCommitments": ["不揭示父亲身份"],
                    },
                    {
                        "clientKey": "chapter-two",
                        "chapterNumber": 2,
                        "primaryNodeId": unit_id,
                        "title": "晚餐余波",
                        "publicTitle": "余波",
                        "goal": "检验信任",
                        "conflict": "外界施压",
                        "dependencyKeys": ["daughter-trust"],
                        "allowances": [],
                        "writerGuidance": "保留余地",
                        "publicCommitments": [],
                    },
                ],
            },
            1,
            idempotency_key="plan-v2",
        )
        chapter_plan_id = plan["payload"]["chapters"][0]["chapterPlanId"]
        plan_review = create_review(
            self.db_path, self.project_id, "plan", plan["revisionId"]
        )
        accept_review(
            self.db_path, self.project_id, plan_review["id"], 1, "accept-plan-v2"
        )
        receipt = materialize_chapter_from_plan(
            self.db_path, self.project_id, chapter_plan_id, 2, "materialize-one"
        )
        replay = materialize_chapter_from_plan(
            self.db_path, self.project_id, chapter_plan_id, 2, "materialize-one"
        )
        self.assertEqual(replay, receipt)
        standalone = materialize_standalone_chapter(
            self.db_path, self.project_id, "第二章独立稿", 3, "standalone-two"
        )
        second_plan_id = plan["payload"]["chapters"][1]["chapterPlanId"]
        associated = associate_standalone_chapter_to_plan(
            self.db_path,
            self.project_id,
            standalone["chapterId"],
            second_plan_id,
            4,
            "associate-two",
        )
        self.assertEqual(associated["chapterPlanId"], second_plan_id)
        current = workspace_state(self.db_path, self.project_id)
        self.assertEqual(
            {
                binding["chapter_plan_id"]: binding["chapter_id"]
                for binding in current["chapterPlanBindings"]
            },
            {
                chapter_plan_id: receipt["chapterId"],
                second_plan_id: standalone["chapterId"],
            },
        )
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": receipt["chapterId"],
                "body": "父女在沉默中试探。",
                "sceneGroups": [{"id": "dinner", "body": "父女在沉默中试探。"}],
            },
            5,
            expected_candidate_revision_id=None,
            idempotency_key="planned-draft-one",
        )
        replayed_draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": receipt["chapterId"],
                "body": "父女在沉默中试探。",
                "sceneGroups": [{"id": "dinner", "body": "父女在沉默中试探。"}],
            },
            5,
            expected_candidate_revision_id=None,
            idempotency_key="planned-draft-one",
        )
        self.assertEqual(replayed_draft, draft)
        refs = workspace_state(self.db_path, self.project_id)["chapters"][0][
            "candidate"
        ]["sourcePlanRefs"]
        self.assertEqual(
            refs,
            {
                "lineageStatus": "current",
                "planRevisionId": plan["revisionId"],
                "outlineRevisionId": outline["revisionId"],
                "chapterPlanId": chapter_plan_id,
                "localOnly": False,
            },
        )

    def test_candidate_cas_rejects_overwriting_a_newer_candidate(self) -> None:
        first = save_artifact(
            self.db_path, self.project_id, "brief", {"premise": "第一版"}, 0
        )
        with self.assertRaisesRegex(WorkspaceError, "候选版本已变化"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "并发初稿"},
                0,
                expected_candidate_revision_id=None,
                enforce_candidate_cas=True,
            )
        save_artifact(
            self.db_path,
            self.project_id,
            "brief",
            {"premise": "第二版"},
            0,
            expected_candidate_revision_id=first["revisionId"],
        )
        with self.assertRaisesRegex(WorkspaceError, "候选版本已变化"):
            save_artifact(
                self.db_path,
                self.project_id,
                "brief",
                {"premise": "旧编辑覆盖"},
                0,
                expected_candidate_revision_id=first["revisionId"],
            )

    def test_confirmed_fact_feeds_back_to_plan_unit_and_book(self) -> None:
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
                        "purpose": "信任",
                        "summary": "",
                        "dependencyKeys": ["daughter-trust"],
                        "certainty": "settled",
                        "protection": "preserve-intent",
                    },
                    {
                        "clientKey": "unit",
                        "parentClientKey": "book",
                        "kind": "unit",
                        "position": 0,
                        "title": "晚餐单元",
                        "purpose": "试探",
                        "summary": "",
                        "dependencyKeys": ["daughter-trust"],
                        "certainty": "tentative",
                        "protection": "free",
                    },
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
            "accept-feedback-outline",
        )
        unit_id = outline["payload"]["nodes"][1]["id"]
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "one",
                        "chapterNumber": 1,
                        "primaryNodeId": unit_id,
                        "title": "晚餐",
                        "goal": "试探",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                    {
                        "clientKey": "two",
                        "chapterNumber": 2,
                        "primaryNodeId": unit_id,
                        "title": "余波",
                        "goal": "回应信任",
                        "dependencyKeys": ["daughter-trust"],
                        "publicCommitments": [],
                        "allowances": [],
                    },
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(self.db_path, self.project_id, "plan", plan["revisionId"])[
                "id"
            ],
            1,
            "accept-feedback-plan",
        )
        first_plan_id = plan["payload"]["chapters"][0]["chapterPlanId"]
        chapter = materialize_chapter_from_plan(
            self.db_path, self.project_id, first_plan_id, 2, "materialize-feedback-one"
        )
        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": chapter["chapterId"],
                "body": "女儿第一次选择相信父亲。",
                "sceneGroups": [{"id": "trust", "body": "女儿第一次选择相信父亲。"}],
            },
            3,
        )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            draft["revisionId"],
            [
                {
                    "sceneGroupId": "trust",
                    "factKind": "world",
                    "predicate": "daughter-trust",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "女儿第一次选择相信父亲。",
                }
            ],
        )
        receipt = accept_review(
            self.db_path,
            self.project_id,
            review["id"],
            3,
            "accept-feedback-draft",
            {
                "sceneGroupIds": ["trust"],
                "factDecisions": {review["factProposals"][0]["id"]: "confirmed"},
                "directionChangeDecisions": {
                    impact["id"]: "accept-and-open-impact"
                    for impact in review["provisionalImpacts"]
                    if impact["severity"] == "locked-conflict"
                },
            },
        )
        self.assertEqual(
            {
                (
                    impact["targetArtifactKind"],
                    impact["targetLabel"],
                    impact["severity"],
                )
                for impact in receipt["impacts"]
            },
            {
                ("plan", "余波", "affected"),
                ("outline", "晚餐单元", "affected"),
                ("outline", "全书", "locked-conflict"),
            },
        )
        impact = next(
            item for item in receipt["impacts"] if item["targetArtifactKind"] == "plan"
        )
        resolved = resolve_planning_impact(
            self.db_path,
            self.project_id,
            impact["id"],
            4,
            "kept",
            "正文变化不影响第二章的回应方式",
            "keep-feedback-plan",
        )
        self.assertEqual(resolved["resultRevision"], 5)
        impact_history = planning_impact_page(self.db_path, self.project_id, limit=50)
        retained = next(
            item for item in impact_history["data"] if item["id"] == impact["id"]
        )
        self.assertEqual(retained["state"], "resolved-kept")
        self.assertEqual(retained["sourceChapterNumber"], 1)
        self.assertEqual(retained["sourceChapterTitle"], "第 1 章")
        self.assertEqual(retained["sourceChapterId"], chapter["chapterId"])
        self.assertEqual(impact_history["counts"]["retained"], 1)
        remaining = workspace_state(self.db_path, self.project_id)["impacts"]
        unit_impact = next(
            item for item in remaining if item["targetLabel"] == "晚餐单元"
        )
        book_impact = next(item for item in remaining if item["targetLabel"] == "全书")

        deleting = save_artifact(
            self.db_path,
            self.project_id,
            "outline",
            {"nodes": [outline["payload"]["nodes"][0]]},
            5,
        )
        deleting_review = create_review(
            self.db_path, self.project_id, "outline", deleting["revisionId"]
        )
        with self.assertRaisesRegex(WorkspaceError, "删除了仍有待处理反馈的节点"):
            accept_review(
                self.db_path,
                self.project_id,
                deleting_review["id"],
                5,
                "reject-deleting-impacted-unit",
            )

        revised_payload = deepcopy(outline["payload"])
        revised_payload["nodes"][1]["summary"] = "信任已经建立，单元改为检验信任的代价"
        revised = save_artifact(
            self.db_path,
            self.project_id,
            "outline",
            revised_payload,
            5,
            expected_candidate_revision_id=deleting["revisionId"],
            linked_impact_ids=[unit_impact["id"]],
        )
        revised_review = create_review(
            self.db_path, self.project_id, "outline", revised["revisionId"]
        )
        outline_acceptance = accept_review(
            self.db_path,
            self.project_id,
            revised_review["id"],
            5,
            "accept-updated-feedback-outline",
        )
        migrated_open = workspace_state(self.db_path, self.project_id)["impacts"]
        self.assertEqual(len(migrated_open), 1)
        self.assertEqual(migrated_open[0]["targetRevisionId"], revised["revisionId"])
        self.assertEqual(migrated_open[0]["supersedesImpactId"], book_impact["id"])
        self.assertEqual(
            workspace_state(self.db_path, self.project_id)["planLineageStatus"], "stale"
        )
        historical_writer = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            6,
            "stale-plan-writer",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        self.assertEqual(
            historical_writer["inputManifest"]["planningBaseline"],
            {
                "outlineRevisionId": outline["revisionId"],
                "planRevisionId": plan["revisionId"],
                "chapterPlanId": first_plan_id,
                "planSourceRole": "accepted",
                "lineageStatus": "historical",
            },
        )
        second_plan_id = plan["payload"]["chapters"][1]["chapterPlanId"]
        with self.assertRaisesRegex(WorkspaceError, "章纲已不再基于当前正式大纲"):
            materialize_chapter_from_plan(
                self.db_path,
                self.project_id,
                second_plan_id,
                6,
                "stale-plan-materialize",
            )
        self.assertEqual(
            workspace_state(self.db_path, self.project_id)["openImpactCount"], 1
        )
        undo_commit(
            self.db_path,
            self.project_id,
            outline_acceptance["commitId"],
            6,
            "undo-impact-target-migration",
        )
        restored_impacts = workspace_state(self.db_path, self.project_id)["impacts"]
        self.assertEqual(
            {impact["targetRevisionId"] for impact in restored_impacts},
            {outline["revisionId"]},
        )
        self.assertEqual(len(restored_impacts), 2)

    def test_undo_restores_the_exact_previous_fact_state(self) -> None:
        first = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "门是开启的。",
                "sceneGroups": [{"id": "door-scene", "body": "门是开启的。"}],
            },
            0,
        )
        first_review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            first["revisionId"],
            [
                {
                    "sceneGroupId": "door-scene",
                    "factKind": "world",
                    "predicate": "door-open",
                    "value": True,
                    "epistemicState": "known",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "门是开启的。",
                }
            ],
        )
        first_decision = {first_review["factProposals"][0]["id"]: "confirmed"}
        accept_review(
            self.db_path,
            self.project_id,
            first_review["id"],
            0,
            "accept-door-open",
            {"sceneGroupIds": ["door-scene"], "factDecisions": first_decision},
        )

        second = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": first["chapterId"],
                "body": "门已经关闭。",
                "sceneGroups": [{"id": "door-scene", "body": "门已经关闭。"}],
            },
            1,
        )
        second_review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            second["revisionId"],
            [
                {
                    "sceneGroupId": "door-scene",
                    "factKind": "world",
                    "predicate": "door-open",
                    "value": False,
                    "epistemicState": "false",
                    "readingChapter": 1,
                    "sceneOrder": 1,
                    "evidence": "门已经关闭。",
                }
            ],
        )
        second_decision = {second_review["factProposals"][0]["id"]: "confirmed"}
        second_receipt = accept_review(
            self.db_path,
            self.project_id,
            second_review["id"],
            1,
            "accept-door-closed",
            {"sceneGroupIds": ["door-scene"], "factDecisions": second_decision},
        )
        undo_commit(
            self.db_path,
            self.project_id,
            second_receipt["commitId"],
            2,
            "undo-door-closed",
        )

        context = get_context_snapshot(
            self.db_path, self.project_id, "critic", 3, reading_chapter=1, scene_order=3
        )
        door = next(
            fact for fact in context["facts"] if fact["predicate"] == "door-open"
        )
        self.assertEqual(door["value"], True)
        self.assertEqual(door["epistemicState"], "known")

    def test_structural_chapter_creation_can_be_undone_without_dependencies(
        self,
    ) -> None:
        created = materialize_standalone_chapter(
            self.db_path, self.project_id, "临时章节", 0, "create-temporary"
        )
        undone = undo_commit(
            self.db_path, self.project_id, created["commitId"], 1, "undo-temporary"
        )
        self.assertEqual(undone["undoneOperation"], "materialize-standalone")
        self.assertNotIn(
            created["chapterId"],
            {
                chapter["id"]
                for chapter in workspace_state(self.db_path, self.project_id)[
                    "chapters"
                ]
            },
        )

    def test_local_only_writer_does_not_inherit_unrelated_accepted_planning(
        self,
    ) -> None:
        outline = self.accept_outline()
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "unmaterialized-plan",
                        "primaryNodeId": outline["payload"]["nodes"][0]["id"],
                        "chapterNumber": 1,
                        "goal": "不属于独立章节的内部目标",
                        "dependencyKeys": [],
                        "publicCommitments": ["不相关规划承诺"],
                        "allowances": [],
                    }
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(self.db_path, self.project_id, "plan", plan["revisionId"])[
                "id"
            ],
            1,
            "accept-unrelated-plan",
        )
        chapter = materialize_standalone_chapter(
            self.db_path, self.project_id, "独立第一章", 2, "standalone-local-only"
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            3,
            "thread-local-only",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": chapter["chapterNumber"],
                "scope": {"type": "wholeChapter"},
            },
        )

        self.assertNotIn("planningBaseline", task["inputManifest"])
        self.assertEqual(
            task["inputManifest"]["localOnlySnapshot"],
            {
                "chapterId": chapter["chapterId"],
                "projectRevision": 3,
                "localOnly": True,
            },
        )
        context = get_context_snapshot(
            self.db_path,
            self.project_id,
            "writer",
            3,
            reading_chapter=chapter["chapterNumber"],
            scene_order=2_147_483_647,
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-local-only",
        )
        self.assertIsNone(context["planningContext"])
        self.assertNotIn("不相关规划承诺", str(context))

        draft = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": chapter["chapterId"],
                "body": "独立章节正文。",
                "sceneGroups": [{"id": "local-scene", "body": "独立章节正文。"}],
            },
            3,
            "candidate",
            task["id"],
            "thread-local-only",
            expected_candidate_revision_id=None,
            idempotency_key="local-only-draft",
            enforce_candidate_cas=True,
        )
        saved = next(
            item
            for item in workspace_state(self.db_path, self.project_id)["chapters"]
            if item["id"] == chapter["chapterId"]
        )["candidate"]
        self.assertEqual(saved["id"], draft["revisionId"])
        self.assertEqual(
            saved["sourcePlanRefs"],
            {"lineageStatus": "current", "localOnly": True},
        )

    def test_plan_association_and_feedback_resolution_can_be_undone(self) -> None:
        outline = self.accept_outline()
        plan = save_artifact(
            self.db_path,
            self.project_id,
            "plan",
            {
                "sourceOutlineRevisionId": outline["revisionId"],
                "chapters": [
                    {
                        "clientKey": "one",
                        "primaryNodeId": outline["payload"]["nodes"][0]["id"],
                        "chapterNumber": 1,
                        "goal": "试写",
                        "dependencyKeys": [],
                        "publicCommitments": [],
                        "allowances": [],
                    }
                ],
            },
            1,
        )
        accept_review(
            self.db_path,
            self.project_id,
            create_review(self.db_path, self.project_id, "plan", plan["revisionId"])[
                "id"
            ],
            1,
            "accept-undo-plan",
        )
        chapter = materialize_standalone_chapter(
            self.db_path, self.project_id, "独立第一章", 2, "create-independent"
        )
        associated = associate_standalone_chapter_to_plan(
            self.db_path,
            self.project_id,
            chapter["chapterId"],
            plan["payload"]["chapters"][0]["chapterPlanId"],
            3,
            "associate-independent",
        )
        undo_commit(
            self.db_path, self.project_id, associated["commitId"], 4, "undo-association"
        )
        current = workspace_state(self.db_path, self.project_id)
        self.assertEqual(current["chapterPlanBindings"], [])
        self.assertIn(
            chapter["chapterId"], {item["id"] for item in current["chapters"]}
        )

        with sqlite3.connect(self.db_path) as connection:
            connection.execute("PRAGMA foreign_keys = ON")
            connection.execute(
                "INSERT INTO planning_impacts(id, impact_chain_id, project_id, target_artifact_kind, target_revision_id, target_node_id, target_label, dependency_key, severity, state, created_at) VALUES (?, ?, ?, 'plan', ?, ?, '第一章', 'test', 'affected', 'open', ?)",
                (
                    "impact-undo",
                    "impact-chain-undo",
                    self.project_id,
                    plan["revisionId"],
                    plan["payload"]["chapters"][0]["chapterPlanId"],
                    "2026-09-13T00:00:00Z",
                ),
            )
        resolved = resolve_planning_impact(
            self.db_path,
            self.project_id,
            "impact-undo",
            5,
            "false-positive",
            "确认没有实际依赖",
            "resolve-for-undo",
        )
        undo_commit(
            self.db_path, self.project_id, resolved["commitId"], 6, "undo-resolution"
        )
        self.assertEqual(
            workspace_state(self.db_path, self.project_id)["impacts"][0]["id"],
            "impact-undo",
        )

    def test_current_schema_initializes_without_compatibility_tables(self) -> None:
        fresh_db = Path(self.directory.name) / "fresh.sqlite3"
        initialize(fresh_db)
        initialize(fresh_db)
        current = workspace_state(fresh_db, "project-last-lamp")

        with sqlite3.connect(fresh_db) as connection:
            table_names = {
                row[0]
                for row in connection.execute(
                    "SELECT name FROM sqlite_master WHERE type = 'table'"
                )
            }
            draft_columns = {
                row[1]
                for row in connection.execute("PRAGMA table_info(draft_revisions)")
            }
            binding_columns = {
                row[1]
                for row in connection.execute(
                    "PRAGMA table_info(chapter_plan_bindings)"
                )
            }

        self.assertNotIn("plan_impacts", table_names)
        self.assertNotIn("source_plan_revision_id", draft_columns)
        self.assertIn("source_plan_refs_json", draft_columns)
        self.assertIn("plan_revision_id", binding_columns)
        self.assertEqual(current["revision"], 0)
        self.assertEqual(current["chapters"][0]["accepted"]["source"], "author")

    def test_operation_is_the_only_task_authority_and_writer_target_is_normalized(
        self,
    ) -> None:
        with self.assertRaisesRegex(WorkspaceError, "operation 无效"):
            create_task(self.db_path, self.project_id, "draft", 0, "thread-invalid")
        brief_task = create_task(
            self.db_path,
            self.project_id,
            "generateBrief",
            0,
            "thread-brief-only",
        )
        with self.assertRaisesRegex(WorkspaceError, "未授权"):
            save_artifact(
                self.db_path,
                self.project_id,
                "outline",
                {
                    "nodes": [
                        {
                            "clientKey": "book",
                            "kind": "book",
                            "position": 0,
                            "title": "越权大纲",
                            "purpose": "越权",
                            "summary": "",
                            "dependencyKeys": [],
                            "certainty": "tentative",
                            "protection": "free",
                        }
                    ]
                },
                0,
                "candidate",
                brief_task["id"],
                "thread-brief-only",
            )
        chapter = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 2,
                "body": "目标章。",
                "sceneGroups": [{"id": "target", "body": "目标章。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-writer-contract",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 2,
                "scope": {"type": "wholeChapter"},
            },
        )

        self.assertEqual(
            (task["kind"], task["purpose"], task["requiredOutputKind"]),
            ("draft", "writer", "draft"),
        )
        self.assertEqual(task["target"]["readingChapter"], 2)
        self.assertEqual(task["target"]["sceneOrder"], 2_147_483_647)

    def test_plan_pov_and_story_time_are_validated_and_frozen_into_writer_task(
        self,
    ) -> None:
        outline = self.accept_outline()
        node_id = outline["payload"]["nodes"][0]["id"]
        base_plan = {
            "sourceOutlineRevisionId": outline["revisionId"],
            "chapters": [
                {
                    "clientKey": "chapter-one",
                    "chapterNumber": 1,
                    "primaryNodeId": node_id,
                    "title": "第一章",
                    "goal": "进入困境",
                    "dependencyKeys": [],
                    "publicCommitments": [],
                    "allowances": [],
                }
            ],
        }
        invalid_character = deepcopy(base_plan)
        invalid_character["chapters"][0]["povCharacterId"] = "outside-project"
        with self.assertRaisesRegex(WorkspaceError, "本作品角色"):
            save_artifact(self.db_path, self.project_id, "plan", invalid_character, 1)
        for invalid_story_time in (True, -1, 1.5):
            invalid_time = deepcopy(base_plan)
            invalid_time["chapters"][0]["storyTime"] = invalid_story_time
            with self.subTest(story_time=invalid_story_time):
                with self.assertRaisesRegex(WorkspaceError, "非负整数"):
                    save_artifact(
                        self.db_path, self.project_id, "plan", invalid_time, 1
                    )

        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "INSERT INTO characters(id, project_id, name, role, status, color) VALUES (?, ?, ?, ?, ?, ?)",
                ("hero", self.project_id, "主角", "视角人物", "active", "#000000"),
            )
        valid_plan = deepcopy(base_plan)
        valid_plan["chapters"][0].update({"povCharacterId": "hero", "storyTime": 7})
        plan = save_artifact(self.db_path, self.project_id, "plan", valid_plan, 1)
        review = create_review(
            self.db_path, self.project_id, "plan", plan["revisionId"]
        )
        accept_review(self.db_path, self.project_id, review["id"], 1, "accept-pov-plan")
        chapter = materialize_chapter_from_plan(
            self.db_path,
            self.project_id,
            plan["payload"]["chapters"][0]["chapterPlanId"],
            2,
            "materialize-pov-plan",
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            3,
            "thread-plan-projection",
            target={
                "type": "chapter",
                "chapterId": chapter["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "wholeChapter"},
            },
        )
        self.assertEqual(task["target"]["povCharacterId"], "hero")
        self.assertEqual(task["target"]["storyTime"], 7)
        self.assertEqual(task["target"]["readingChapter"], 1)

    def test_candidate_review_requires_latest_task_draft_and_exact_fact_scope(
        self,
    ) -> None:
        initial = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterNumber": 1,
                "body": "起点。",
                "sceneGroups": [{"id": "scene", "body": "起点。"}],
            },
            0,
        )
        task = create_task(
            self.db_path,
            self.project_id,
            "generateChapter",
            0,
            "thread-review-scope",
            target={
                "type": "chapter",
                "chapterId": initial["chapterId"],
                "chapterNumber": 1,
                "scope": {"type": "throughScene", "endSceneOrder": 2},
                "storyTime": 9,
            },
        )
        first = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "第一稿。",
                "sceneOrder": 1,
                "sceneGroups": [{"id": "scene", "body": "第一稿。"}],
            },
            0,
            "candidate",
            task["id"],
            "thread-review-scope",
        )
        latest = save_draft_checkpoint(
            self.db_path,
            self.project_id,
            {
                "chapterId": initial["chapterId"],
                "body": "第二稿。",
                "sceneOrder": 2,
                "sceneGroups": [{"id": "scene", "body": "第二稿。"}],
            },
            0,
            "candidate",
            task["id"],
            "thread-review-scope",
        )
        with self.assertRaisesRegex(WorkspaceError, "最新保存的草稿"):
            create_review(
                self.db_path,
                self.project_id,
                "draft",
                first["revisionId"],
                actor="candidate",
                task_id=task["id"],
                thread_id="thread-review-scope",
            )

        valid_fact = {
            "sceneGroupId": "scene",
            "factKind": "world",
            "predicate": "arrived",
            "value": True,
            "epistemicState": "known",
            "storyTime": 9,
            "readingChapter": 1,
            "sceneOrder": 1,
            "evidence": "第二稿。",
        }
        for field, value in (
            ("storyTime", 8),
            ("readingChapter", 2),
            ("sceneOrder", 0),
            ("sceneOrder", 3),
        ):
            invalid = {**valid_fact, field: value}
            with self.subTest(field=field):
                with self.assertRaisesRegex(WorkspaceError, "候选事实|事实位置"):
                    create_review(
                        self.db_path,
                        self.project_id,
                        "draft",
                        latest["revisionId"],
                        [invalid],
                        actor="candidate",
                        task_id=task["id"],
                        thread_id="thread-review-scope",
                    )
        review = create_review(
            self.db_path,
            self.project_id,
            "draft",
            latest["revisionId"],
            [valid_fact],
            actor="candidate",
            task_id=task["id"],
            thread_id="thread-review-scope",
        )
        self.assertEqual(review["artifactRevisionId"], latest["revisionId"])

    def test_author_context_rejects_non_current_project_revision(self) -> None:
        self.accept_outline()
        with self.assertRaisesRegex(WorkspaceError, "当前作品版本"):
            get_context_snapshot(
                self.db_path,
                self.project_id,
                "planner",
                0,
                actor="author",
            )


if __name__ == "__main__":
    unittest.main()
