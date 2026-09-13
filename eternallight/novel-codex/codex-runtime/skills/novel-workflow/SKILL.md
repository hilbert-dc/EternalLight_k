---
name: novel-workflow
description: Use Novel Runtime candidate tools to turn a story request into bounded context, reviewable artifacts, checkpoints, and fact or plan proposals.
---

# Novel Workflow

Use this skill for any request that may change the story, character knowledge, plot dependency, scene structure, or chapter prose.

- Every tool call must include the exact `taskId`, `projectId`, `threadId`, and `expectedRevision` from the current work package. Never reuse identifiers from another project or thread.
- Start with `read_context`, selecting the task's exact `purpose`, story time, reading position, POV, item budget, and byte budget. Treat returned `evidence` and source revision IDs as the supported material; omissions and stale index watermarks are constraints, not permission to invent facts.
- Follow the order `Context → Brief/Outline/ChapterPlan → Dependency Analysis → Draft Checkpoints → Fact Proposals → Completion` as applicable to the request.
- Keep `Story Canon`, `Narrative IR`, and `Novel Edition` as separate namespaces. Do not encode a Canon change only in prose.
- Use `save_brief`, `save_outline`, and `save_chapter_plans` for structured candidates. Use `save_draft_checkpoint` with `partial`, `sceneComplete`, or `chapterComplete` after recoverable boundaries. A writing task may only save to its bound target chapter.
- Submit new world facts, reader knowledge, character beliefs, and author secrets through `submit_fact_proposals` with their evidence and temporal/reading position. Do not present proposals as confirmed canon.
- Use `propose_plan_revisions` only for the supplied impact IDs; preserve locked objects and report locked conflicts instead of rewriting around them.
- When a change affects an event, character relationship, knowledge state, scene, or chapter, include the affected entity IDs and the reason in the plan.
- Call `complete_task` only after the required candidate is saved. Provide a concise summary; the Runtime verifies persisted output references and rejects a completion claim without the target artifact.
- These are candidate-only tools. There is no accept or undo capability: never attempt to move accepted pointers, confirm facts, apply a draft, or claim the author has accepted a result. Author review is a separate explicit action.
- On a 409 conflict, re-read context at the Runtime's current revision and rebuild only the stale candidate. On validation or lock errors, preserve completed checkpoints and surface the smallest corrective action.
