import copy
import json
import os
import unittest
from unittest.mock import patch

from runtime.mcp_server import CandidateRuntimeClient, RuntimeResponse, handle_message


class PlanningReviewMcpTests(unittest.TestCase):
    def setUp(self) -> None:
        environment = patch.dict(
            os.environ,
            {
                "NOVEL_CODEX_OPERATION": "reviewPlanningSeat",
                "NOVEL_CODEX_RUNTIME_URL": "http://127.0.0.1:43123",
                "NOVEL_CODEX_CANDIDATE_TOKEN": "candidate-token",
                "NOVEL_CODEX_TASK_CAPABILITY": "review-seat-capability-123456789",
            },
        )
        environment.start()
        self.addCleanup(environment.stop)
        self.client = CandidateRuntimeClient()
        self.arguments = {
            "taskId": "task-seat-1",
            "projectId": "project-1",
            "threadId": "thread-seat-1",
            "expectedRevision": 7,
            "runId": "planning-review-1",
            "report": {
                "summary": "本次仅检查第二卷的规划。",
                "strengths": [
                    {"quote": "她拒绝交出旧物", "reason": "行动保留了主动性。"}
                ],
                "findings": [
                    {
                        "quote": "父亲没有回应",
                        "observation": "此处未看到回应。",
                        "impact": "可能削弱试探带来的紧张感。",
                        "suggestion": "暂不建议改写，需作者判断。",
                    }
                ],
                "coverage": [
                    {
                        "checkId": "check-1",
                        "status": "unknown",
                        "reason": "需要后续情节确认。",
                    }
                ],
            },
        }

    def test_seat_can_only_read_report_and_complete(self) -> None:
        response = handle_message(
            {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}, self.client
        )
        self.assertEqual(
            [tool["name"] for tool in response["result"]["tools"]],
            ["read_context", "submit_planning_review_report", "complete_task"],
        )
        for forbidden in (
            "save_brief",
            "save_outline",
            "save_chapter_plans",
            "save_draft_checkpoint",
            "submit_fact_proposals",
        ):
            with (
                self.subTest(tool=forbidden),
                patch.object(self.client, "_request") as request,
            ):
                with self.assertRaisesRegex(ValueError, "not allowed"):
                    self.client.call_tool(forbidden, {})
                request.assert_not_called()

    def test_other_operations_cannot_submit_a_seat_report(self) -> None:
        for operation in (
            "advisory",
            "generateBrief",
            "generateOutline",
            "generatePlan",
            "generateChapter",
        ):
            with (
                self.subTest(operation=operation),
                patch.dict(os.environ, {"NOVEL_CODEX_OPERATION": operation}),
                patch.object(self.client, "_request") as request,
            ):
                with self.assertRaisesRegex(ValueError, "not allowed"):
                    self.client.call_tool(
                        "submit_planning_review_report", self.arguments
                    )
                request.assert_not_called()

    def test_report_routes_bound_ids_and_encodes_path_segments(self) -> None:
        arguments = {**self.arguments, "projectId": "project/1", "runId": "run/卷二"}
        response = RuntimeResponse(
            201,
            {
                "id": "report-1",
                "role": "structure",
                "status": "partial",
            },
        )
        with patch.object(self.client, "_request", return_value=response) as request:
            self.assertEqual(
                self.client.call_tool("submit_planning_review_report", arguments),
                response,
            )
        request.assert_called_once_with(
            "POST",
            "/api/projects/project%2F1/planning-reviews/run%2F%E5%8D%B7%E4%BA%8C/reports",
            {
                key: value
                for key, value in arguments.items()
                if key not in {"projectId", "runId"}
            },
        )

    def test_review_reads_planner_context_without_inventing_a_reading_position(
        self,
    ) -> None:
        arguments = {
            key: value
            for key, value in self.arguments.items()
            if key not in {"runId", "report"}
        } | {"purpose": "planner", "budget": 24, "byteBudget": 60_000}
        with patch.object(
            self.client, "_request", return_value=RuntimeResponse(200, {})
        ) as request:
            self.client.call_tool("read_context", arguments)
        method, path = request.call_args.args
        self.assertEqual(method, "GET")
        self.assertIn("projectRevision=7", path)
        self.assertIn("taskId=task-seat-1", path)
        self.assertIn("purpose=planner", path)
        self.assertNotIn("readingChapter", path)

    def test_no_findings_and_unchecked_coverage_reach_runtime_without_a_pass_claim(
        self,
    ) -> None:
        arguments = copy.deepcopy(self.arguments)
        arguments["report"]["strengths"] = []
        arguments["report"]["findings"] = []
        arguments["report"]["coverage"][0]["status"] = "notChecked"
        with patch.object(
            self.client, "_request", return_value=RuntimeResponse(201, {})
        ) as request:
            self.client.call_tool("submit_planning_review_report", arguments)
        self.assertEqual(request.call_args.args[2]["report"], arguments["report"])

    def test_finding_can_report_a_symptom_without_inventing_a_fix(self) -> None:
        arguments = copy.deepcopy(self.arguments)
        arguments["report"]["findings"][0]["suggestion"] = ""
        with patch.object(
            self.client, "_request", return_value=RuntimeResponse(201, {})
        ) as request:
            self.client.call_tool("submit_planning_review_report", arguments)
        self.assertEqual(request.call_args.args[2]["report"], arguments["report"])

    def test_invalid_reports_fail_before_transport(self) -> None:
        invalid = [
            {"runId": "x" * 161},
            {"report": []},
            {"report": {"summary": "只有摘要"}},
            {"report": {**self.arguments["report"], "summary": "x" * 2001}},
            {"report": {**self.arguments["report"], "strengths": ["优点"] * 4}},
            {"report": {**self.arguments["report"], "findings": [{}] * 7}},
            {"report": {**self.arguments["report"], "coverage": "已全部审完"}},
            {"report": {**self.arguments["report"], "coverage": [{}] * 121}},
            {"report": {**self.arguments["report"], "role": "other-seat"}},
        ]
        for change in invalid:
            with (
                self.subTest(change=change),
                patch.object(self.client, "_request") as request,
            ):
                with self.assertRaises(ValueError):
                    self.client.call_tool(
                        "submit_planning_review_report", {**self.arguments, **change}
                    )
                request.assert_not_called()

    def test_nested_report_fields_are_bounded_and_do_not_accept_authority_claims(
        self,
    ) -> None:
        invalid = (
            ("strengths", "quote", " " * 3),
            ("strengths", "quote", "界" * 1201),
            ("strengths", "reason", "界" * 1601),
            ("findings", "observation", {"unsupported": "claim"}),
            ("findings", "impact", ""),
            ("findings", "suggestion", None),
            ("findings", "suggestion", "含有\x00空字节"),
            ("findings", "authorDecision", "accepted"),
            ("coverage", "checkId", "x" * 201),
            ("coverage", "status", "passed"),
            ("coverage", "reason", "界" * 1601),
        )
        for field, key, value in invalid:
            arguments = copy.deepcopy(self.arguments)
            arguments["report"][field][0][key] = value
            with (
                self.subTest(field=field, key=key),
                patch.object(self.client, "_request") as request,
            ):
                with self.assertRaises(ValueError):
                    self.client.call_tool("submit_planning_review_report", arguments)
                request.assert_not_called()

    def test_success_only_returns_a_receipt_without_report_or_sibling_content(
        self,
    ) -> None:
        payload = {
            "id": "report-1",
            "runId": "planning-review-1",
            "role": "structure",
            "status": "partial",
            "report": copy.deepcopy(self.arguments["report"]),
            "siblingReports": [{"summary": "OTHER_SEAT_SECRET"}],
            "snapshot": {"body": "SOURCE_SECRET"},
        }
        with patch.object(
            self.client, "_request", return_value=RuntimeResponse(201, payload)
        ):
            response = handle_message(
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "submit_planning_review_report",
                        "_meta": {
                            "callerThreadId": "thread-seat-1",
                            "rootThreadId": "thread-seat-1",
                            "sessionId": "thread-seat-1",
                        },
                        "arguments": self.arguments,
                    },
                },
                self.client,
            )
        self.assertEqual(
            response["result"],
            {
                "content": [
                    {
                        "type": "text",
                        "text": json.dumps(
                            {
                                "reportId": "report-1",
                                "role": "structure",
                                "coverageStatus": "partial",
                            },
                            separators=(",", ":"),
                        ),
                    }
                ],
            },
        )

    def test_invalid_evidence_retains_runtime_error_without_echoing_material(
        self,
    ) -> None:
        payload = {
            "error": {
                "code": "invalid_review_evidence",
                "message": "引文与冻结材料不匹配",
                "details": {"sourceText": "SOURCE_SECRET"},
            }
        }
        with patch.object(
            self.client, "_request", return_value=RuntimeResponse(422, payload)
        ):
            response = handle_message(
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "submit_planning_review_report",
                        "_meta": {
                            "callerThreadId": "thread-seat-1",
                            "rootThreadId": "thread-seat-1",
                            "sessionId": "thread-seat-1",
                        },
                        "arguments": self.arguments,
                    },
                },
                self.client,
            )
        self.assertEqual(
            response["result"],
            {
                "isError": True,
                "content": [
                    {
                        "type": "text",
                        "text": json.dumps(
                            {
                                "error": {
                                    "code": "invalid_review_evidence",
                                    "message": "引文与冻结材料不匹配",
                                }
                            },
                            ensure_ascii=False,
                            separators=(",", ":"),
                        ),
                    }
                ],
            },
        )


if __name__ == "__main__":
    unittest.main()
