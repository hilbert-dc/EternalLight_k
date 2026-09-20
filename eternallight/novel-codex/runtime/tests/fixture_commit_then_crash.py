import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[2]))

from runtime.server import _execute_request


db_path, task_id = sys.argv[1:3]
_execute_request(
    db_path,
    "/api/projects/project-last-lamp/artifacts/brief",
    "POST",
    {
        "taskId": task_id,
        "threadId": "crash-root",
        "expectedRevision": 0,
        "expectedCandidateRevisionId": None,
        "idempotencyKey": "commit-before-host-journal",
        "payload": {
            "premise": "提交后宿主崩溃",
            "promise": "恢复不能重复写入",
            "stakes": "候选必须保持唯一",
            "endingDirection": "从 Runtime 回执恢复",
        },
    },
    "candidate",
    {
        "root_thread_id": "crash-root",
        "caller_thread_id": "crash-root",
        "parent_thread_id": None,
        "turn_id": "turn-crash",
        "tool_call_id": "call-crash",
        "tool_name": "save_brief",
    },
)
os._exit(19)
