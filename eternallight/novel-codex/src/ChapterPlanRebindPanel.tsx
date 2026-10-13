import { useEffect, useRef, useState } from "react";
import type { ChapterPlanPayload, ChapterPlanRebindReceipt, ChapterPlanRebindRequest, CreativeWorkspace, NovelCodexApi } from "./types";
import "./chapter-plan-rebind.css";

type Props = { api: NovelCodexApi; projectId: string; chapterId: string; disabled: boolean; hasUpdate: boolean; submit: (request: ChapterPlanRebindRequest) => Promise<ChapterPlanRebindReceipt>; refresh: () => Promise<void> };
type Preview = { workspace: CreativeWorkspace; choices: ChapterPlanPayload[]; sourceId: string; body: string; planId: string; binding: NonNullable<CreativeWorkspace["chapterPlanBindings"]>[number]; key: string };

export function ChapterPlanRebindPanel(props: Props) {
  return <Controls key={`${props.projectId}/${props.chapterId}`} {...props} />;
}

function Controls({ api, projectId, chapterId, disabled, hasUpdate, submit, refresh }: Props) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [selected, setSelected] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [receipt, setReceipt] = useState<ChapterPlanRebindReceipt | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  async function request(action: () => Promise<void>) {
    if (disabled || pending.current) return;
    pending.current = true; setBusy(true); setError("");
    try { await action(); }
    catch (reason) { if (alive.current) { setPreview(null); setAcknowledged(false); setError(`${reason instanceof Error ? reason.message : "请求失败"}。${receipt ? "关联已成功，可稍后重新读取作品。" : "请重新预览后再确认。"}`); } }
    finally { pending.current = false; if (alive.current) setBusy(false); }
  }
  function load() {
    void request(async () => {
      setPreview(null); setAcknowledged(false); setReceipt(null);
      const workspace = await api.runtime.getCreativeWorkspace(projectId);
      if (!alive.current) return;
      if (workspace.project.id !== projectId) throw new Error("作品范围不匹配");
      const chapter = workspace.chapters.find(value => value.id === chapterId);
      const binding = workspace.chapterPlanBindings?.find(value => value.chapter_id === chapterId);
      const draft = chapter?.candidate ?? chapter?.accepted;
      const plan = workspace.artifacts.plan.candidate ?? workspace.artifacts.plan.accepted;
      if (!chapter || !binding || !draft || !plan) throw new Error("缺少当前正文、旧绑定或新规划");
      const entries = Array.isArray(plan.payload?.chapters) ? plan.payload?.chapters as ChapterPlanPayload[] : [];
      const choices = entries.filter(entry => entry.chapterPlanId && entry.chapterNumber === chapter.number
        && entries.filter(other => other.chapterPlanId === entry.chapterPlanId).length === 1
        && !workspace.chapterPlanBindings?.some(other => other.chapter_id !== chapterId && other.chapter_plan_id === entry.chapterPlanId)
        && (plan.id !== binding.plan_revision_id || entry.chapterPlanId !== binding.chapter_plan_id));
      setPreview({ workspace, choices, sourceId: draft.id, body: draft.body, planId: plan.id, binding, key: `desktop-rebind-${crypto.randomUUID()}` });
      setSelected(choices.find(value => value.chapterPlanId === binding.chapter_plan_id)?.chapterPlanId ?? choices[0]?.chapterPlanId ?? "");
    });
  }
  const plan = preview?.choices.find(value => value.chapterPlanId === selected);
  if (!hasUpdate && !preview && !receipt && !error && !busy) return null;
  return <section className="chapter-plan-rebind" aria-label="调整本章规划关联" aria-busy={busy}>
    <strong>本章规划有更新</strong>
    <p>先核对新章纲，再决定是否替换本章的规划依据。已写正文保持不变。</p>
    <button type="button" className="secondary-button" disabled={disabled || busy} onClick={load}>{busy ? "处理中…" : preview ? "重新预览章纲调整" : "预览章纲调整"}</button>
    {error && <p role="alert">{error}</p>}
    {preview && <>
      <div className="chapter-plan-comparison">
        <div><h4>原章纲</h4><p>{preview.binding.writerProjection?.publicTitle || "无公开标题"}</p><p>{preview.binding.writerProjection?.writerGuidance || "无公开写作指引"}</p></div>
        <div><h4>新章纲</h4><label>同章号的新章纲<select aria-label="同章号的新章纲" disabled={disabled || busy} value={selected} onChange={event => { setSelected(event.target.value); setAcknowledged(false); }}>
          {preview.choices.map(entry => <option key={entry.chapterPlanId} value={entry.chapterPlanId}>{entry.publicTitle || entry.title || `第 ${entry.chapterNumber} 章章纲`}</option>)}
        </select></label><p>{plan?.writerGuidance || "无公开写作指引"}</p></div>
      </div>
      {preview.choices.length === 0 && <p role="alert">没有同章号、唯一且未被其他章节占用的新章纲。请先调整规划。</p>}
      <div className="rebind-preserved-prose"><strong>保留当前正文</strong><p>{preview.body.slice(0, 240)}{preview.body.length > 240 ? "…" : ""}</p></div>
      <details><summary>查看来源版本</summary><dl><dt>旧规划</dt><dd>{preview.binding.plan_revision_id} / {preview.binding.chapter_plan_id}</dd><dt>新规划</dt><dd>{preview.planId} / {selected || "未选择"}</dd><dt>当前正文</dt><dd>{preview.sourceId}</dd><dt>作品版本</dt><dd>{preview.workspace.revision}</dd></dl></details>
      <label className="rebind-acknowledgment"><input type="checkbox" checked={acknowledged} disabled={disabled || busy || !plan} onChange={event => setAcknowledged(event.target.checked)} />我确认保留当前正文，替换规划关联并撤销旧任务的写入权限。</label>
      <button type="button" className="primary-button" disabled={disabled || busy || !acknowledged || !plan} onClick={() => void request(async () => {
        const result = await submit({ chapterId, previousPlanRevisionId: preview.binding.plan_revision_id, previousChapterPlanId: preview.binding.chapter_plan_id, sourceRevisionId: preview.sourceId, planRevisionId: preview.planId, chapterPlanId: selected, expectedRevision: preview.workspace.revision, idempotencyKey: preview.key });
        if (alive.current) { setPreview(null); setAcknowledged(false); setReceipt(result); }
      })}>确认替换章纲关联</button>
    </>}
    {receipt && <div role="status"><p>关联已更新至作品版本 {receipt.binding.resultRevision}，正文保留。新内容仍需单独生成和审阅。</p><p>如需继续中断的创作，请在作品概况中重新预览运行，选择“基于当前章重写”。</p><details><summary>查看关联回执</summary><p>原规划：{receipt.previousPlanRevisionId} / {receipt.previousChapterPlanId}</p><p>新规划：{receipt.binding.planRevisionId} / {receipt.binding.chapterPlanId}</p><p>保留正文来源：{receipt.sourceRevisionId}</p></details><button type="button" className="secondary-button" disabled={disabled || busy} onClick={() => void request(refresh)}>重新读取作品</button></div>}
  </section>;
}
