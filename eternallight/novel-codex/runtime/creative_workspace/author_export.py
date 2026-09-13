"""Author-selected exports from one authoritative, read-only database snapshot."""

import json
import re

from .errors import NotFoundError, WorkspaceError

try:
    from ..storage import connect, now
except ImportError:
    from storage import connect, now


SCOPES = {
    "manuscript": "正式正文",
    "planning": "正式规划（公开指引）",
    "candidates": "候选稿（未采用）",
    "private": "作者私密资料",
}


def export_project(db_path, project_id, payload):
    scopes = payload.get("scopes")
    format_name = payload.get("format")
    if (
        set(payload) != {"scopes", "format"}
        or not isinstance(scopes, list)
        or not scopes
        or any(not isinstance(scope, str) or scope not in SCOPES for scope in scopes)
        or len(scopes) != len(set(scopes))
        or format_name not in ("txt", "markdown")
    ):
        raise WorkspaceError("invalid_export", "请选择导出范围和 TXT／Markdown 格式")
    sections = []
    with connect(db_path) as connection:
        connection.execute("BEGIN")
        project = connection.execute(
            "SELECT p.id, w.revision FROM projects p JOIN project_workspace w ON w.project_id=p.id WHERE p.id=?",
            (project_id,),
        ).fetchone()
        if project is None:
            raise NotFoundError("作品不存在")
        for pointer in connection.execute(
            "SELECT c.number, w.accepted_revision_id, w.candidate_revision_id FROM chapters c JOIN chapter_workspace w ON w.chapter_id=c.id AND w.project_id=c.project_id WHERE c.project_id=? ORDER BY c.number, c.id",
            (project_id,),
        ).fetchall():
            for scope, key in (
                ("manuscript", "accepted_revision_id"),
                ("candidates", "candidate_revision_id"),
            ):
                revision_id = pointer[key]
                if (
                    scope not in scopes
                    or not revision_id
                    or (
                        scope == "candidates"
                        and revision_id == pointer["accepted_revision_id"]
                    )
                ):
                    continue
                row = connection.execute(
                    "SELECT id, body FROM draft_revisions WHERE id=? AND project_id=?",
                    (revision_id, project_id),
                ).fetchone()
                sections.append(
                    (
                        f"{SCOPES[scope]} · 第 {pointer['number']} 章",
                        row["id"],
                        row["body"],
                    )
                )
        for pointer in connection.execute(
            "SELECT * FROM artifact_pointers WHERE project_id=? ORDER BY artifact_kind",
            (project_id,),
        ).fetchall():
            for scope, key in (
                ("planning", "accepted_revision_id"),
                ("candidates", "candidate_revision_id"),
            ):
                revision_id = pointer[key]
                candidate = scope == "candidates"
                if (
                    not revision_id
                    or (
                        candidate
                        and (
                            scope not in scopes
                            or revision_id == pointer["accepted_revision_id"]
                        )
                    )
                    or (
                        not candidate
                        and "planning" not in scopes
                        and "private" not in scopes
                    )
                ):
                    continue
                row = connection.execute(
                    "SELECT payload_json FROM artifact_revisions WHERE id=? AND project_id=?",
                    (revision_id, project_id),
                ).fetchone()
                data = json.loads(row["payload_json"])
                kind = pointer["artifact_kind"]
                if "private" in scopes:
                    sections.append(
                        (
                            f"作者私密规划 · {'候选' if candidate else '正式'} {kind}",
                            revision_id,
                            json.dumps(data, ensure_ascii=False, indent=2),
                        )
                    )
                elif kind in {"outline", "plan"}:
                    public = []
                    for item in data.get(
                        "nodes" if kind == "outline" else "chapters", []
                    ):
                        public.append(
                            {
                                key: item[key]
                                for key in (
                                    "publicTitle",
                                    "writerGuidance",
                                    "publicCommitments",
                                )
                                if key in item
                            }
                        )
                    public = [item for item in public if any(item.values())]
                    sections.append(
                        (
                            f"{SCOPES[scope]} · {kind}",
                            revision_id,
                            json.dumps(public, ensure_ascii=False, indent=2)
                            if public
                            else "尚未填写公开指引。完整内部规划需单独选择作者私密资料。",
                        )
                    )
        if "private" in scopes:
            facts = connection.execute(
                "SELECT * FROM temporal_state_events WHERE project_id=? AND fact_kind='author-secret' AND reverted_by_commit_id IS NULL ORDER BY project_revision, id",
                (project_id,),
            ).fetchall()
            superseded = {
                fact["supersedes_id"] for fact in facts if fact["supersedes_id"]
            }
            for fact in facts:
                data = {
                    key: fact[key]
                    for key in (
                        "subject_id",
                        "predicate",
                        "epistemic_owner_id",
                        "epistemic_state",
                        "story_time",
                        "reading_chapter",
                        "scene_order",
                        "project_revision",
                        "source_draft_revision_id",
                        "supersedes_id",
                        "evidence",
                    )
                }
                data["value"] = json.loads(fact["value_json"])
                sections.append(
                    (
                        "作者私密事实（历史，已被替代）"
                        if fact["id"] in superseded
                        else "作者私密事实",
                        fact["id"],
                        json.dumps(data, ensure_ascii=False, indent=2),
                    )
                )
    if not sections:
        raise WorkspaceError(
            "export_empty", "所选范围尚无已保存内容，请先保存或采用后再导出"
        )
    exported_at = now()
    heading = "# " if format_name == "markdown" else ""
    content = [
        f"{heading}作品导出",
        f"作品 ID：{project_id}\n作品版本：{project['revision']}\n导出时间：{exported_at}\n范围：{'、'.join(SCOPES[scope] for scope in scopes)}",
    ]
    for title, revision_id, body in sections:
        content.extend([f"{heading}{heading}{title}", f"内容版本：{revision_id}", body])
    extension = "md" if format_name == "markdown" else "txt"
    safe_id = re.sub(r"[^a-zA-Z0-9_-]", "_", project_id)
    return {
        "fileName": f"{safe_id}-r{project['revision']}.{extension}",
        "mimeType": "text/markdown;charset=utf-8"
        if format_name == "markdown"
        else "text/plain;charset=utf-8",
        "content": "\n\n".join(content) + "\n",
        "projectRevision": project["revision"],
        "exportedAt": exported_at,
        "scopes": scopes,
    }
