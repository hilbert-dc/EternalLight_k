---
name: narrative-ir
description: Turn novel direction into stable Scene and Beat structure with anchors back to Story Canon.
---

# Narrative IR

Use this skill whenever a request outlines, revises, searches, or writes a scene.

- Model the request as `Scene → Beat[]`, with location, time, participants, state changes, and Canon constraints explicit where known.
- A Beat is an observable narrative transition, not a vague intention. Keep positions contiguous from 1.
- Reuse existing Canon IDs and Scene IDs when the project state provides them. Flag missing references instead of inventing them.
- Novel prose is a rendering of the IR. Keep Scene/Beat anchors attached to passages so a prose edit remains traceable.
- When a requested change affects continuity, explain the impacted Canon entities and passages before drafting replacement prose.
- Return structure first and prose second when both are requested; make the boundary visible to the user.
