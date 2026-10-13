use super::*;
use crate::{BranchId, CreateTaskRequest, ProjectId, ProjectRecord, SnapshotId};

struct Fixture {
    repo: SqliteStoryRepository,
    task: TaskRecord,
}

impl Fixture {
    fn new() -> Self {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        let project = ProjectId::parse("author-proof").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "完整作者要求".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        let task = repo
            .create_task(Self::request(&project, "root-task"))
            .unwrap();
        Self { repo, task }
    }

    fn request(project: &ProjectId, id: &str) -> CreateTaskRequest {
        CreateTaskRequest {
            task_id: id.into(),
            project_id: project.clone(),
            thread_id: format!("thread-{id}"),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"type":"project"}).to_string(),
            // JSON escaping crosses the transport page without exceeding the
            // existing author-history byte bound. This is isolated transport data.
            input_manifest_json: json!({"authorIntent":format!("保留结局并核对人物动机。{}", "\u{0001}".repeat(SOURCE_CONTENT_PAGE_BYTES / 6 + 600))})
                .to_string(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        }
    }

    fn source(&self, position: Value) -> Value {
        let mut context =
            json!({"target":{"position":position},"coverage":{"complete":true,"missingScope":[]}});
        self.repo
            .project_task_sources(&self.task, &mut context)
            .unwrap();
        context["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["sectionPath"] == "/authorRequirementsSource")
            .unwrap()
            .clone()
    }

    fn two_policy_sources(&self) -> (Value, Value) {
        let normal = self.source(json!({"chapterNumber":7}));
        // revision_outcome_api has no target in its report context, hence position=null.
        let report = self.source(Value::Null);
        assert_ne!(normal["sourceId"], report["sourceId"]);
        assert_eq!(normal["contentHash"], report["contentHash"]);
        let first = self
            .repo
            .source_record(&self.task, normal["sourceId"].as_str().unwrap())
            .unwrap();
        let second = self
            .repo
            .source_record(&self.task, report["sourceId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            first, second,
            "Both policies contain the exact same current author payload"
        );
        (normal, report)
    }

    fn raw(&self, source: &Value, mut cursor: Option<String>, reader: &str) {
        loop {
            let page = self
                .repo
                .task_source_page_for_reader(
                    &self.task,
                    source["sourceId"].as_str().unwrap(),
                    cursor.as_deref(),
                    reader,
                )
                .unwrap();
            assert_eq!(page["contentHash"], source["contentHash"]);
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
    }

    fn parsed(&self, source: &Value, mut cursor: Option<String>, reader: &str) {
        loop {
            let page = self
                .repo
                .task_source_content_for_reader(
                    &self.task,
                    source["sourceId"].as_str().unwrap(),
                    cursor.as_deref(),
                    reader,
                )
                .unwrap();
            assert_eq!(page["contentHash"], source["contentHash"]);
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
    }

    fn require(&self) -> Result<(), RepositoryError> {
        require_author_requirements_read(&self.repo.connection, &self.task)
    }

    fn unread(&self) {
        assert!(
            matches!(self.require(), Err(RepositoryError::InvalidContent(message)) if message.contains("作者原要求尚未由主任务完整读取"))
        );
    }

    fn not_selected_by_original_query(&self, a: Value, b: Value) -> Value {
        let chosen: String = self.repo.connection.query_row(
            "SELECT source_id,length(CAST(payload_json AS BLOB)) FROM story_task_source_items_v2 WHERE task_id=?1 AND section_path='/authorRequirementsSource' AND content_hash=?2",
            params![self.task.task_id, a["contentHash"].as_str().unwrap()], |row| row.get(0),
        ).unwrap();
        let other = if a["sourceId"] == chosen { b } else { a };
        assert_ne!(other["sourceId"], chosen);
        other
    }
}

#[test]
fn raw_complete_current_author_source_is_valid_when_another_policy_is_unread() {
    let f = Fixture::new();
    let (normal, report) = f.two_policy_sources();
    let read = f.not_selected_by_original_query(normal, report);
    f.raw(&read, None, &f.task.thread_id);
    f.require()
        .expect("A complete current root read cannot be hidden by another unread policy source");
}

#[test]
fn parsed_complete_current_author_source_is_valid_when_another_policy_is_unread() {
    let f = Fixture::new();
    let (normal, report) = f.two_policy_sources();
    let read = f.not_selected_by_original_query(normal, report);
    f.parsed(&read, None, &f.task.thread_id);
    f.require().expect(
        "A complete current root parsed read cannot be hidden by another unread policy source",
    );
}

#[test]
fn missing_or_index_only_author_sources_do_not_prove_content_reading() {
    let f = Fixture::new();
    assert!(
        matches!(f.require(), Err(RepositoryError::InvalidContent(message)) if message.contains("先 read_context"))
    );
    let (a, b) = f.two_policy_sources();
    for source in [a, b] {
        let mut cursor = None;
        loop {
            let page = f
                .repo
                .task_source_index(
                    &f.task,
                    source["sourceId"].as_str().unwrap(),
                    cursor.as_deref(),
                )
                .unwrap();
            assert_eq!(page["contentRead"], false);
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
    }
    f.unread();
}

#[test]
fn child_raw_and_parsed_reads_of_both_policies_do_not_fill_root_proof() {
    let f = Fixture::new();
    let (a, b) = f.two_policy_sources();
    for source in [a, b] {
        f.raw(&source, None, "independent-child");
        f.parsed(&source, None, "independent-child");
    }
    f.unread();
}

#[test]
fn raw_prefix_and_suffix_from_different_policy_sources_cannot_be_combined() {
    let f = Fixture::new();
    let (a, b) = f.two_policy_sources();
    let prefix = f
        .repo
        .task_source_page(&f.task, a["sourceId"].as_str().unwrap(), None)
        .unwrap();
    assert!(prefix["nextCursor"].is_string());
    let child_prefix = f
        .repo
        .task_source_page_for_reader(&f.task, b["sourceId"].as_str().unwrap(), None, "child")
        .unwrap();
    assert_eq!(prefix["byteRange"], child_prefix["byteRange"]);
    f.raw(
        &b,
        Some(child_prefix["nextCursor"].as_str().unwrap().into()),
        &f.task.thread_id,
    );
    f.unread();
}

#[test]
fn parsed_prefix_and_suffix_from_different_policy_sources_cannot_be_combined() {
    let f = Fixture::new();
    let (a, b) = f.two_policy_sources();
    let prefix = f
        .repo
        .task_source_content(&f.task, a["sourceId"].as_str().unwrap(), None)
        .unwrap();
    assert!(prefix["nextCursor"].is_string());
    let child_prefix = f
        .repo
        .task_source_content_for_reader(&f.task, b["sourceId"].as_str().unwrap(), None, "child")
        .unwrap();
    assert_eq!(prefix["coverage"], child_prefix["coverage"]);
    f.parsed(
        &b,
        Some(child_prefix["nextCursor"].as_str().unwrap().into()),
        &f.task.thread_id,
    );
    f.unread();
}

#[test]
fn raw_or_parsed_suffix_only_does_not_prove_full_root_reading() {
    for parsed in [false, true] {
        let f = Fixture::new();
        let source = f.source(json!({"chapterNumber":7}));
        let first = if parsed {
            f.repo
                .task_source_content_for_reader(
                    &f.task,
                    source["sourceId"].as_str().unwrap(),
                    None,
                    "child",
                )
                .unwrap()
        } else {
            f.repo
                .task_source_page_for_reader(
                    &f.task,
                    source["sourceId"].as_str().unwrap(),
                    None,
                    "child",
                )
                .unwrap()
        };
        let suffix = Some(first["nextCursor"].as_str().unwrap().into());
        if parsed {
            f.parsed(&source, suffix, &f.task.thread_id);
        } else {
            f.raw(&source, suffix, &f.task.thread_id);
        }
        f.unread();
    }
}

#[test]
fn complete_read_of_another_task_with_identical_author_text_is_not_root_proof() {
    let mut f = Fixture::new();
    f.two_policy_sources();
    let other = f
        .repo
        .create_task(Fixture::request(&f.task.project_id, "other-task"))
        .unwrap();
    let mut context =
        json!({"target":{"position":null},"coverage":{"complete":true,"missingScope":[]}});
    f.repo.project_task_sources(&other, &mut context).unwrap();
    let source = context["authorRequirementsSource"]["sourceId"]
        .as_str()
        .unwrap();
    let mut cursor = None;
    loop {
        let page = f
            .repo
            .task_source_page(&other, source, cursor.as_deref())
            .unwrap();
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    f.unread();
}

#[test]
fn delivered_new_author_requirements_cannot_reuse_a_complete_old_hash_read() {
    let mut f = Fixture::new();
    let source = f.source(json!({"chapterNumber":7}));
    f.raw(&source, None, &f.task.thread_id);
    f.require().unwrap();
    f.repo.task_intent_store().append_task_intent(
            &f.task.project_id,
            &f.task.task_id,
            0,
            "还须保留港口藏信原文",
            2,
        )
        .unwrap();
    f.repo.task_intent_store().deliver_task_intent(&f.task.project_id, &f.task.task_id, 1, 3)
        .unwrap();
    f.task = f
        .repo
        .task(&f.task.project_id, &f.task.task_id)
        .unwrap()
        .unwrap();
    assert!(
        matches!(f.require(), Err(RepositoryError::InvalidContent(message)) if message.contains("先 read_context"))
    );
    // Updating intent at the SAME position must invalidate the source projection.
    let current = f.source(json!({"chapterNumber":7}));
    assert_ne!(current["contentHash"], source["contentHash"]);
    f.unread();
    f.raw(&current, None, &f.task.thread_id);
    f.require().unwrap();
}

#[test]
fn current_draft_context_requires_complete_root_read() {
    let f = Fixture::new();
    let content = json!({"chapterId":"chapter", "body":"正文".repeat(30000), "sceneGroups":[]});
    f.repo.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,parent_revision_id,content_digest,content_json) VALUES('revision',?1,'draft-chapter','draft',NULL,?2,?3)",params![f.task.project_id.as_str(),digest(&content),content.to_string()]).unwrap();
    let mut context = json!({"relevantText":[{"objectId":"draft-chapter","sourceRevisionId":"revision","content":content}],"coverage":{"complete":true,"missingScope":[]}});
    f.repo.project_task_sources(&f.task, &mut context).unwrap();
    let source = context["taskSources"]["entries"].as_array().unwrap().iter().find(|s|s["sectionPath"]=="/relevantText").unwrap();
    let require = || require_current_revision_read(&f.repo.connection, &f.task, "revision");
    assert!(require().is_err());
    f.raw(source, None, "child");
    f.parsed(source, None, "child");
    assert!(require().is_err(), "child reads must not certify the root");
    let prefix = f.repo.task_source_page(&f.task, source["sourceId"].as_str().unwrap(), None).unwrap();
    assert!(prefix["nextCursor"].is_string());
    assert!(require().is_err(), "a root prefix is insufficient");
    f.raw(source, Some(prefix["nextCursor"].as_str().unwrap().into()), &f.task.thread_id);
    require().expect("the complete root read of this exact revision is evidence");
}

#[test]
fn visible_root_readiness_uses_the_completion_proof() {
    let f=Fixture::new();
    let context = || {
        let mut c=json!({"target":{"position":null},"coverage":{"complete":true,"missingScope":[]}});
        f.repo.project_task_sources(&f.task,&mut c).unwrap(); c
    };
    let before=context();
    assert_eq!(before["authorRequirementsSource"]["rootReadComplete"], false);
    let source=f.source(Value::Null);
    f.parsed(&source,None,"child");
    assert_eq!(context()["authorRequirementsSource"]["rootReadComplete"], false);
    f.parsed(&source,None,&f.task.thread_id);
    assert_eq!(context()["authorRequirementsSource"]["rootReadComplete"], true);
    f.require().unwrap();
}

use crate::{ProjectRevisionRepository, TaskSourcesRepository, TasksRepository};
