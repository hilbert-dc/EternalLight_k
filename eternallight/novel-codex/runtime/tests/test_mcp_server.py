import json
import os
import unittest
from unittest.mock import patch

from runtime.mcp_server import CandidateRuntimeClient, RuntimeResponse, handle_message


class CandidateRuntimeClientTests(unittest.TestCase):
    def test_rejects_model_supplied_thread_identity_without_trusted_match(self) -> None:
        client = type(
            "Client",
            (),
            {"call_tool": lambda *_args: self.fail("must not call runtime")},
        )()
        response = handle_message(
            {
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "read_context",
                    "_meta": {
                        "callerThreadId": "trusted-thread",
                        "rootThreadId": "trusted-thread",
                        "sessionId": "trusted-thread",
                    },
                    "arguments": {"threadId": "forged-thread"},
                },
            },
            client,
        )
        self.assertTrue(response["result"]["isError"])
        self.assertIn(
            "trusted execution identity", response["result"]["content"][0]["text"]
        )

    def test_subagent_is_restricted_to_context_reading_by_trusted_metadata(
        self,
    ) -> None:
        client = type(
            "Client",
            (),
            {"call_tool": lambda *_args: self.fail("must not call runtime")},
        )()
        response = handle_message(
            {
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/call",
                "params": {
                    "name": "save_outline",
                    "_meta": {
                        "callerThreadId": "child-thread",
                        "rootThreadId": "root-thread",
                        "sessionId": "root-thread",
                        "parentThreadId": "parent-thread",
                        "agentRole": "story_architect",
                    },
                    "arguments": {"threadId": "root-thread"},
                },
            },
            client,
        )
        self.assertTrue(response["result"]["isError"])
        self.assertIn(
            "restricted to read_context", response["result"]["content"][0]["text"]
        )

    def test_tool_result_has_one_copy_and_bounded_model_items(self) -> None:
        payload = {"context": "界" * 3_000}

        response = handle_message(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "read_context",
                    "_meta": {
                        "callerThreadId": "thread-1",
                        "rootThreadId": "thread-1",
                        "sessionId": "thread-1",
                    },
                    "arguments": {"threadId": "thread-1"},
                },
            },
            type(
                "Client",
                (),
                {
                    "call_tool": lambda _self, _name, _arguments, _meta: (
                        RuntimeResponse(200, payload)
                    )
                },
            )(),
        )

        result = response["result"]
        self.assertNotIn("structuredContent", result)
        self.assertGreater(len(result["content"]), 1)
        self.assertTrue(
            all(
                len(item["text"].encode("utf-8")) <= 9_000 for item in result["content"]
            )
        )
        self.assertEqual(
            "".join(item["text"] for item in result["content"]),
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
        )

    def test_candidate_write_result_does_not_echo_generated_prose(self) -> None:
        payload = {
            "chapterId": "chapter-1",
            "revisionId": "draft-1",
            "version": 2,
            "body": "不能重新进入模型上下文" * 2_000,
            "sceneGroups": [{"id": "scene-1", "body": "重复正文" * 2_000}],
            "checkpointId": "checkpoint-1",
            "completeness": "chapterComplete",
        }
        response = handle_message(
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "save_draft_checkpoint",
                    "_meta": {
                        "callerThreadId": "thread-1",
                        "rootThreadId": "thread-1",
                        "sessionId": "thread-1",
                    },
                    "arguments": {"threadId": "thread-1"},
                },
            },
            type(
                "Client",
                (),
                {
                    "call_tool": lambda _self, _name, _arguments, _meta: (
                        RuntimeResponse(201, payload)
                    )
                },
            )(),
        )

        text = "".join(item["text"] for item in response["result"]["content"])
        self.assertEqual(
            json.loads(text),
            {
                "chapterId": "chapter-1",
                "revisionId": "draft-1",
                "version": 2,
                "checkpointId": "checkpoint-1",
                "completeness": "chapterComplete",
            },
        )

    def test_advisory_exposes_only_context_reading(self) -> None:
        with patch.dict(
            os.environ,
            {"NOVEL_CODEX_OPERATION": "advisory"},
            clear=False,
        ):
            response = handle_message(
                {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}},
                object(),
            )

        self.assertEqual(
            [tool["name"] for tool in response["result"]["tools"]],
            ["read_context"],
        )

    def test_writer_context_can_omit_unset_story_time(self) -> None:
        with patch.dict(
            os.environ,
            {
                "NOVEL_CODEX_OPERATION": "generateChapter",
                "NOVEL_CODEX_RUNTIME_URL": "http://127.0.0.1:43123",
                "NOVEL_CODEX_CANDIDATE_TOKEN": "candidate-token",
            },
            clear=False,
        ):
            client = CandidateRuntimeClient()
            response = RuntimeResponse(200, {"projectRevision": 7})
            with patch.object(client, "_request", return_value=response) as request:
                result = client.call_tool(
                    "read_context",
                    {
                        "taskId": "task-1",
                        "projectId": "project-1",
                        "threadId": "thread-1",
                        "expectedRevision": 7,
                        "purpose": "writer",
                        "readingChapter": 3,
                        "sceneOrder": 2,
                        "budget": 24,
                        "byteBudget": 9_000,
                    },
                )

        self.assertEqual(result, response)
        method, path = request.call_args.args
        self.assertEqual(method, "GET")
        self.assertNotIn("storyTime", path)
        self.assertIn("projectRevision=7", path)

        with self.assertRaisesRegex(ValueError, "byteBudget must be between"):
            CandidateRuntimeClient._validate_arguments(
                "read_context",
                {
                    "taskId": "task-1",
                    "projectId": "project-1",
                    "threadId": "thread-1",
                    "expectedRevision": 7,
                    "purpose": "writer",
                    "readingChapter": 3,
                    "sceneOrder": 2,
                    "budget": 24,
                    "byteBudget": 60_001,
                },
            )


if __name__ == "__main__":
    unittest.main()
