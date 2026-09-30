//! Measures authorized source paging, not model latency or semantic retrieval quality.
use std::collections::BTreeMap;
use std::time::Instant;

use novel_codex_story_core::{
    CreateTaskRequest, RevisionId, SqliteStoryRepository, StoryQueryPlan,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

#[path = "support/long_work_fixture.rs"]
mod support;

const CHAPTERS: usize = 512;
const SAMPLES: usize = 30;

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("原文基准.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let began = Instant::now();
    let (project, _) = support::build(&mut repo, CHAPTERS);
    let build_ms = began.elapsed().as_secs_f64() * 1000.0;
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut statement = connection
        .prepare("SELECT content_json FROM story_revisions_v2 WHERE object_kind = 'draft'")
        .unwrap();
    let mut han_characters = 0;
    let mut scene_count = 0;
    let mut chapter_count = 0;
    let mut expected_lengths = BTreeMap::new();
    for content in statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
    {
        let content: Value = serde_json::from_str(&content.unwrap()).unwrap();
        han_characters += content["body"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
            .count();
        scene_count += content["sceneGroups"].as_array().unwrap().len();
        chapter_count += 1;
        expected_lengths.insert(
            content["chapterNumber"].as_i64().unwrap(),
            content["body"].as_str().unwrap().chars().count(),
        );
    }
    let claim_count: i64 = connection.query_row("SELECT SUM(json_array_length(assertions_json)) FROM story_modeling_batches_v2 WHERE status = 'ready'",[],|row|row.get(0)).unwrap();
    assert!(
        chapter_count == CHAPTERS
            && han_characters >= 1_000_000
            && scene_count >= 1000
            && claim_count >= 50_000
    );
    let task = repo
        .create_task(CreateTaskRequest {
            task_id: "passage-benchmark".into(),
            project_id: project,
            thread_id: "benchmark-thread".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"type":"project"}).to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    let query = |cursor: Option<&str>| {
        let plan: StoryQueryPlan = serde_json::from_value(json!({
            "question":"原文中的行为意味着什么？", "byteBudget":9000,"ranking":{"type":"literal"},
            "steps":[{"id":"read","operation":{"type":"passages","cursor":cursor,"maxChars":160,"limit":2}}],
        })).unwrap();
        let report = repo.query_task_plan(&task, &plan, /*now*/ 5).unwrap();
        assert_eq!(report["coverage"]["responseTruncated"], false);
        assert_eq!(report["steps"][0]["coverage"]["pageComplete"], true);
        assert_eq!(report["steps"][0]["coverage"]["fullScanCertified"], false);
        let rows = report["steps"][0]["results"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.len() <= 2);
        report
    };
    let first = measure(|| {
        let report = query(/*cursor*/ None);
        verify(&repo, &report);
        assert_eq!(report["steps"][0]["results"][0]["chapterNumber"], 1);
    });
    let mut cursor = query(/*cursor*/ None)["steps"][0]["nextCursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let sequential = measure(|| {
        let report = query(Some(&cursor));
        verify(&repo, &report);
        cursor = report["steps"][0]["nextCursor"]
            .as_str()
            .unwrap()
            .to_owned();
    });
    // Select an authorized late position; this is not proof earlier pages were consumed.
    let mut late: Value = serde_json::from_str(&cursor).unwrap();
    late["position"] = json!({"chapter":512,"object":"draft-chapter-512","field":0,"start":1500});
    let late = late.to_string();
    let late_page = measure(|| {
        let report = query(Some(&late));
        verify(&repo, &report);
        assert_eq!(report["steps"][0]["results"][0]["chapterNumber"], 512);
        assert_eq!(report["steps"][0]["coverage"]["startsAtBeginning"], false);
    });
    let mut chapters = vec![false; CHAPTERS];
    let mut anchors = 0;
    for (slot, read) in chapters.iter_mut().enumerate() {
        let chapter = slot + 1;
        let mut selected: Value = serde_json::from_str(&late).unwrap();
        selected["position"] = json!({"chapter":chapter,"object":format!("draft-chapter-{chapter}"),"field":0,"start":0});
        let report = query(Some(&selected.to_string()));
        verify(&repo, &report);
        assert_eq!(report["steps"][0]["results"][0]["chapterNumber"], chapter);
        *read = true;
        anchors += report["steps"][0]["results"].as_array().unwrap().len();
    }
    assert!(chapters.iter().all(|read| *read));
    let mut covered = expected_lengths
        .keys()
        .map(|chapter| (*chapter, 0))
        .collect::<BTreeMap<_, usize>>();
    let started = Instant::now();
    let mut cursor = None::<String>;
    let mut full_pages = 0;
    let mut full_rows = 0;
    loop {
        let report = query(cursor.as_deref());
        verify(&repo, &report);
        for row in report["steps"][0]["results"].as_array().unwrap() {
            let chapter = row["chapterNumber"].as_i64().unwrap();
            let end_so_far = covered.get_mut(&chapter).unwrap();
            let anchor = &report["evidence"][row["evidenceRef"].as_str().unwrap()];
            let start = anchor["start"].as_u64().unwrap() as usize;
            let end = anchor["end"].as_u64().unwrap() as usize;
            assert!(
                start <= *end_so_far && end > *end_so_far,
                "no skipped or repeated page may certify character coverage"
            );
            *end_so_far = end;
            full_rows += 1;
        }
        full_pages += 1;
        assert!(
            full_pages <= CHAPTERS * 32,
            "finite fixture traversal must terminate"
        );
        cursor = report["steps"][0]["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(report["steps"][0]["coverage"]["endOfScopeFromCursor"], true);
            break;
        }
    }
    let full_scan_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(covered, expected_lengths);
    let machine = std::process::Command::new("uname")
        .arg("-a")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    println!("{}", serde_json::to_string_pretty(&json!({
        "dataset":"long-work-literal-v1","chapters":chapter_count,"hanCharacters":han_characters,"scenes":scene_count,
        "modelClaims":claim_count,"fixtureBuildMs":build_ms,"samples":SAMPLES,
        "environment":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"machine":machine},
        "firstPage":first,"sequentialPages":sequential,"latePage":late_page,
        "mechanicalFullTraversal":{"elapsedMs":full_scan_ms,"pages":full_pages,"anchorRows":full_rows,"verifiedCharacters":covered.values().sum::<usize>()},
        "sourceCoverage":{"chaptersWithVerifiedPage":chapters.len(),"verifiedAnchorRows":anchors,
            "allCharactersRead":true,"fullScanCertified":false,"semanticQualityVerified":false},
    })).unwrap());
}

fn verify(repo: &SqliteStoryRepository, report: &Value) {
    for row in report["steps"][0]["results"].as_array().unwrap() {
        let revision = RevisionId::parse(row["sourceRevisionId"].as_str().unwrap()).unwrap();
        let content: Value =
            serde_json::from_str(&repo.revision_content(&revision).unwrap().unwrap()).unwrap();
        let text = content
            .pointer(row["sourcePath"].as_str().unwrap())
            .unwrap()
            .as_str()
            .unwrap();
        let evidence = &report["evidence"][row["evidenceRef"].as_str().unwrap()];
        let start = evidence["start"].as_u64().unwrap() as usize;
        let end = evidence["end"].as_u64().unwrap() as usize;
        assert_eq!(
            text.chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
            evidence["quote"].as_str().unwrap()
        );
        assert_eq!(
            row["sourceObjectId"],
            format!("draft-chapter-{}", row["chapterNumber"].as_i64().unwrap())
        );
        assert_eq!(
            row["sourceRevisionId"],
            format!("revision-{}", row["chapterNumber"].as_i64().unwrap())
        );
    }
}

fn measure(mut query: impl FnMut()) -> Value {
    let started = Instant::now();
    query();
    let first_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut times = (0..SAMPLES)
        .map(|_| {
            let started = Instant::now();
            query();
            started.elapsed().as_secs_f64() * 1000.0
        })
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    json!({"firstMs":first_ms,"warmP50Ms":times[SAMPLES/2],"warmP95Ms":times[(SAMPLES*95).div_ceil(100)-1],"warmMaxMs":times[SAMPLES-1]})
}
