import json
import re
import sqlite3
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Iterator
from uuid import uuid4

try:
    from .creative_workspace.schema import create_workspace_schema
except ImportError:
    from creative_workspace.schema import create_workspace_schema


DEFAULT_PROJECT_ID = "project-last-lamp"


def now() -> str:
    return datetime.now(UTC).isoformat(timespec="seconds")


SCHEMA = """
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  logline TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS characters (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  role TEXT NOT NULL,
  background TEXT NOT NULL DEFAULT '',
  personality TEXT NOT NULL DEFAULT '',
  knowledge_boundary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  color TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS relationships (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  outcome TEXT NOT NULL,
  occurred_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS locations (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  constraints TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS world_rules (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  statement TEXT NOT NULL,
  severity TEXT NOT NULL DEFAULT 'canon'
);
CREATE TABLE IF NOT EXISTS plot_threads (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  state TEXT NOT NULL,
  tone TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS scenes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  location TEXT NOT NULL,
  time TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS beats (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  description TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS scene_participants (
  scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
  character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  PRIMARY KEY(scene_id, character_id)
);
CREATE TABLE IF NOT EXISTS scene_state_changes (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
  description TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS scene_canon_constraints (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
  rule_id TEXT NOT NULL REFERENCES world_rules(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS chapters (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS passages (
  id TEXT PRIMARY KEY,
  chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  body TEXT NOT NULL,
  scene_id TEXT REFERENCES scenes(id),
  beat_ids TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS chapter_diffs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
  from_text TEXT NOT NULL,
  to_text TEXT NOT NULL,
  summary TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS character_knowledge (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  fact_key TEXT NOT NULL,
  state TEXT NOT NULL,
  chapter_number INTEGER NOT NULL,
  evidence TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS story_dependencies (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL,
  source_id TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  relation TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_characters_project ON characters(project_id);
CREATE INDEX IF NOT EXISTS idx_scenes_project ON scenes(project_id);
CREATE INDEX IF NOT EXISTS idx_chapters_project ON chapters(project_id);
CREATE INDEX IF NOT EXISTS idx_diffs_chapter ON chapter_diffs(chapter_id);
CREATE INDEX IF NOT EXISTS idx_character_knowledge_project ON character_knowledge(project_id);
CREATE INDEX IF NOT EXISTS idx_story_dependencies_project ON story_dependencies(project_id);
CREATE TABLE IF NOT EXISTS search_documents (
  document_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_search_documents_project ON search_documents(project_id);
"""


@contextmanager
def connect(db_path: str | Path) -> Iterator[sqlite3.Connection]:
    path = Path(db_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute("PRAGMA busy_timeout = 5000")
    connection.execute("PRAGMA journal_mode = WAL")
    try:
        yield connection
        connection.commit()
    except Exception:
        connection.rollback()
        raise
    finally:
        connection.close()


def initialize(db_path: str | Path) -> None:
    with connect(db_path) as connection:
        connection.executescript(SCHEMA)
        create_workspace_schema(connection)
        seeded = False
        if connection.execute("SELECT 1 FROM projects LIMIT 1").fetchone() is None:
            seed(connection)
            initialize_seed_workspace(connection)
            seeded = True
        ensure_search_index(connection)
        if (
            seeded
            or connection.execute("SELECT COUNT(*) FROM search_documents").fetchone()[0]
            == 0
        ):
            refresh_search_index_connection(connection)


def initialize_project_workspace(
    connection: sqlite3.Connection, project_id: str
) -> None:
    connection.execute(
        "INSERT INTO project_workspace(project_id) VALUES (?)", (project_id,)
    )
    connection.executemany(
        "INSERT INTO artifact_pointers(project_id, artifact_kind) VALUES (?, ?)",
        ((project_id, kind) for kind in ("brief", "outline", "plan")),
    )


def initialize_seed_workspace(connection: sqlite3.Connection) -> None:
    initialize_project_workspace(connection, DEFAULT_PROJECT_ID)
    passage = connection.execute(
        "SELECT body, scene_id FROM passages WHERE chapter_id = 'chapter-01' ORDER BY position LIMIT 1"
    ).fetchone()
    body = passage["body"] if passage else ""
    revision_id = "initial-draft-chapter-01"
    scene_groups = [
        {
            "id": "initial-group-chapter-01",
            "sceneId": passage["scene_id"] if passage else None,
            "body": body,
            "locked": False,
            "baseAcceptedRevisionId": None,
            "candidateDraftRevisionId": revision_id,
        }
    ]
    connection.execute(
        "INSERT INTO chapter_workspace(project_id, chapter_id, candidate_revision_id, accepted_revision_id) VALUES (?, 'chapter-01', ?, ?)",
        (DEFAULT_PROJECT_ID, revision_id, revision_id),
    )
    connection.execute(
        "INSERT INTO draft_revisions(id, project_id, chapter_id, version, parent_revision_id, source_plan_refs_json, base_project_revision, body, scene_groups_json, source, created_at) VALUES (?, ?, 'chapter-01', 1, NULL, ?, 0, ?, ?, 'author', ?)",
        (
            revision_id,
            DEFAULT_PROJECT_ID,
            json.dumps(
                {"lineageStatus": "current", "localOnly": True}, ensure_ascii=False
            ),
            body,
            json.dumps(scene_groups, ensure_ascii=False),
            now(),
        ),
    )
    knowledge = connection.execute(
        "SELECT * FROM character_knowledge WHERE project_id = ?",
        (DEFAULT_PROJECT_ID,),
    ).fetchall()
    for item in knowledge:
        connection.execute(
            "INSERT INTO temporal_state_events(id, project_id, project_revision, fact_kind, subject_id, predicate, value_json, epistemic_owner_id, epistemic_state, story_time, reading_chapter, scene_order, source_draft_revision_id, evidence, created_at) VALUES (?, ?, 0, 'character', ?, ?, 'true', ?, ?, NULL, ?, 0, ?, ?, ?)",
            (
                f"initial-state-{item['id']}",
                DEFAULT_PROJECT_ID,
                item["character_id"],
                item["fact_key"],
                item["character_id"],
                item["state"],
                item["chapter_number"],
                revision_id,
                item["evidence"],
                now(),
            ),
        )


def recover_search_indexes(db_path: str | Path) -> None:
    """Rebuild indexes left stale by an interrupted formal-state commit."""
    initialize(db_path)
    with connect(db_path) as connection:
        stale_projects = connection.execute(
            "SELECT project_id, index_target_revision FROM project_workspace WHERE indexed_revision < index_target_revision"
        ).fetchall()
        for stale in stale_projects:
            refresh_search_index_connection(connection, stale["project_id"])
            connection.execute(
                "UPDATE project_workspace SET indexed_revision = ? WHERE project_id = ?",
                (stale["index_target_revision"], stale["project_id"]),
            )


def seed(connection: sqlite3.Connection) -> None:
    timestamp = now()
    project_id = DEFAULT_PROJECT_ID
    connection.execute(
        "INSERT INTO projects(id, title, logline, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        (
            project_id,
            "最后一盏灯",
            "在一座正在失去记忆的港城里，一个不再相信预言的抄写员，必须决定要不要替未来留下证词。",
            timestamp,
            timestamp,
        ),
    )
    characters = [
        (
            "linxi",
            "林夕",
            "抄写员 / 视角人物",
            "",
            "",
            "不知道张衡参与密函交换",
            "正在怀疑",
            "#e6a24a",
        ),
        ("zhangheng", "张衡", "灯塔守望者", "", "", "", "秘密行动", "#8da7ff"),
        ("mo", "墨鸦", "无名信使", "", "", "", "未出场", "#bf91e7"),
    ]
    connection.executemany(
        "INSERT INTO characters(id, project_id, name, role, background, personality, knowledge_boundary, status, color) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [(item[0], project_id, *item[1:]) for item in characters],
    )
    connection.executemany(
        "INSERT INTO relationships(id, project_id, source_id, target_id, kind, state) VALUES (?, ?, ?, ?, ?, ?)",
        [
            (
                "rel-linxi-zhangheng",
                project_id,
                "linxi",
                "zhangheng",
                "隐瞒与合作",
                "未揭示",
            )
        ],
    )
    connection.executemany(
        "INSERT INTO events(id, project_id, title, outcome, occurred_at) VALUES (?, ?, ?, ?, ?)",
        [
            (
                "event-memory-loss",
                project_id,
                "港城第一次遗忘",
                "一名居民失去与海有关的人名。",
                "白塔熄灭之后",
            )
        ],
    )
    connection.executemany(
        "INSERT INTO locations(id, project_id, name, constraints) VALUES (?, ?, ?, ?)",
        [
            ("old-harbor", project_id, "旧港档案馆", "潮汐记录必须按港务局规则保存"),
            ("white-tower", project_id, "白塔", "灯每晚只亮四十七秒"),
        ],
    )
    rules = [
        ("rule-lamp", project_id, "白塔的灯每晚只亮四十七秒。", "canon"),
        ("rule-memory", project_id, "港城居民会逐渐忘记与海有关的人名。", "canon"),
    ]
    connection.executemany(
        "INSERT INTO world_rules(id, project_id, statement, severity) VALUES (?, ?, ?, ?)",
        rules,
    )
    threads = [
        ("thread-1", project_id, "谁在篡改潮汐记录？", "进行中", "amber"),
        ("thread-2", project_id, "张衡究竟看见了什么？", "待揭示", "blue"),
        ("thread-3", project_id, "林夕失去的那一天", "伏笔", "violet"),
    ]
    connection.executemany(
        "INSERT INTO plot_threads(id, project_id, title, state, tone) VALUES (?, ?, ?, ?, ?)",
        threads,
    )
    scenes = [
        (
            "scene-184",
            project_id,
            "潮汐档案室",
            "旧港档案馆",
            "深夜",
            "林夕发现记录正在被改写。",
            "draft",
        ),
        (
            "scene-185",
            project_id,
            "没有影子的客人",
            "南码头",
            "凌晨",
            "一个没有影子的信使出现。",
            "review",
        ),
        (
            "scene-186",
            project_id,
            "灯塔下的交换",
            "白塔",
            "黎明前",
            "张衡用一份密函交换灯塔的秘密。",
            "ready",
        ),
    ]
    connection.executemany(
        "INSERT INTO scenes(id, project_id, title, location, time, summary, status) VALUES (?, ?, ?, ?, ?, ?, ?)",
        scenes,
    )
    beats = [
        ("beat-184-1", "scene-184", 1, "林夕抬头发现白塔没有亮。"),
        ("beat-184-2", "scene-184", 2, "墨水在纸上重新变湿。"),
        ("beat-184-3", "scene-184", 3, "张衡留下无法解释的印记。"),
        ("beat-184-4", "scene-184", 4, "林夕决定保留这份证词。"),
        ("beat-185-1", "scene-185", 1, "无影客人从潮雾中走出。"),
        ("beat-185-2", "scene-185", 2, "他交出一枚旧港钥匙。"),
        ("beat-185-3", "scene-185", 3, "远处传来四十七秒的钟声。"),
        ("beat-186-1", "scene-186", 1, "张衡点亮白塔。"),
        ("beat-186-2", "scene-186", 2, "密函被放入灯室。"),
        ("beat-186-3", "scene-186", 3, "林夕看见自己的名字。"),
        ("beat-186-4", "scene-186", 4, "灯光熄灭。"),
        ("beat-186-5", "scene-186", 5, "港城忘记了一个人。"),
    ]
    connection.executemany(
        "INSERT INTO beats(id, scene_id, position, description) VALUES (?, ?, ?, ?)",
        beats,
    )
    connection.executemany(
        "INSERT INTO scene_participants(scene_id, character_id) VALUES (?, ?)",
        [
            ("scene-184", "linxi"),
            ("scene-184", "zhangheng"),
            ("scene-185", "linxi"),
            ("scene-185", "mo"),
            ("scene-186", "linxi"),
            ("scene-186", "zhangheng"),
        ],
    )
    connection.executemany(
        "INSERT INTO scene_canon_constraints(id, scene_id, rule_id) VALUES (?, ?, ?)",
        [
            ("constraint-184-lamp", "scene-184", "rule-lamp"),
            ("constraint-186-lamp", "scene-186", "rule-lamp"),
        ],
    )
    chapter_body = "\n\n".join(
        [
            "海水退到旧港的石阶以下时，林夕才从档案室里抬起头。",
            "她已经连续三晚在同一页纸上抄写同一个名字。墨水干了又湿，纸页却始终留不住那三个字的笔画，仿佛有人在她落笔之前，先一步把它们从这个世界上擦掉。",
            "窗外的白塔没有亮。按照港务局的记录，它应该在子夜后亮四十七秒。",
        ]
    )
    connection.execute(
        "INSERT INTO chapters(id, project_id, number, title, status, summary, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        (
            "chapter-01",
            project_id,
            1,
            "灯亮之前",
            "draft",
            "林夕在档案室发现潮汐记录被改写。",
            timestamp,
        ),
    )
    connection.execute(
        "INSERT INTO passages(id, chapter_id, position, body, scene_id, beat_ids) VALUES (?, ?, ?, ?, ?, ?)",
        (
            "passage-01",
            "chapter-01",
            1,
            chapter_body,
            "scene-184",
            json.dumps(["beat-184-1", "beat-184-2", "beat-184-3", "beat-184-4"]),
        ),
    )
    connection.executemany(
        "INSERT INTO character_knowledge(id, project_id, character_id, fact_key, state, chapter_number, evidence) VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            (
                "knowledge-linxi-traitor",
                project_id,
                "linxi",
                "zhangheng-is-traitor",
                "unknown",
                47,
                "林夕还不知道张衡参与密函交换。",
            ),
            (
                "knowledge-linxi-lamp",
                project_id,
                "linxi",
                "white-tower-lamp-rule",
                "known",
                1,
                "林夕熟悉港务局关于白塔亮灯的记录。",
            ),
        ],
    )
    connection.executemany(
        "INSERT INTO story_dependencies(id, project_id, source_kind, source_id, target_kind, target_id, relation, reason) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        [
            (
                "dependency-event-thread-1",
                project_id,
                "event",
                "event-memory-loss",
                "plotThread",
                "thread-1",
                "advances",
                "港城第一次遗忘推动篡改记录的调查。",
            ),
            (
                "dependency-scene-chapter-01",
                project_id,
                "scene",
                "scene-184",
                "chapter",
                "chapter-01",
                "rendered-as",
                "潮汐档案室场景由第一章正文呈现。",
            ),
            (
                "dependency-character-relationship",
                project_id,
                "character",
                "linxi",
                "relationship",
                "rel-linxi-zhangheng",
                "has-state",
                "林夕与张衡的关系状态影响她的怀疑线。",
            ),
            (
                "dependency-relationship-thread-2",
                project_id,
                "relationship",
                "rel-linxi-zhangheng",
                "plotThread",
                "thread-2",
                "advances",
                "林夕与张衡的隐瞒关系推动张衡真相线。",
            ),
        ],
    )
    connection.commit()


def rows(
    connection: sqlite3.Connection, query: str, parameters: tuple[Any, ...] = ()
) -> list[dict[str, Any]]:
    return [dict(row) for row in connection.execute(query, parameters).fetchall()]


def ensure_search_index(connection: sqlite3.Connection) -> bool:
    try:
        connection.execute(
            """CREATE VIRTUAL TABLE IF NOT EXISTS search_documents_fts USING fts5(
                project_id UNINDEXED,
                entity_id UNINDEXED,
                kind UNINDEXED,
                title,
                body,
                tokenize='unicode61'
            )"""
        )
        return True
    except sqlite3.OperationalError:
        return False


def search_documents(
    connection: sqlite3.Connection, project_id: str
) -> list[dict[str, str]]:
    documents: list[dict[str, str]] = []
    project = connection.execute(
        "SELECT id, title, logline FROM projects WHERE id = ?", (project_id,)
    ).fetchone()
    if project is None:
        return documents

    def add(entity_id: str, kind: str, title: str, body: str) -> None:
        documents.append(
            {
                "document_id": f"{kind}:{entity_id}",
                "project_id": project_id,
                "entity_id": entity_id,
                "kind": kind,
                "title": title,
                "body": body,
            }
        )

    add(project["id"], "project", project["title"], project["logline"])
    for character in rows(
        connection, "SELECT * FROM characters WHERE project_id = ?", (project_id,)
    ):
        add(
            character["id"],
            "character",
            character["name"],
            "\n".join(
                filter(
                    None,
                    [
                        character["role"],
                        character["background"],
                        character["personality"],
                        character["knowledge_boundary"],
                        character["status"],
                    ],
                )
            ),
        )
    for relationship in rows(
        connection,
        """
        SELECT r.id, r.source_id, r.target_id, r.kind, r.state, source.name AS source_name, target.name AS target_name
        FROM relationships r
        LEFT JOIN characters source ON source.id = r.source_id
        LEFT JOIN characters target ON target.id = r.target_id
        WHERE r.project_id = ?
    """,
        (project_id,),
    ):
        title = f"{relationship['source_name'] or relationship['source_id']} ↔ {relationship['target_name'] or relationship['target_id']}"
        add(
            relationship["id"],
            "relationship",
            title,
            f"{relationship['kind']}\n{relationship['state']}",
        )
    for event in rows(
        connection, "SELECT * FROM events WHERE project_id = ?", (project_id,)
    ):
        add(
            event["id"],
            "event",
            event["title"],
            f"{event['outcome']}\n{event['occurred_at']}",
        )
    for location in rows(
        connection, "SELECT * FROM locations WHERE project_id = ?", (project_id,)
    ):
        add(location["id"], "location", location["name"], location["constraints"])
    for rule in rows(
        connection, "SELECT * FROM world_rules WHERE project_id = ?", (project_id,)
    ):
        add(rule["id"], "worldRule", rule["statement"], rule["severity"])
    for thread in rows(
        connection, "SELECT * FROM plot_threads WHERE project_id = ?", (project_id,)
    ):
        add(
            thread["id"],
            "plotThread",
            thread["title"],
            f"{thread['state']}\n{thread['tone']}",
        )
    for scene in rows(
        connection, "SELECT * FROM scenes WHERE project_id = ?", (project_id,)
    ):
        beats = rows(
            connection,
            "SELECT description FROM beats WHERE scene_id = ? ORDER BY position",
            (scene["id"],),
        )
        add(
            scene["id"],
            "scene",
            scene["title"],
            "\n".join(
                filter(
                    None,
                    [
                        scene["location"],
                        scene["time"],
                        scene["summary"],
                        *(beat["description"] for beat in beats),
                    ],
                )
            ),
        )
    for chapter in rows(
        connection, "SELECT * FROM chapters WHERE project_id = ?", (project_id,)
    ):
        passages = rows(
            connection,
            "SELECT body FROM passages WHERE chapter_id = ? ORDER BY position",
            (chapter["id"],),
        )
        add(
            chapter["id"],
            "chapter",
            chapter["title"],
            "\n".join(
                filter(
                    None,
                    [chapter["summary"], *(passage["body"] for passage in passages)],
                )
            ),
        )
    return documents


def refresh_search_index_connection(
    connection: sqlite3.Connection, project_id: str | None = None
) -> None:
    projects = (
        [project_id]
        if project_id
        else [
            row["id"]
            for row in connection.execute("SELECT id FROM projects").fetchall()
        ]
    )
    fts_enabled = ensure_search_index(connection)
    for current_project_id in projects:
        connection.execute(
            "DELETE FROM search_documents WHERE project_id = ?", (current_project_id,)
        )
        if fts_enabled:
            connection.execute(
                "DELETE FROM search_documents_fts WHERE project_id = ?",
                (current_project_id,),
            )
        documents = search_documents(connection, current_project_id)
        connection.executemany(
            "INSERT INTO search_documents(document_id, project_id, entity_id, kind, title, body) VALUES (?, ?, ?, ?, ?, ?)",
            [
                (
                    document["document_id"],
                    document["project_id"],
                    document["entity_id"],
                    document["kind"],
                    document["title"],
                    document["body"],
                )
                for document in documents
            ],
        )
        if fts_enabled:
            connection.executemany(
                "INSERT INTO search_documents_fts(project_id, entity_id, kind, title, body) VALUES (?, ?, ?, ?, ?)",
                [
                    (
                        document["project_id"],
                        document["entity_id"],
                        document["kind"],
                        document["title"],
                        document["body"],
                    )
                    for document in documents
                ],
            )


def refresh_search_index(db_path: str | Path, project_id: str | None = None) -> None:
    with connect(db_path) as connection:
        refresh_search_index_connection(connection, project_id)


def excerpt(text: str, query: str, limit: int = 180) -> str:
    normalized_query = query.strip()
    if not text:
        return ""
    index = (
        text.casefold().find(normalized_query.casefold()) if normalized_query else -1
    )
    if index < 0:
        return text[:limit]
    start = max(0, index - limit // 3)
    return ("…" if start else "") + text[start : start + limit]


def project_state(
    db_path: str | Path, project_id: str = DEFAULT_PROJECT_ID
) -> dict[str, Any]:
    initialize(db_path)
    with connect(db_path) as connection:
        project = connection.execute(
            "SELECT * FROM projects WHERE id = ?", (project_id,)
        ).fetchone()
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        characters = rows(
            connection,
            "SELECT * FROM characters WHERE project_id = ? ORDER BY name",
            (project_id,),
        )
        relationships = rows(
            connection,
            "SELECT id, source_id AS sourceId, target_id AS targetId, kind, state FROM relationships WHERE project_id = ? ORDER BY id",
            (project_id,),
        )
        events = rows(
            connection,
            "SELECT id, title, outcome, occurred_at AS occurredAt FROM events WHERE project_id = ? ORDER BY occurred_at, id",
            (project_id,),
        )
        locations = rows(
            connection,
            "SELECT id, name, constraints FROM locations WHERE project_id = ? ORDER BY name",
            (project_id,),
        )
        threads = rows(
            connection,
            "SELECT * FROM plot_threads WHERE project_id = ? ORDER BY id",
            (project_id,),
        )
        rules = rows(
            connection,
            "SELECT * FROM world_rules WHERE project_id = ? ORDER BY id",
            (project_id,),
        )
        scenes = rows(
            connection,
            "SELECT * FROM scenes WHERE project_id = ? ORDER BY id",
            (project_id,),
        )
        for scene in scenes:
            scene["beats"] = rows(
                connection,
                "SELECT id, position, description FROM beats WHERE scene_id = ? ORDER BY position",
                (scene["id"],),
            )
            scene["participants"] = rows(
                connection,
                "SELECT c.id, c.name FROM characters c JOIN scene_participants p ON p.character_id = c.id WHERE p.scene_id = ? ORDER BY c.name",
                (scene["id"],),
            )
            scene["stateChanges"] = rows(
                connection,
                "SELECT id, description FROM scene_state_changes WHERE scene_id = ? ORDER BY id",
                (scene["id"],),
            )
            scene["canonConstraints"] = rows(
                connection,
                "SELECT w.id, w.statement FROM world_rules w JOIN scene_canon_constraints c ON c.rule_id = w.id WHERE c.scene_id = ? ORDER BY w.id",
                (scene["id"],),
            )
        chapters = rows(
            connection,
            "SELECT * FROM chapters WHERE project_id = ? ORDER BY number",
            (project_id,),
        )
        for chapter in chapters:
            passages = rows(
                connection,
                "SELECT * FROM passages WHERE chapter_id = ? ORDER BY position",
                (chapter["id"],),
            )
            for passage in passages:
                passage["beatIds"] = json.loads(passage.pop("beat_ids"))
            chapter["passages"] = passages
        canon = {
            "characters": characters,
            "relationships": relationships,
            "events": events,
            "locations": locations,
            "worldRules": rules,
            "plotThreads": threads,
            "notes": [rule["statement"] for rule in rules],
        }
        anchors = [
            {
                "chapterId": chapter["id"],
                "passageId": passage["id"],
                "sceneId": passage.get("scene_id"),
                "beatIds": passage["beatIds"],
            }
            for chapter in chapters
            for passage in chapter["passages"]
        ]
        return {
            "project": dict(project),
            "canon": canon,
            "narrativeIr": {"version": "1.0", "scenes": scenes, "anchors": anchors},
            "novelEdition": {"chapters": chapters},
        }


def list_projects(db_path: str | Path) -> list[dict[str, Any]]:
    initialize(db_path)
    with connect(db_path) as connection:
        return rows(
            connection,
            "SELECT id, title, logline, created_at, updated_at FROM projects ORDER BY updated_at DESC, created_at DESC",
        )


def create_project(
    db_path: str | Path, title: str, logline: str = ""
) -> dict[str, Any]:
    normalized_title = title.strip()
    normalized_logline = logline.strip()
    if not normalized_title:
        raise ValueError("Project title cannot be empty")
    if len(normalized_title) > 120:
        raise ValueError("Project title is too long")
    if len(normalized_logline) > 500:
        raise ValueError("Project logline is too long")

    initialize(db_path)
    project_id = f"project-{uuid4().hex}"
    timestamp = now()
    with connect(db_path) as connection:
        connection.execute(
            "INSERT INTO projects(id, title, logline, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            (project_id, normalized_title, normalized_logline, timestamp, timestamp),
        )
        initialize_project_workspace(connection, project_id)
    refresh_search_index(db_path, project_id)
    return project_state(db_path, project_id)


def chapter_diffs(
    db_path: str | Path, project_id: str, chapter_id: str
) -> list[dict[str, Any]]:
    initialize(db_path)
    with connect(db_path) as connection:
        chapter = connection.execute(
            "SELECT 1 FROM chapters WHERE id = ? AND project_id = ?",
            (chapter_id, project_id),
        ).fetchone()
        if chapter is None:
            raise KeyError("Chapter not found in project")
        diffs = rows(
            connection,
            "SELECT d.id, d.chapter_id, d.from_text AS fromText, d.to_text AS toText, d.summary, d.created_at AS createdAt FROM chapter_diffs d JOIN chapters c ON c.id = d.chapter_id WHERE d.chapter_id = ? AND c.project_id = ? ORDER BY d.id DESC",
            (chapter_id, project_id),
        )
        for diff in diffs:
            diff["addedChars"] = max(0, len(diff["toText"]) - len(diff["fromText"]))
            diff["removedChars"] = max(0, len(diff["fromText"]) - len(diff["toText"]))
        return diffs


def search_project(
    db_path: str | Path,
    project_id: str,
    query: str,
    limit: int = 20,
    kind: str | None = None,
) -> list[dict[str, Any]]:
    initialize(db_path)
    normalized_query = query.strip()
    if not normalized_query:
        return []
    bounded_limit = min(max(limit, 1), 50)
    with connect(db_path) as connection:
        matches: list[dict[str, Any]] = []
        seen: set[str] = set()
        fts_enabled = ensure_search_index(connection)
        tokens = re.findall(r"[\w\u4e00-\u9fff]+", normalized_query, flags=re.UNICODE)[
            :8
        ]
        if fts_enabled and tokens:
            fts_query = " OR ".join(
                f'"{token.replace(chr(34), "")}"' for token in tokens
            )
            try:
                fts_rows = connection.execute(
                    f"""SELECT entity_id, kind, title, body
                    FROM search_documents_fts
                    WHERE project_id = ? AND search_documents_fts MATCH ?{(" AND kind = ?" if kind else "")}
                    ORDER BY bm25(search_documents_fts)
                    LIMIT ?""",
                    (project_id, fts_query, *((kind,) if kind else ()), bounded_limit),
                ).fetchall()
                for row in fts_rows:
                    key = f"{row['kind']}:{row['entity_id']}"
                    seen.add(key)
                    matches.append(
                        {
                            "id": row["entity_id"],
                            "title": row["title"],
                            "kind": row["kind"],
                            "snippet": excerpt(row["body"], normalized_query),
                            "source": row["kind"],
                        }
                    )
            except sqlite3.OperationalError:
                pass
        like_term = f"%{normalized_query}%"
        where_kind = " AND kind = ?" if kind else ""
        parameters: tuple[Any, ...] = (
            project_id,
            like_term,
            like_term,
            like_term,
            *((kind,) if kind else ()),
        )
        fallback_rows = connection.execute(
            f"""SELECT entity_id, kind, title, body
            FROM search_documents
            WHERE project_id = ? AND (title LIKE ? OR body LIKE ? OR body LIKE ?){where_kind}
            ORDER BY kind, title
            LIMIT ?""",
            (*parameters, bounded_limit),
        ).fetchall()
        for row in fallback_rows:
            key = f"{row['kind']}:{row['entity_id']}"
            if key in seen:
                continue
            matches.append(
                {
                    "id": row["entity_id"],
                    "title": row["title"],
                    "kind": row["kind"],
                    "snippet": excerpt(row["body"], normalized_query),
                    "source": row["kind"],
                }
            )
            if len(matches) >= bounded_limit:
                break
        return matches[:bounded_limit]


def run_story_tests(db_path: str | Path, project_id: str) -> dict[str, Any]:
    state = project_state(db_path, project_id)
    canon = state["canon"]
    scenes = state["narrativeIr"]["scenes"]
    chapters = state["novelEdition"]["chapters"]
    character_ids = {character["id"] for character in canon["characters"]}
    rule_ids = {rule["id"] for rule in canon["worldRules"]}
    scene_ids = {scene["id"] for scene in scenes}
    beat_ids_by_scene = {
        scene["id"]: {beat["id"] for beat in scene["beats"]} for scene in scenes
    }
    checks: list[dict[str, Any]] = []

    def check(
        check_id: str, label: str, passed: bool, details: str, severity: str = "error"
    ) -> None:
        checks.append(
            {
                "id": check_id,
                "label": label,
                "passed": passed,
                "severity": severity,
                "details": details,
            }
        )

    check(
        "canon-has-foundation",
        "Canon 至少包含角色与世界规则",
        bool(canon["characters"] and canon["worldRules"]),
        "需要至少一个角色和一条世界规则，Agent 才有可验证的事实基础。",
        "warning",
    )
    check(
        "scenes-have-beats",
        "每个场景至少包含一个 Beat",
        all(scene["beats"] for scene in scenes),
        "空场景无法作为 Narrative IR 的可执行叙事单元。",
    )
    check(
        "beats-are-contiguous",
        "每个场景的 Beat 顺序连续",
        all(
            [beat["position"] for beat in scene["beats"]]
            == list(range(1, len(scene["beats"]) + 1))
            for scene in scenes
        ),
        "Beat position 必须从 1 开始连续，避免正文锚点漂移。",
    )
    check(
        "scene-participants-valid",
        "场景参与者都引用已存在角色",
        all(
            participant["id"] in character_ids
            for scene in scenes
            for participant in scene["participants"]
        ),
        "请先补齐角色或修正 Scene participant 引用。",
    )
    check(
        "scene-constraints-valid",
        "场景约束都引用已存在规则",
        all(
            constraint["id"] in rule_ids
            for scene in scenes
            for constraint in scene["canonConstraints"]
        ),
        "请先补齐 WorldRule 或修正 Scene canon constraint 引用。",
    )
    check(
        "passages-have-scene-anchor",
        "每个正文段落都有有效 Scene Anchor",
        all(
            passage.get("scene_id") in scene_ids
            for chapter in chapters
            for passage in chapter["passages"]
        ),
        "正文必须能回溯到 Narrative IR 的 Scene。",
    )
    check(
        "passage-beats-valid",
        "正文 Beat Anchor 属于对应场景",
        all(
            set(passage["beatIds"]).issubset(
                beat_ids_by_scene.get(passage.get("scene_id"), set())
            )
            for chapter in chapters
            for passage in chapter["passages"]
        ),
        "正文引用的 Beat 必须属于该段落的 Scene。",
    )
    return {
        "passed": all(
            check["passed"] or check["severity"] == "warning" for check in checks
        ),
        "checks": checks,
        "projectId": project_id,
    }


def add_canon_note(
    db_path: str | Path, project_id: str, statement: str
) -> dict[str, Any]:
    if not statement.strip():
        raise ValueError("Canon note cannot be empty")
    initialize(db_path)
    with connect(db_path) as connection:
        note_id = f"rule-{abs(hash((project_id, statement))) % 10**12}"
        connection.execute(
            "INSERT OR IGNORE INTO world_rules(id, project_id, statement, severity) VALUES (?, ?, ?, 'canon')",
            (note_id, project_id, statement.strip()),
        )
        connection.execute(
            "UPDATE projects SET updated_at = ? WHERE id = ?", (now(), project_id)
        )
        connection.commit()
    refresh_search_index(db_path, project_id)
    return project_state(db_path, project_id)
