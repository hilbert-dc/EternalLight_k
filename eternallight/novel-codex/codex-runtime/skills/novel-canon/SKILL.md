---
name: novel-canon
description: Maintain Novel Codex Story Canon as explicit, reviewable facts instead of burying story state in prose.
---

# Novel Canon

Use this skill whenever a request creates or changes a character, relationship, event, location, world rule, or plot thread.

- Treat `CreativeProject` metadata as the project boundary and `canon` as the source of truth.
- Separate established facts, current character knowledge, hypotheses, and unresolved questions. Never silently promote a hypothesis to Canon.
- Before proposing a change, use the current project state and cite the affected entity IDs when they exist.
- Prefer the host's structured Runtime operation for writes. If no write operation is available, return a concise proposed patch with entity type, ID, fields, and rationale; do not claim that it was saved.
- Keep prose and conversation history out of Canon unless the user explicitly confirms the fact.
- After a Canon change, identify which Scenes, Beats, passages, or Story Tests may be affected.
