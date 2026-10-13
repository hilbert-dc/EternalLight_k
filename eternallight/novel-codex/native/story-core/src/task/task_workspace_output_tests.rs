//! Actual Native saves, their immutable workspace evidence, and reader scope.
use super::*;
use crate::*;

struct Fixture {
    repo: SqliteStoryRepository,
    project: ProjectId,
    serial: i64,
}
impl Fixture {
    fn new() -> Self {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        let project = ProjectId::parse("workspace-output-project").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "产物回读".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        Self {
            repo,
            project,
            serial: 0,
        }
    }
    fn task(&mut self, id: &str, op: &str, target: Value) -> TaskRecord {
        self.repo
            .create_task(CreateTaskRequest {
                task_id: id.into(),
                project_id: self.project.clone(),
                thread_id: "root".into(),
                operation: op.into(),
                input_revision: 0,
                model: None,
                target_json: target.to_string(),
                input_manifest_json: json!({"authorIntent":"保存授权候选供比较，不采用、不发布。"})
                    .to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            })
            .unwrap()
    }
    fn budgeted_root(&mut self, operation: &str, target: Value, intent: &str) -> TaskRecord {
        let snapshot: String = self.repo.connection.query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [self.project.as_str()], |r| r.get(0)).unwrap();
        let digest = self.repo.project_source_watermarks(&self.project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        self.repo
            .create_budgeted_root_task(
                CreateTaskRequest {
                    task_id: "native-mints-root".into(),
                    project_id: self.project.clone(),
                    thread_id: "root".into(),
                    operation: operation.into(),
                    input_revision: 0,
                    model: None,
                    target_json: target.to_string(),
                    input_manifest_json: json!({"authorIntent":intent}).to_string(),
                    capability_hash: "a".repeat(64),
                    capability_expires_at: i64::MAX,
                },
                &InitialWorkBudget {
                    authorization_id: "saved-output-authorization".into(),
                    author_message_id: "saved-output-author-message".into(),
                    expected_base_snapshot_id: snapshot,
                    expected_source_digest: digest,
                    max_requests: Some(16),
                    max_tokens: Some(10000),
                    deadline_at: None,
                },
                1,
            )
            .unwrap()
    }
    fn budgeted_writer(&mut self, n: i64) -> TaskRecord {
        self.budgeted_root(
            "generateChapter",
            writer_target(n),
            "保存授权候选供比较，不采用、不发布。",
        )
    }
    fn settle(&mut self, work: &str, task: &TaskRecord) {
        let stored: String = self
            .repo
            .connection
            .query_row(
                "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
                [&task.task_id],
                |r| r.get(0),
            )
            .unwrap();
        let grant: TaskProviderBudget = serde_json::from_str(&stored).unwrap();
        self.repo.charge_work_provider_usage(&self.project, work, &task.task_id,
            &json!({"runId":task.task_id,"budgetId":grant.budget_id,"status":task.status,
                "limits":{"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"maxDurationMs":null},
                "usage":{"requests":0,"tokens":0,"unknownTokens":0,"overrun":false}})).unwrap();
    }
    fn current(&self, id: &str) -> TaskRecord {
        self.repo.task(&self.project, id).unwrap().unwrap()
    }
    fn authorize(&mut self, work: &str) {
        let w = self.repo.work_task(&self.project, work).unwrap();
        let digest = self.repo.project_source_watermarks(&self.project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        self.repo
            .authorize_work_budget(
                &self.project,
                work,
                &WorkBudgetAuthorization {
                    expected_version: w.version,
                    expected_project_revision: 0,
                    expected_source_digest: digest,
                    max_requests: Some(16),
                    max_tokens: Some(10000),
                    deadline_at: None,
                },
                1,
            )
            .unwrap();
    }
    fn decision(&mut self, work: &str, id: &str) -> TaskRecord {
        let w = self.repo.work_task(&self.project, work).unwrap();
        let digest = self.repo.project_source_watermarks(&self.project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        let result = self
            .repo
            .prepare_work_decision(
                &self.project,
                work,
                &WorkDecisionRequest {
                    expected_work_version: w.version,
                    expected_revision: 0,
                    expected_source_digest: digest,
                },
            )
            .unwrap();
        let step = result
            .step
            .unwrap_or_else(|| panic!("decision did not attach: {:?}", result.stop_reason));
        let mut target = step.target;
        target["resumeStepId"] = json!(step.id);
        self.task(id, "advisory", target)
    }
    fn save(&mut self, object: &str, kind: &str, content: Value, task: Option<&str>) -> String {
        self.serial += 1;
        let revision = format!("saved-{}", self.serial);
        let snapshot:String=self.repo.connection.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[self.project.as_str()],|r|r.get(0)).unwrap();
        let previous:Option<String>=self.repo.connection.query_row("SELECT candidate_revision_id FROM story_object_heads_v2 WHERE project_id=?1 AND object_id=?2",params![self.project.as_str(),object],|r|r.get(0)).optional().unwrap().flatten();
        let projection = (kind == "draft").then(|| ChapterProjection {
            chapter_id: object.strip_prefix("draft-").unwrap().into(),
            number: content["chapterNumber"].as_i64().unwrap(),
            title: content["title"].as_str().unwrap().into(),
        });
        self.repo
            .save_candidate(SaveCandidateRequest {
                project_id: self.project.clone(),
                branch_id: BranchId::parse("main").unwrap(),
                object_id: ObjectId::parse(object).unwrap(),
                object_kind: kind.into(),
                revision_id: RevisionId::parse(&revision).unwrap(),
                commit_id: CommitId::parse(format!("commit-{}", self.serial)).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("key-{}", self.serial)).unwrap(),
                request_digest: "request".into(),
                content_digest: "content".into(),
                content_json: content.to_string(),
                expected_project_revision: 0,
                expected_snapshot_id: SnapshotId::parse(snapshot).unwrap(),
                next_snapshot_id: SnapshotId::parse(format!("snapshot-{}", self.serial)).unwrap(),
                expected_candidate_revision_id: previous.map(|r| RevisionId::parse(r).unwrap()),
                chapter_projection: projection,
                task_output: task.map(|id| TaskOutputBinding {
                    task_id: id.into(),
                    thread_id: "root".into(),
                    output_kind: kind.into(),
                    completeness: (kind == "draft").then(|| "chapterComplete".into()),
                }),
                linked_impact_ids: vec![],
            })
            .unwrap();
        revision
    }
    fn directory(&self, t: &TaskRecord) -> String {
        let mut context = json!({"coverage":{"complete":false,"missingScope":[]}});
        self.repo.project_task_sources(t, &mut context).unwrap();
        context["taskWorkspace"]["sourceId"]
            .as_str()
            .unwrap()
            .into()
    }
    fn items(&self, t: &TaskRecord) -> Vec<Value> {
        let source = self.directory(t);
        let mut cursor = None;
        let mut all = Vec::new();
        loop {
            let p = self
                .repo
                .task_workspace_page(t, Some(&source), cursor.as_deref(), None, None, None, None)
                .unwrap();
            assert!(p.to_string().len() <= 6500);
            assert_eq!(p["contentRead"], false);
            all.extend(p["items"].as_array().unwrap().clone());
            cursor = p["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                return all;
            }
        }
    }
    fn raw(&self, t: &TaskRecord, item: &Value) -> (String, Value) {
        let source = item["sourceId"].as_str().unwrap();
        let mut cursor = None;
        let mut raw = String::new();
        let mut digest = None;
        let mut path = None;
        loop {
            let p = self
                .repo
                .task_source_page(t, source, cursor.as_deref())
                .unwrap();
            assert!(p.to_string().len() <= 6500);
            let part = p["payloadPart"].as_str().unwrap();
            assert!(part.len() <= 2000);
            assert_eq!(p["byteRange"]["start"], raw.len());
            raw.push_str(part);
            assert_eq!(p["byteRange"]["end"], raw.len());
            if let Some(h) = &digest {
                assert_eq!(h, &p["contentHash"])
            } else {
                digest = Some(p["contentHash"].clone())
            }
            if let Some(h) = &path {
                assert_eq!(h, &p["sectionPath"])
            } else {
                path = Some(p["sectionPath"].clone())
            }
            cursor = p["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                assert_eq!(p["byteRange"]["total"], raw.len());
                break;
            }
        }
        assert_eq!(
            digest.unwrap(),
            format!("{:x}", Sha256::digest(raw.as_bytes()))
        );
        (raw.clone(), serde_json::from_str(&raw).unwrap())
    }
}
fn draft(n: i64, title: &str, body: &str) -> Value {
    json!({"chapterNumber":n,"title":title,"body":body,"sceneGroups":[{"id":"one","body":body,"locked":false}]})
}
fn writer_target(n: i64) -> Value {
    json!({"type":"chapter","chapterId":format!("chapter-{n}"),"chapterNumber":n,"readingChapter":n,"sceneOrder":1,"scope":{"type":"wholeChapter"}})
}
fn saved(items: &[Value], revision: &str) -> Value {
    items
        .iter()
        .find(|i| i["purpose"] == "savedCheckpoint" && i["revisionId"] == revision)
        .unwrap()
        .clone()
}

#[test]
fn accepted_input_without_working_and_multiple_own_saves_have_separate_immutable_sources() {
    let mut f = Fixture::new();
    let original = f.save("draft-chapter-1", "draft", draft(1, "原题", "原稿"), None);
    // Seed author acceptance independently of candidate production; no invented TaskOutput.
    f.repo.connection.execute("UPDATE story_object_heads_v2 SET accepted_revision_id=?1 WHERE object_id='draft-chapter-1'",[&original]).unwrap();
    let writer = f.task("writer", "generateChapter", writer_target(1));
    let originals = f.items(&writer);
    let before = originals
        .iter()
        .find(|i| i["authority"] == "frozenWriterInput")
        .unwrap()
        .clone();
    let (_, old) = f.raw(&writer, &before);
    assert_eq!(old["content"]["sceneGroups"][0]["body"], "原稿");
    let first = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "第一保存题", &"保存一🙂\"\\\n".repeat(900)),
        Some("writer"),
    );
    let w = f.current("writer");
    let direct = f.repo.task_saved_draft_source(&w, &first).unwrap();
    assert_eq!(direct["contentRead"], false);
    assert!(f.repo.task_saved_draft_source(&w, &original).is_err());
    let read_count: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_task_workspace_reads_v1 WHERE source_id=?1",
            [direct["sourceId"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(read_count, 0, "reference registration is not delivery");
    let (first_raw, first_content) = f.raw(&w, &direct);
    let first_item = saved(&f.items(&w), &first);
    assert_eq!(direct["sourceId"], first_item["sourceId"]);
    assert_eq!(first_item["producerTaskId"], "writer");
    assert_eq!(first_item["outputOrdinal"], 1);
    assert_eq!(first_item["isCurrentAtListing"], true);
    assert_eq!(first_item["completeness"], "chapterComplete");

    assert_eq!(first_content["content"]["title"], "第一保存题");
    // This fixture's aggregate retains a trailing newline whereas canonical
    // scene assembly trims it. Non-equivalent text must remain available.
    assert_eq!(
        first_content["content"]["body"],
        "保存一🙂\"\\\n".repeat(900)
    );
    assert_eq!(first_content["textRepresentation"], "body");
    assert_eq!(
        first_content["content"]["sceneGroups"][0]["body"],
        "保存一🙂\"\\\n".repeat(900)
    );
    assert!(first_content.get("isCurrentAtListing").is_none());
    let second = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "第二保存题", "保存二"),
        Some("writer"),
    );
    let w = f.current("writer");
    let items = f.items(&w);
    assert_eq!(saved(&items, &first)["isCurrentAtListing"], false);
    assert_eq!(
        f.repo.task_saved_draft_source(&w, &first).unwrap()["sourceId"],
        direct["sourceId"]
    );
    assert_eq!(saved(&items, &second)["isCurrentAtListing"], true);
    let second_content = f.raw(&w, &saved(&items, &second)).1;
    assert!(second_content["content"].get("body").is_none());
    assert_eq!(second_content["textRepresentation"], "sceneGroups");
    assert_eq!(
        second_content["content"]["sceneGroups"][0]["body"],
        "保存二"
    );
    assert_eq!(f.raw(&w, &first_item).0, first_raw);
    assert_eq!(f.raw(&w, &before).1, old);
    assert_eq!(
        items
            .iter()
            .find(|i| i["authority"] == "frozenWriterInput")
            .unwrap()["title"],
        "原题"
    );
    let (heads, _) = f.repo.task_context_origins(&w).unwrap();
    assert_eq!(
        heads[0].candidate_revision_id.as_ref().unwrap().as_str(),
        original
    );
    assert_eq!(
        heads[0].accepted_revision_id.as_ref().unwrap().as_str(),
        original
    );
    assert!(f.repo.assert_task_inputs_current(&w).is_ok());
    assert!(dependencies_current(&f.repo.connection, "writer").unwrap());
    assert!(
        !frozen_comparison(
            &f.repo.connection,
            first_item["sourceId"].as_str().unwrap(),
            f.project.as_str(),
            &first
        )
        .unwrap()
    );
    let check = crate::source_check_evidence::SourceCheck {
        source_id: first_item["sourceId"].as_str().unwrap().into(),
        revision_id: first,
        content_hash: format!("{:x}", Sha256::digest(first_raw.as_bytes())),
        evidence: json!({"quote":"保存一","start":0,"end":3}),
    };
    assert!(
        crate::source_check_evidence::validate(
            &f.repo.connection,
            &f.project,
            &f.repo
                .work_task_for_attempt(&f.project, "writer")
                .unwrap()
                .id,
            &check
        )
        .is_err(),
        "historical saved authority must not acquire frozenComparison/current-source exemption"
    );
    f.repo
        .complete_task(
            &f.project,
            "writer",
            "root",
            "保存二已登记".into(),
            None,
            10,
        )
        .unwrap();
}

#[test]
fn catalogue_cursor_changes_on_save_while_started_body_cursor_keeps_exact_hash() {
    let mut f = Fixture::new();
    for n in 1..=12 {
        f.save(
            &format!("draft-chapter-{n}"),
            "draft",
            draft(n, "目录题", "原文"),
            None,
        );
    }
    let w = f.task("writer", "generateChapter", writer_target(12));
    let source = f.directory(&w);
    let directory = f
        .repo
        .task_workspace_page(&w, Some(&source), None, None, None, None, None)
        .unwrap();
    assert!(directory["nextCursor"].is_string());
    let first = f.save(
        "draft-chapter-12",
        "draft",
        draft(12, "保存题", &"长保存🙂".repeat(1800)),
        Some("writer"),
    );
    let w = f.current("writer");
    assert!(matches!(
        f.repo.task_workspace_page(
            &w,
            Some(&source),
            directory["nextCursor"].as_str(),
            None,
            None,
            None,
            None
        ),
        Err(RepositoryError::TaskSourceCursorInvalid)
    ));
    let item = saved(&f.items(&w), &first);
    let body = f
        .repo
        .task_source_page(&w, item["sourceId"].as_str().unwrap(), None)
        .unwrap();
    assert!(body["nextCursor"].is_string());
    f.save(
        "draft-chapter-12",
        "draft",
        draft(12, "下次题", "下次正文"),
        Some("writer"),
    );
    let w = f.current("writer");
    let continued = f
        .repo
        .task_source_page(
            &w,
            item["sourceId"].as_str().unwrap(),
            body["nextCursor"].as_str(),
        )
        .unwrap();
    assert_eq!(continued["contentHash"], body["contentHash"]);
    assert_eq!(continued["byteRange"]["start"], body["byteRange"]["end"]);
    f.raw(&w, &item);
}

#[test]
fn own_planning_checkpoint_and_original_plan_artifact_share_the_generic_source_pipeline() {
    let mut f = Fixture::new();
    let old = f.save(
        "artifact-brief",
        "brief",
        json!({"title":"原简报","premise":"原设想"}),
        None,
    );
    let p = f.task("planner", "generateBrief", json!({"type":"artifact"}));
    let original = f
        .items(&p)
        .into_iter()
        .find(|i| i["revisionId"] == old)
        .unwrap();
    assert_eq!(original["authority"], "frozenTaskInput");
    f.raw(&p, &original);
    let first = f.save(
        "artifact-brief",
        "brief",
        json!({"title":"新简报","premise":"实际保存🙂".repeat(500)}),
        Some("planner"),
    );
    let p = f.current("planner");
    let item = saved(&f.items(&p), &first);
    let (_, content) = f.raw(&p, &item);
    assert_eq!(content["content"]["title"], "新简报");
    f.save(
        "artifact-brief",
        "brief",
        json!({"title":"第二简报","premise":"第二次"}),
        Some("planner"),
    );
    let p = f.current("planner");
    assert_eq!(f.raw(&p, &original).1["content"]["title"], "原简报");
    assert_eq!(f.raw(&p, &item).1, content);
    f.repo
        .complete_task(&f.project, "planner", "root", "真实保存".into(), None, 10)
        .unwrap();
}

#[test]
fn pov_prefix_saved_checkpoint_never_exposes_suffix_future_or_planning_artifact() {
    let mut f = Fixture::new();
    let body = "可读前缀\n\n---\n\n秘密后缀";
    let mut original = draft(1, "本章", body);
    original["sceneGroups"] = json!([{"id":"one","body":"可读前缀","locked":false},{"id":"two","body":"秘密后缀","locked":true}]);
    f.save("draft-chapter-1", "draft", original.clone(), None);
    f.save(
        "draft-chapter-2",
        "draft",
        draft(2, "未来题", "未来秘密"),
        None,
    );
    f.save(
        "artifact-brief",
        "brief",
        json!({"premise":"规划秘密"}),
        None,
    );
    let mut target = writer_target(1);
    target["scope"] = json!({"type":"throughScene","endSceneOrder":1});
    target["readScope"] = json!({"type":"throughScene","endSceneOrder":1});
    target["povCharacterId"] = json!("person");
    f.task("prefix", "generateChapter", target);
    let mut out = original;
    out["sceneGroups"][0]["body"] = json!("新前缀");
    out["body"] = json!("新前缀\n\n---\n\n秘密后缀");
    let revision = f.save("draft-chapter-1", "draft", out, Some("prefix"));
    let w = f.current("prefix");
    let items = f.items(&w);
    assert!(items.iter().all(|i| i["objectId"] == "draft-chapter-1"));
    let item = saved(&items, &revision);
    let direct = f.repo.task_saved_draft_source(&w, &revision).unwrap();
    assert_eq!(direct["sourceId"], item["sourceId"]);
    let (text, content) = f.raw(&w, &item);
    assert!(!text.contains("秘密后缀"));
    assert_eq!(content["content"]["sceneGroups"][0]["body"], "新前缀");
    assert_eq!(
        content["content"]["sceneGroups"].as_array().unwrap().len(),
        1
    );
    let page = f
        .repo
        .task_workspace_page(
            &w,
            Some(&f.directory(&w)),
            None,
            None,
            None,
            Some("秘密后缀"),
            None,
        )
        .unwrap();
    assert!(page["items"].as_array().unwrap().is_empty());
    assert_eq!(page["coverage"]["queryMode"], "visibleMetadataOnly");
    assert_eq!(
        page["visibility"]["planningArtifacts"],
        "excludedByWriterScope"
    );
    assert_eq!(
        page["visibility"]["prose"],
        json!({"fromChapter":1,"throughChapter":1,"throughScene":1})
    );
    assert_eq!(page["visibility"]["existenceOutsideScope"], "unknown");
    let hidden = f
        .repo
        .task_workspace_page(&w, None, None, None, Some("brief"), None, None)
        .unwrap();
    assert_eq!(hidden["items"], json!([]));
    assert_eq!(hidden["visibility"], page["visibility"]);
    assert!(!hidden.to_string().contains("规划秘密"));
}

#[test]
fn foreign_task_cannot_reuse_output_source_or_advertise_foreign_work_as_own_output() {
    let mut f = Fixture::new();
    f.save("draft-chapter-1", "draft", draft(1, "题", "原文"), None);
    f.task("writer", "generateChapter", writer_target(1));
    let revision = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "保存题", "已保存正文"),
        Some("writer"),
    );
    let w = f.current("writer");
    let item = saved(&f.items(&w), &revision);
    let foreign = f.task("foreign", "advisory", json!({"type":"project"}));
    assert!(
        !f.items(&foreign)
            .iter()
            .any(|i| i["purpose"] == "savedCheckpoint")
    );
    assert!(
        f.repo
            .task_source_page(&foreign, item["sourceId"].as_str().unwrap(), None)
            .is_err()
    );
    let mut forged = w.clone();
    forged.thread_id = "foreign-root".into();
    assert!(
        f.repo
            .task_workspace_page(
                &forged,
                Some(&f.directory(&w)),
                None,
                None,
                None,
                None,
                None
            )
            .is_err()
    );
    // Corrupt only the negative fixture: a claimed output is not a Native save relation.
    f.repo.connection.execute("INSERT INTO story_task_outputs_v2 VALUES('foreign',1,'draft',?1,NULL,'chapterComplete')",[&revision]).unwrap();
    let foreign = f.current("foreign");
    assert!(
        !f.items(&foreign)
            .iter()
            .any(|i| i["purpose"] == "savedCheckpoint")
    );
}

#[test]
fn catalogue_and_child_read_do_not_forge_root_prose_receipts_and_cancelled_reader_stops() {
    let mut f = Fixture::new();
    f.save("draft-chapter-1", "draft", draft(1, "题", "原文"), None);
    f.task("writer", "generateChapter", writer_target(1));
    let revision = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "保存题", "实际长原文"),
        Some("writer"),
    );
    let w = f.current("writer");
    let item = saved(&f.items(&w), &revision);
    let id = item["sourceId"].as_str().unwrap();
    let returns: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_task_source_raw_returns_v1 WHERE source_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(returns, 0);
    f.repo
        .task_source_content_for_reader(&w, id, None, "trusted-child")
        .unwrap();
    let returns: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_task_source_returned_v1 WHERE source_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(returns, 0);
    f.raw(&w, &item);
    f.repo
        .update_task(
            &f.project,
            "writer",
            "root",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 10,
            },
        )
        .unwrap();
    let stopped = f.current("writer");
    assert!(f.repo.task_source_page(&stopped, id, None).is_err());
    assert!(
        f.repo
            .task_workspace_page(
                &stopped,
                Some(&directory_id(&stopped)),
                None,
                None,
                None,
                None,
                None
            )
            .is_err()
    );
}

#[test]
fn recorded_saved_checkpoint_facts_survive_work_stops_without_granting_read_authority() {
    let mut f = Fixture::new();
    let writer = f.budgeted_writer(1);
    let revision = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "保存题", "同 Work 保存的真实正文"),
        Some(&writer.task_id),
    );
    f.repo
        .complete_task(
            &f.project,
            &writer.task_id,
            "root",
            "已保存".into(),
            None,
            10,
        )
        .unwrap();
    let work = f
        .repo
        .work_task_for_attempt(&f.project, &writer.task_id)
        .unwrap()
        .id;
    f.settle(&work, &f.current(&writer.task_id));
    let reader = f.decision(&work, "checkpoint-fact-reader");
    let items = f.items(&reader);
    let checkpoint = saved(&items, &revision);
    assert_eq!(checkpoint["authority"], "nativeSavedCheckpoint");
    let source = checkpoint["sourceId"].as_str().unwrap();
    f.raw(&reader, &checkpoint);
    let current = items
        .iter()
        .find(|i| i["producerTaskId"].is_null() && i["revisionId"] == revision)
        .unwrap();
    assert_eq!(current["authority"], "candidate");
    f.raw(&reader, current);
    let fact = load_ref(&f.repo.connection, &reader.task_id, source)
        .unwrap()
        .unwrap();
    let before_heads = f.repo.object_heads(&f.project).unwrap();
    let before_budget: String = f
        .repo
        .connection
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [&work],
            |r| r.get(0),
        )
        .unwrap();
    for status in ["paused", "blocked", "completed", "cancelled"] {
        // Seed only the lifecycle state under test; all save/read/provenance
        // relationships above were produced by the current Native operations.
        f.repo
            .connection
            .execute(
                "UPDATE story_work_tasks_v1 SET status=?2 WHERE id=?1",
                params![work, status],
            )
            .unwrap();
        assert!(
            dependencies_current(&f.repo.connection, &reader.task_id).unwrap(),
            "recorded facts survive {status}"
        );
        assert!(
            ref_current(
                &f.repo.connection,
                &reader,
                &fact,
                ReferenceValidation::RecordedDependency
            )
            .unwrap()
        );
        assert!(
            !ref_current(
                &f.repo.connection,
                &reader,
                &fact,
                ReferenceValidation::ActiveRead
            )
            .unwrap()
        );
        assert!(matches!(
            f.repo.materialize_workspace_source(&reader, source),
            Err(RepositoryError::TaskInputStale)
        ));
        assert!(
            f.repo.task_source_page(&reader, source, None).is_err(),
            "{status} cannot read even cached source"
        );
        assert!(
            f.repo
                .task_workspace_page(
                    &reader,
                    Some(&directory_id(&reader)),
                    None,
                    None,
                    None,
                    None,
                    None
                )
                .is_err()
        );
        assert!(
            !frozen_comparison(&f.repo.connection, source, f.project.as_str(), &revision).unwrap()
        );
    }
    assert_eq!(f.repo.object_heads(&f.project).unwrap(), before_heads);
    assert_eq!(
        f.repo
            .connection
            .query_row(
                "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
                [&work],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        before_budget
    );
    let foreign = f.task(
        "foreign-fact-writer",
        "generateBrief",
        json!({"type":"artifact"}),
    );
    let foreign_revision = f.save(
        "artifact-brief",
        "brief",
        json!({"premise":"另一个 Work 的真实保存"}),
        Some(&foreign.task_id),
    );
    let foreign_item = saved(&f.items(&f.current(&foreign.task_id)), &foreign_revision);
    let foreign_fact = load_ref(
        &f.repo.connection,
        &foreign.task_id,
        foreign_item["sourceId"].as_str().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(
        !ref_current(
            &f.repo.connection,
            &reader,
            &foreign_fact,
            ReferenceValidation::RecordedDependency
        )
        .unwrap(),
        "a real other Work output does not become this reader's fact"
    );
    let mut forged = fact.clone();
    forged.producer = Some("foreign-producer".into());
    assert!(
        !ref_current(
            &f.repo.connection,
            &reader,
            &forged,
            ReferenceValidation::RecordedDependency
        )
        .unwrap()
    );
    forged = fact.clone();
    forged.ordinal = Some(999);
    assert!(
        !ref_current(
            &f.repo.connection,
            &reader,
            &forged,
            ReferenceValidation::RecordedDependency
        )
        .unwrap()
    );
    let reader_step: String = f
        .repo
        .connection
        .query_row(
            "SELECT step_id FROM story_work_attempts_v1 WHERE attempt_id=?1",
            [&reader.task_id],
            |r| r.get(0),
        )
        .unwrap();
    f.repo
        .connection
        .execute(
            "UPDATE story_work_steps_v1 SET current_attempt_id=?2 WHERE id=?1",
            params![reader_step, writer.task_id],
        )
        .unwrap();
    assert!(
        !dependencies_current(&f.repo.connection, &reader.task_id).unwrap(),
        "a replaced current reader does not retain provenance authority"
    );
    f.repo
        .connection
        .execute(
            "UPDATE story_work_steps_v1 SET current_attempt_id=?2 WHERE id=?1",
            params![reader_step, reader.task_id],
        )
        .unwrap();
    f.repo
        .connection
        .execute(
            "UPDATE story_work_tasks_v1 SET thread_id='foreign-root' WHERE id=?1",
            [&work],
        )
        .unwrap();
    assert!(
        !dependencies_current(&f.repo.connection, &reader.task_id).unwrap(),
        "root identity stays mandatory"
    );
    f.repo
        .connection
        .execute(
            "UPDATE story_work_tasks_v1 SET thread_id='root' WHERE id=?1",
            [&work],
        )
        .unwrap();
    f.repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='changed-current-draft' WHERE project_id=?1 AND object_id='draft-chapter-1'", [f.project.as_str()]).unwrap();
    assert!(
        ref_current(
            &f.repo.connection,
            &reader,
            &fact,
            ReferenceValidation::RecordedDependency
        )
        .unwrap(),
        "immutable saved fact stays true"
    );
    assert!(
        !dependencies_current(&f.repo.connection, &reader.task_id).unwrap(),
        "the separately read current draft still becomes stale"
    );
}

#[test]
fn fresh_work_decision_reads_actual_prior_output_while_author_selected_accepted_input_stays_old() {
    let mut f = Fixture::new();
    let old = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "原题", "作者已采纳原稿"),
        None,
    );
    f.repo.connection.execute("UPDATE story_object_heads_v2 SET accepted_revision_id=?1 WHERE object_id='draft-chapter-1'",[&old]).unwrap();
    let writer = f.budgeted_writer(1);
    let writer_id = writer.task_id.clone();
    let latest = f.save(
        "draft-chapter-1",
        "draft",
        draft(1, "保存题", "同Work实际新稿"),
        Some(&writer_id),
    );
    f.repo
        .complete_task(&f.project, &writer_id, "root", "真实保存".into(), None, 10)
        .unwrap();
    let work = f
        .repo
        .work_task_for_attempt(&f.project, &writer_id)
        .unwrap()
        .id;
    let completed = f.current(&writer_id);
    f.settle(&work, &completed);
    let author_before: String = f
        .repo
        .connection
        .query_row(
            "SELECT input_manifest_json FROM story_tasks_v2 WHERE task_id=?1",
            [&writer_id],
            |r| r.get(0),
        )
        .unwrap();
    let budget_before: String = f
        .repo
        .connection
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [&work],
            |r| r.get(0),
        )
        .unwrap();
    let reader = f.decision(&work, "fresh-decision");
    assert_eq!(
        f.repo
            .work_task_for_attempt(&f.project, "fresh-decision")
            .unwrap()
            .id,
        work
    );
    f.repo
        .source_task(&reader)
        .expect("actual fresh decision binding is current before negative corruption");
    let items = f.items(&reader);
    let selected = items
        .iter()
        .find(|i| i["producerTaskId"].is_null() && i["objectId"] == "draft-chapter-1")
        .unwrap();
    assert_eq!(selected["revisionId"], old);
    let produced = saved(&items, &latest);
    assert_eq!(produced["producerTaskId"], writer_id);
    assert_eq!(produced["producerStatusAtListing"], "completed");
    assert_eq!(
        f.raw(&reader, &produced).1["content"]["sceneGroups"][0]["body"],
        "同Work实际新稿"
    );
    assert_eq!(
        f.raw(&reader, selected).1["content"]["sceneGroups"][0]["body"],
        "作者已采纳原稿"
    );
    let authors: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_author_messages_v1 WHERE work_task_id=?1",
            [&work],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(authors, 1, "reading does not insert another author message");
    assert_eq!(f.current(&writer_id).input_manifest_json, author_before);
    let budget_after: String = f
        .repo
        .connection
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [&work],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(budget_after, budget_before);
    assert_eq!(reader.thread_id, writer.thread_id);
    // Only corrupt negative fixture relationships after the real save/read proof.
    let original_sequence: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT sequence FROM story_work_attempts_v1 WHERE attempt_id=?1",
            [&writer_id],
            |r| r.get(0),
        )
        .unwrap();
    let reader_sequence: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT sequence FROM story_work_attempts_v1 WHERE attempt_id=?1",
            [&reader.task_id],
            |r| r.get(0),
        )
        .unwrap();
    f.repo
        .connection
        .execute(
            "UPDATE story_work_attempts_v1 SET sequence=?2 WHERE attempt_id=?1",
            params![writer_id, reader_sequence + 1],
        )
        .unwrap();
    assert!(
        f.repo
            .task_source_page(&reader, produced["sourceId"].as_str().unwrap(), None)
            .is_err(),
        "cached source must not bypass a producer later than this reader"
    );
    assert!(
        f.repo
            .task_workspace_page(
                &reader,
                Some(&directory_id(&reader)),
                None,
                None,
                None,
                None,
                None
            )
            .is_err(),
        "author lineage rejects producer moved after reader before directory listing"
    );
    f.repo
        .connection
        .execute(
            "UPDATE story_work_attempts_v1 SET sequence=?2 WHERE attempt_id=?1",
            params![writer_id, original_sequence],
        )
        .unwrap();
    f.repo
        .connection
        .execute(
            "UPDATE story_work_tasks_v1 SET thread_id='foreign-root' WHERE id=?1",
            [&work],
        )
        .unwrap();
    assert!(
        f.repo
            .task_workspace_page(
                &reader,
                Some(&directory_id(&reader)),
                None,
                None,
                None,
                None,
                None
            )
            .is_err()
    );
    assert!(
        f.repo
            .task_source_page(&reader, produced["sourceId"].as_str().unwrap(), None)
            .is_err(),
        "cached source must retain the original Work root binding"
    );
}

#[test]
fn real_planning_report_store_keeps_cancelled_saved_analysis_readable_to_same_work_decision_only() {
    let mut f = Fixture::new();
    let outline = f.save(
        "artifact-outline",
        "outline",
        json!({"nodes":[{"id":"book","kind":"book","title":"全书"}]}),
        None,
    );
    let root = f.budgeted_root(
        "advisory",
        json!({"type":"project"}),
        "评审现有大纲，允许停止评审后回读已保存分析，不采用、不发布。",
    );
    let root_id = root.task_id.clone();
    let work = f
        .repo
        .work_task_for_attempt(&f.project, &root_id)
        .unwrap()
        .id;
    let mut context = json!({"coverage":{"complete":false,"missingScope":[]}});
    f.repo.project_task_sources(&root, &mut context).unwrap();
    let author = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["sectionPath"] == "/authorRequirementsSource")
        .unwrap();
    assert_eq!(
        f.raw(&root, author).1["authorText"],
        serde_json::from_str::<Value>(&root.input_manifest_json).unwrap()["authorIntent"]
    );
    let plan: UpdateModelWorkPlan = serde_json::from_value(json!({"expectedPlanVersion":0,
        "idempotencyKey":"report-read-goal","plan":{"intentMode":"executeGoals","goals":[{
            "id":"review","title":"登记评审并核对实际保存分析","acceptance":"原会话核对真实报告并明确尚未通过",
            "kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"停止评审前保存了什么",
            "checkScope":"当前大纲","approach":"按需组织评审与回读","assessment":"","evidence":[],
            "sourceIds":[],"sourceChecks":[],"continuation":null}]}})).unwrap();
    f.repo.update_model_work_plan(&root, &plan).unwrap();
    let run = f
        .repo
        .request_task_planning_review(
            &f.project,
            &root_id,
            &TaskPlanningReviewRequest {
                thread_id: "root".into(),
                expected_revision: 0,
                expected_intent_revision: 0,
                artifact_kind: "outline".into(),
                revision_id: outline.clone(),
                target_node_id: None,
                roles: vec!["structure".into(), "experience".into()],
                focus: "核对结构与体验".into(),
                idempotency_key: "original-review".into(),
            },
            2,
        )
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_owned();
    assert_eq!(run["workTaskId"], work);
    f.repo
        .complete_task(
            &f.project,
            &root_id,
            "root",
            "已登记评审，实际报告待回读".into(),
            None,
            3,
        )
        .unwrap();
    let completed_root = f.current(&root_id);
    f.settle(&work, &completed_root);
    f.repo.create_planning_review_seat_task(&f.project,&run_id,"structure", CreateTaskRequest {
        task_id:"seat-one".into(),project_id:f.project.clone(),thread_id:"root".into(),
        operation:"reviewPlanningSeat".into(),input_revision:0,model:None,
        target_json:json!({"type":"artifact","artifactKind":"outline","revisionId":outline,"reviewRunId":run_id,"role":"structure"}).to_string(),
        input_manifest_json:"{}".into(),capability_hash:"a".repeat(64),capability_expires_at:i64::MAX}).unwrap();
    let report = json!({"summary":"实际保存的分析🙂".repeat(400),"strengths":[],"findings":[],
        "coverage":[{"checkId":"structure:book","status":"checked"}]});
    let stored = f
        .repo
        .save_planning_review_report(SavePlanningReviewReport {
            project_id: &f.project,
            run_id: &run_id,
            task_id: "seat-one",
            thread_id: "root",
            expected_revision: 0,
            report: &report,
            now: 4,
        })
        .unwrap();
    let seat = f.current("seat-one");
    let own = f
        .items(&seat)
        .into_iter()
        .find(|i| i["revisionId"] == stored["id"])
        .unwrap();
    assert_eq!(own["purpose"], "savedReport");
    assert_eq!(own["authority"], "nativePlanningReport");
    assert_eq!(f.raw(&seat, &own).1["content"], report);
    let exists: bool = f
        .repo
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_revisions_v2 WHERE revision_id=?1)",
            [stored["id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !exists,
        "report id must not pretend to be a revision-store row"
    );
    f.repo
        .finish_planning_review(&f.project, &run_id, true, 5)
        .unwrap();
    let cancelled = f.current("seat-one");
    assert_eq!(cancelled.status, "cancelled");
    f.settle(&work, &cancelled);
    assert_eq!(
        f.repo.work_task(&f.project, &work).unwrap().status,
        "running",
        "only domain review was cancelled; original conversation delivery remains pending"
    );
    assert!(
        f.repo
            .task_source_page(&cancelled, own["sourceId"].as_str().unwrap(), None)
            .is_err()
    );
    let reader = f.decision(&work, "report-reader");
    let old = f
        .items(&reader)
        .into_iter()
        .find(|i| i["revisionId"] == stored["id"])
        .unwrap();
    assert_eq!(old["producerTaskId"], "seat-one");
    assert_eq!(old["producerStatusAtListing"], "cancelled");
    assert_eq!(old["completeness"], "complete");
    assert_eq!(f.raw(&reader, &old).1["content"], report);
    let report_source = old["sourceId"].as_str().unwrap();
    for status in ["paused", "blocked", "completed", "cancelled"] {
        f.repo
            .connection
            .execute(
                "UPDATE story_work_tasks_v1 SET status=?2 WHERE id=?1",
                params![work, status],
            )
            .unwrap();
        assert!(
            dependencies_current(&f.repo.connection, &reader.task_id).unwrap(),
            "saved analysis remains a recorded fact in {status}"
        );
        assert!(matches!(
            f.repo.materialize_workspace_source(&reader, report_source),
            Err(RepositoryError::TaskInputStale)
        ));
        assert!(
            f.repo
                .task_source_page(&reader, report_source, None)
                .is_err()
        );
    }
    f.repo
        .connection
        .execute(
            "UPDATE story_work_tasks_v1 SET status='running' WHERE id=?1",
            [&work],
        )
        .unwrap();
    assert!(
        f.repo
            .task_source_page(&reader, own["sourceId"].as_str().unwrap(), None)
            .is_err(),
        "new reader must rediscover its own source identity"
    );
    let foreign = f.task("foreign", "advisory", json!({"type":"project"}));
    assert!(
        !f.items(&foreign)
            .iter()
            .any(|i| i["revisionId"] == stored["id"])
    );
    // Negative-only tampering must not pass even after real report was cached.
    f.repo
        .connection
        .execute(
            "UPDATE story_planning_review_reports_v2 SET task_id='foreign' WHERE report_id=?1",
            [stored["id"].as_str().unwrap()],
        )
        .unwrap();
    assert!(
        f.repo
            .task_source_page(&reader, old["sourceId"].as_str().unwrap(), None)
            .is_err()
    );
}

use crate::{InitialWorkBudgetRepository, ModelWorkPlanRepository, OrdinaryWorkBudgetRepository, PlanningReviewsRepository, ProjectRevisionRepository, TaskInputRepository, TaskPlanningReviewRepository, TaskSourcesRepository, TaskWorkspaceRepository, TasksRepository, WorkBudgetAuthorizationRepository, WorkDecisionRepository, WorkTasksRepository};
