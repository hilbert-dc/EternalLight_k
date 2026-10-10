//! Discover installed resources without turning the catalog into a router or
//! granting tools. Availability comes from the live admitted tool set.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

// Generated from the same role source that creates the actual Codex profiles.
static ROLES: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../shared/novel-agent-roles.json"))
        .expect("checked generated role resource")
});

pub(super) fn spec() -> Value {
    json!({"name":"discover_creative_capabilities",
    "description":"Browse or search installed tools, optional methods and role templates. Available tool results load their real current schemas through Engine discovery; outsideCurrentTask results grant no authority. Use roleId from a role result to read its actual instructions in bounded pages; omit query in that mode. This is not a closed list of thinking methods or a mandatory team. You may solve directly or invent a temporary method. No writes, story evidence or extra permission. Copy cursor for later pages.",
    "inputSchema":{"type":"object","additionalProperties":false,
        "required":["threadId"],"properties":{
            "threadId":{"type":"string","minLength":1,"maxLength":160},
            "query":{"type":"string","maxLength":120,"description":"Choose concise search terms from the author's goal. Native matches installed metadata literally, not semantic intent. If matches are absent or uncertain, omit query to browse and decide relevance yourself."},
            "cursor":{"type":"integer","minimum":0,"maximum":100000},
            "roleId":{"type":"string","minLength":1,"maxLength":64}
        }}})
}

fn domain(name: &str) -> &'static str {
    match name {
        "read_media_workspace" | "read_media_source" | "save_media_document" | "patch_media_document" => "漫画 动画 画格 页面 对白 镜头 片段 时间线 素材 改编 原文 来源 编辑 共同设定 canon comic animation media adaptation source",
        "request_media_generation" | "read_media_generation" | "wait_media_generation" | "cancel_media_generation" => "生成 图片 编辑图 视频 配音 语音 转写 媒体 作业 核对 取消 image video speech audio transcription media generation",
        "read_context"
        | "read_task_source"
        | "read_source_content"
        | "search_story"
        | "query_story"
        | "resolve_story_evidence"
        | "read_reading_progress"
        | "checkpoint_reading" => "作品资料 原文 证据 检索 分页 阅读 上下文",
        "search_relations" | "analyze_story_impact" => "人物 关系 连续性 修改影响 显式依赖",
        "simulate_story_hypothesis" => "假设 反事实 探索 分歧",
        "save_brief"
        | "save_outline"
        | "save_chapter_plans"
        | "save_property_rules"
        | "request_planning_review"
        | "submit_planning_review_report" => "简报 大纲 近章规划 规则 结构 规划评审",
        "save_draft_checkpoint" | "save_delegated_draft" | "read_delegated_draft" => {
            "正文 起草 修订 场景 版本 隔离候选 子智能体"
        }
        "read_commitment" | "submit_commitment_candidate" | "submit_commitment_extraction" => {
            "伏笔 长篇承诺 兑现 提取 修订"
        }
        "submit_revision_outcome"
        | "submit_scene_assessment"
        | "read_revision_outcome"
        | "read_chapter_structure_source" => "正文核验 独立评估 时间线 人物知情 修改效果 检查",
        "read_quality_notebook"
        | "record_quality_note"
        | "read_quality_review"
        | "read_quality_packet"
        | "submit_quality_reading" => "审美 读者体验 方法草案 比较 偏好 质量记录",
        "read_work_issues" => "问题 观点 证据 修订 核验 争议 待复核 交付",
        "publish_knowledge_batch" => "知识 人物状态 候选事实 来源建模",
        _ => "工作目标 任务 计划 继续 暂停 控制 回执 交付",
    }
}

fn score(record: &Value, query: &str) -> usize {
    let text = record.to_string().to_lowercase();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 1;
    }
    let terms: Vec<_> = query.split_whitespace().collect();
    let whole = terms.iter().filter(|term| text.contains(**term)).count() * 20;
    let chinese = query
        .chars()
        .filter(|c| ('\u{3400}'..='\u{9fff}').contains(c) && text.contains(*c))
        .count();
    whole + chinese
}

pub(super) fn read(
    all: &[Value],
    available: &[Value],
    query: &str,
    cursor: usize,
) -> Result<Value, String> {
    let mut entries: Vec<_> = all.iter().filter_map(|tool| {
        let name = tool["name"].as_str()?;
        if matches!(name, "read_creative_strategy" | "discover_creative_capabilities") { return None; }
        let callable = available.iter().any(|item| item["name"] == name);
        let preview=tool["description"].as_str().unwrap_or_default().chars().take(140).collect::<String>();
        Some(json!({"id":name,"kind":"tool","topics":domain(name),
            "description":preview,"availability":if callable {"available"} else {"outsideCurrentTask"},
            "limitations":if callable {"Use its actual current schema and admitted scope"} else {"Installed resource; current task has no call authority. Request a legitimate scoped task only when authorized."}}))
    }).collect();
    let methods = super::creative_strategy::read("catalog")?;
    for method in methods["content"].as_array().into_iter().flatten() {
        entries.push(json!({"id":method["id"],"kind":"method","title":method["title"],
            "when":method["when"],"skip":method["skip"],"status":methods["status"],
            "availability":if available.iter().any(|t|t["name"]=="read_creative_strategy") {"available"} else {"outsideCurrentTask"},
            "authority":"advisoryMethodOnly","readWith":"read_creative_strategy"}));
    }
    for role in ROLES["roles"].as_array().into_iter().flatten() {
        entries.push(json!({"id":role["id"],"kind":"role","description":role["description"],
            "topics":role["topics"],"permissionMode":role["permissionMode"],
            "availability":"installedTemplate","authority":ROLES["authority"],
            "instructionsRef":role["instructionsRef"],
            "limitations":"A template does not grant delegation or tools. Engine availability, Native lineage and current task scope remain authoritative."}));
    }
    let mut matches: Vec<_> = entries
        .into_iter()
        .filter_map(|item| {
            let rank = score(&item, query);
            (rank > 0).then_some((rank, item))
        })
        .collect();
    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1["id"].as_str().cmp(&b.1["id"].as_str()))
    });
    if cursor > matches.len() {
        return Err("cursor is outside this query result; browse again from zero".into());
    }
    let mut end = (cursor + 6).min(matches.len());
    while end > cursor + 1
        && serde_json::to_vec(
            &matches[cursor..end]
                .iter()
                .map(|(_, item)| item)
                .collect::<Vec<_>>(),
        )
        .map_err(|_| "Cannot encode capability catalog")?
        .len()
            > 4000
    {
        end -= 1;
    }
    let loaded: Vec<_> = matches[cursor..end]
        .iter()
        .filter(|(_, item)| item["kind"] == "tool" && item["availability"] == "available")
        .map(|(_, item)| json!({"name":item["id"],"namespace":null}))
        .collect();
    Ok(
        json!({"query":query,"items":matches[cursor..end].iter().map(|(_,item)|item).collect::<Vec<_>>(),
        "nextCursor":(end<matches.len()).then_some(end),"total":matches.len(),
        "catalogIsExhaustive":false,"storyEvidence":false,"authority":"resourceDiscoveryOnly",
        "toolDiscovery":{"version":1,"tools":loaded},
        "guidance":"Understand the author's goal and decide semantic relevance yourself; this catalog only locates literal metadata matches. A missing match does not prove a capability is absent; browse without query when uncertain. Methods and tools differ. You may use zero methods, combine available tools or design a task-local method. Method drafts and their evidence-based effects are in read_quality_notebook, reusable on demand only within the same Native Work and never auto-promoted."}),
    )
}

pub(super) fn read_role(id: &str, cursor: usize) -> Result<Value, String> {
    let role = ROLES["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .ok_or("Unknown installed roleId")?;
    let source = role["instructions"]
        .as_str()
        .ok_or("Invalid generated role instructions")?;
    let chars: Vec<_> = source.chars().collect();
    if cursor >= chars.len() {
        return Err("Invalid role cursor; restart without cursor".into());
    }
    let mut end = cursor;
    let mut bytes = 0;
    while end < chars.len() && bytes + chars[end].len_utf8() <= 1500 {
        bytes += chars[end].len_utf8();
        end += 1;
    }
    Ok(
        json!({"roleId":id,"representation":"roleInstructions","authority":ROLES["authority"],
        "permissionMode":role["permissionMode"],"contentHash":format!("{:x}",Sha256::digest(source.as_bytes())),
        "content":chars[cursor..end].iter().collect::<String>(),"range":{"start":cursor,"end":end,"total":chars.len(),"unit":"UnicodeCharacters"},
        "nextCursor":(end<chars.len()).then_some(end),"storyEvidence":false,
        "toolDiscovery":{"version":1,"tools":[]},"guidance":ROLES["guidance"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chinese_search_does_not_grant_tools_or_require_registered_methods() {
        let all = vec![
            json!({"name":"save_draft_checkpoint","description":"Save actual prose"}),
            json!({"name":"analyze_story_impact","description":"Explicit dependencies"}),
        ];
        let result = read(&all, &[], "正文", 0).unwrap();
        assert!(result["items"].as_array().unwrap().iter().any(|i| i["id"]
            == "save_draft_checkpoint"
            && i["availability"] == "outsideCurrentTask"));
        assert_eq!(result["storyEvidence"], false);
        assert_eq!(result["catalogIsExhaustive"], false);
        let available = read(&all, &all, "正文", 0).unwrap();
        assert!(
            available["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["id"] == "save_draft_checkpoint" && i["availability"] == "available")
        );
        assert!(read(&all, &all, "", 999).is_err());
        assert!(
            result["toolDiscovery"]["tools"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            available["toolDiscovery"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| all.iter().any(|s| s["name"] == t["name"]))
        );
    }
    #[test]
    fn role_templates_are_discoverable_and_exact_paginated_guidance_not_authority() {
        let found = read(&[], &[], "隔离候选 写作", 0).unwrap();
        assert!(
            found["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == "candidate_writer" && r["kind"] == "role")
        );
        let mut cursor = 0;
        let mut content = String::new();
        loop {
            let page = read_role("candidate_writer", cursor).unwrap();
            assert!(page.to_string().len() < 3000);
            assert_eq!(page["storyEvidence"], false);
            assert!(
                page["toolDiscovery"]["tools"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            content.push_str(page["content"].as_str().unwrap());
            let Some(next) = page["nextCursor"].as_u64() else {
                break;
            };
            assert!(next as usize > cursor);
            cursor = next as usize;
        }
        let original = ROLES["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "candidate_writer")
            .unwrap();
        assert_eq!(content, original["instructions"].as_str().unwrap());
        assert!(read_role("../credentials", 0).is_err());
        assert!(read_role("candidate_writer", 100000).is_err());
    }
}
