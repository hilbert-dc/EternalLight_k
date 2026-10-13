import { useEffect, useState } from "react";
import type { Chapter, NovelCodexApi, RollingPlanPayload } from "./types";
import { useCharacterStory } from "./useCharacterStory";
import { characterRecords, storyDescription, storySource, type StoryEntry } from "./character-story";
import "./character-catalog.css";

export function CharacterCatalog({ api, projectId, refreshToken, chapters, onOpenChapter, onDiscuss }: {
  api: NovelCodexApi | undefined; projectId: string; refreshToken: number;
  onDiscuss?: (prompt: string) => void; chapters: Chapter[]; onOpenChapter: (chapter: Chapter) => void;
}) {
  const [selection, setSelection] = useState<{ projectId: string; id: string } | null>(null);
  const selectedId = selection?.projectId === projectId ? selection.id : null;
  const data = useCharacterStory({ api, projectId, refreshToken, selectedEntityId: selectedId });
  const { workspace, catalog, knowledge } = data;
  const people = characterRecords(catalog);
  const person = people.find(item => item.id === selectedId);
  useEffect(() => {
    if (!data.current || people.some(item => item.id === selectedId)) return;
    setSelection(people[0] ? { projectId, id: people[0].id } : null);
  }, [catalog, data.current, projectId, selectedId]);
  const ready = data.current && data.knowledgeCurrent;
  const entries = knowledge?.entries ?? [];
  const descriptionEntries = person && workspace ? entries.filter(entry => entry.canonicalSubjectId === person.id
    && ["state", "assertion", "relation"].includes(entry.assertion.kind) && storySource(entry, workspace).status !== "历史来源") : [];
  const chapterGroups = new Map<string, StoryEntry[]>();
  const otherEntries: StoryEntry[] = [];
  for (const entry of entries) {
    const chapter = workspace && storySource(entry, workspace).chapter;
    if (chapter) chapterGroups.set(chapter.id, [...(chapterGroups.get(chapter.id) ?? []), entry]);
    else otherEntries.push(entry);
  }
  const ordered = workspace?.chapters.filter(chapter => chapterGroups.has(chapter.id)).sort((a, b) => a.number - b.number) ?? [];
  const plan = workspace?.artifacts.plan.candidate ?? workspace?.artifacts.plan.accepted;
  const planned = person ? ((plan?.payload as unknown as RollingPlanPayload | undefined)?.chapters ?? []).filter(chapter => chapter.povCharacterId && person.members.includes(chapter.povCharacterId)).sort((a, b) => a.chapterNumber - b.chapterNumber) : [];
  const incomplete = knowledge && (!knowledge.coverage.complete || knowledge.coverage.truncatedEntries || knowledge.coverage.pendingSources.length > 0);
  const discuss = () => {
    if (!onDiscuss || !person || !workspace || !ready) return;
    onDiscuss(`请围绕「${person.name}」梳理和完善人物简介与故事线。作品：${projectId}；当前已保存资料版本：${workspace.revision}；来源水位：${workspace.sourceDigest}；人物身份：${person.id}；请读取当前身份目录核对该人物的关联身份。请先读取并核对当前已保存正文、人物事件、关系、认知和规划；按章节梳理经历、关键选择和变化，把已写内容与后续建议分开，指出缺失或矛盾。同名不能视为同一人，不凭空补性格或动机。需要补分析时按当前任务工具处理并核对保存结果；先讨论，不修改正文、不自动确认事实或合并身份。`);
  };
  const openChapter = (id: string) => { const chapter = chapters.find(item => item.id === id); if (chapter) onOpenChapter(chapter); };
  function entryView(entry: StoryEntry) {
    if (!workspace || !catalog) return null;
    const text = storyDescription(entry, catalog);
    const source = storySource(entry, workspace);
    return <li className="character-story-entry" key={`${entry.modelingBatchId}:${entry.assertionIndex}`}
      data-discussion-revision={entry.sourceRevisionId} data-discussion-object={entry.sourceObjectId}
      data-discussion-locator={`${entry.modelingBatchId}:${entry.assertionIndex}`}>
      <div className="character-story-entry-heading"><strong>{text.title}</strong><small>{entry.confirmationState === "confirmed" ? "作者已确认" : "分析建议"} · {source.status}</small></div>
      <p>{text.description}</p>
      {typeof entry.assertion.storyTime === "number" && <small>故事时刻：{entry.assertion.storyTime}</small>}
      {entry.assertion.evidence?.quote && <details><summary>原文依据</summary><blockquote>{entry.assertion.evidence.quote}</blockquote></details>}
    </li>;
  }
  return <section className="view-panel character-catalog" aria-label="角色资料" data-discussion-snapshot={workspace?.headSnapshotId} data-discussion-revision="">
    <header className="view-intro"><p>了解每个人是谁，以及他们一路经历了什么。故事线随已保存的正文分析更新。</p></header>
    {!api || !projectId ? <p role="status">请先打开作品并连接本地运行时。</p> : <>
      {data.loading && <p role="status">{catalog ? "正在更新角色资料，保留上次读取结果…" : "正在读取角色资料…"}</p>}
      {data.error && <div role="alert"><p>角色资料读取失败：{data.error}{catalog && "。以下保留上次读取结果。"}</p><button type="button" onClick={data.retry}>重新读取角色</button></div>}
      {catalog && (catalog.truncated || !catalog.ambiguityScanComplete) && <p role="status">目前仅展示部分角色来源，人物身份尚未完整核对。</p>}
      {workspace?.artifacts.identity.candidate && workspace.artifacts.identity.candidate.id !== workspace.artifacts.identity.accepted?.id && <p role="status">有身份建议尚未采用，可在下方“同名与身份管理”中处理。</p>}
      {catalog && !people.length && <div className="empty-state"><strong>人物故事将从这里展开</strong><p>章节保存并完成分析后，会自动汇集人物简介与经历。也可以在对话中先讨论人物设定。</p></div>}
      {people.length > 0 && <div className="character-workspace">
        <label className="character-picker"><span>选择角色</span><select aria-label="选择角色" value={person?.id ?? ""} onChange={event => setSelection({ projectId, id: event.target.value })}>{people.map(item => <option key={item.id} value={item.id}>{item.name}{people.some(other => other.id !== item.id && other.name === item.name) ? ` · ${item.claims[0]?.evidence.quote.slice(0, 30) ?? "待核对"}` : ""}</option>)}</select></label>
        <nav className="character-list" aria-label="选择角色">{people.map(item => <button className="character-select" type="button" key={item.id} aria-pressed={item.id === person?.id}
          onClick={() => setSelection({ projectId, id: item.id })}><strong>{item.name}</strong><small>{item.status || `${item.claims.length} 处人物来源`}</small>{people.some(other => other.id !== item.id && other.name === item.name) && <small className="character-name-context">{item.claims[0]?.evidence.quote || "来源说明暂缺"}</small>}</button>)}</nav>
        {person && workspace && <article className="character-catalog-card" aria-label={`${person.name}的人物档案`}
          data-discussion-label={person.name} data-discussion-locator={person.id}>
          <header className="character-profile-heading"><div><h2>{person.name}</h2>{person.status && <small>{person.status}</small>}{person.names.length > 1 && <p>其他名称：{person.names.slice(1).join("、")}</p>}</div>
            {onDiscuss && <button type="button" className="secondary-button" disabled={!ready} onClick={discuss}>讨论人物故事线</button>}</header>
          {data.knowledgeLoading && <p role="status">正在读取人物经历…</p>}
          {data.knowledgeError && <div role="alert"><p>人物经历读取失败：{data.knowledgeError}。已显示的旧资料尚未更新。</p><button type="button" onClick={data.retryKnowledge}>重新读取人物经历</button></div>}
          {incomplete && <p className="character-catalog-warning" role="status">仍有正文待分析或资料未完整读取，下面仅为已读取范围。</p>}
          <section className="character-profile-section"><h3>人物简介</h3>
            {descriptionEntries.length > 0 ? <><p className="character-section-note">来自正文的属性与关系，按来源章节排列，不代表人物始终如此。</p><ul className="character-biography">{[...descriptionEntries].sort((a, b) => (a.sourceChapter ?? Infinity) - (b.sourceChapter ?? Infinity)).map(entry => <li key={`${entry.modelingBatchId}:${entry.assertionIndex}`}><span>{catalog && storyDescription(entry, catalog).description}</span><small>{entry.sourceChapter == null ? "其他来源" : `第 ${entry.sourceChapter} 章`} · {entry.confirmationState === "confirmed" ? "作者已确认" : "分析建议"}</small></li>)}</ul></>
              : <p className="character-section-note">{data.knowledgeLoading ? "正在读取简介信息…" : data.knowledgeError ? "暂时无法核对人物简介，请重试读取。" : "现有正文资料尚不足以形成简介；可以在对话中补充人物身份、性格与目标。"}</p>}
          </section>
          <section className="character-profile-section"><h3>已记录故事线</h3><p className="character-section-note">按章节阅读顺序梳理经历、关系和认知；不推断事件的因果，也不把未分析内容当作没有发生。</p>
            {ordered.length ? <ol className="character-storyline">{ordered.map(chapter => <li className="character-story-chapter" key={chapter.id}><header><h4>第 {chapter.number} 章 · {chapter.title}</h4><button type="button" className="text-button" disabled={!chapters.some(item => item.id === chapter.id)} onClick={() => openChapter(chapter.id)}>查看正文</button></header><ul>{chapterGroups.get(chapter.id)!.map(entryView)}</ul></li>)}</ol>
              : <p>{data.knowledgeLoading ? "正在读取人物经历…" : data.knowledgeError ? "暂时无法核对人物经历，请重试读取。" : "尚未汇集到这个人物的章节经历。"}</p>}
            {otherEntries.length > 0 && <details><summary>其他来源的记录 · {otherEntries.length} 条</summary><ul>{otherEntries.map(entryView)}</ul></details>}
          </section>
          <section className="character-profile-section"><h3>后续规划</h3><p className="character-section-note">仅列明确指定这个人物为视角的已保存规划；规划尚不代表情节已发生。</p>
            {planned.length ? <ul className="character-plans">{planned.map((chapter, index) => <li key={chapter.chapterPlanId ?? chapter.clientKey ?? index}><h4>第 {chapter.chapterNumber} 章{chapter.title ? ` · ${chapter.title}` : ""}</h4><small>{workspace.artifacts.plan.accepted?.id === plan?.id ? "已采用规划" : "规划建议，尚未采用"}{workspace.planLineageStatus === "stale" && " · 大纲依据已变化"}</small><p>{chapter.goal}</p>{chapter.conflict && <p>冲突：{chapter.conflict}</p>}</li>)}</ul>
              : <p>当前规划未指定这个人物的视角安排，可以继续在对话中讨论走向。</p>}
          </section>
          <details className="character-sources"><summary>名称与原文来源 · {person.claims.length} 处</summary>{person.claims.map((claim, index) => { const source = storySource(claim, workspace); return <div className="character-catalog-source" key={`${claim.sourceRevisionId}:${index}`}><strong>{source.chapter ? `第 ${source.chapter.number} 章 · ${source.chapter.title}` : "其他已分析内容"} · {source.status}</strong><blockquote>{claim.evidence.quote}</blockquote>{source.chapter && <button type="button" onClick={() => openChapter(source.chapter!.id)}>查看第 {source.chapter.number} 章</button>}</div>; })}</details>
        </article>}
      </div>}
    </>}
  </section>;
}
