import { PlanningCharacterProvider, PlanningCharacterSelect } from "./PlanningCharacterContext";
import type { ChapterPlanPayload, NovelCodexApi, OutlineNode } from "./types";
import type { usePlanningDraft } from "./usePlanningDraft";

export function ChapterPlanEditor({ api, projectId, refreshToken, disabled, dirty, planningDraft, acceptedPlanChapters, boundPlanIds, lineageStatus, outlineNodes, outlinePaths, onMaterialize }: {
  api?: NovelCodexApi; projectId: string; refreshToken: number; disabled: boolean; dirty: boolean;
  planningDraft: ReturnType<typeof usePlanningDraft>; acceptedPlanChapters: ChapterPlanPayload[];
  boundPlanIds: Set<string>; lineageStatus: string | undefined; outlineNodes: OutlineNode[]; outlinePaths: Map<string, string>;
  onMaterialize: (chapterPlanId: string, chapterNumber: number) => Promise<void>;
}) {
  const { plans } = planningDraft;
  return <PlanningCharacterProvider api={api} projectId={projectId} refreshToken={refreshToken}><fieldset className="plan-editor-list" disabled={disabled}>
        {acceptedPlanChapters.length > 0 && <details className="plan-materialization"><summary>已采用的章纲 · {acceptedPlanChapters.length}</summary><small>{lineageStatus === "stale" ? "章纲基线已过期，采用新版章纲后才能继续开写。" : "创建后才会出现可写的章节。还没开写的章可以继续改。"}</small>{acceptedPlanChapters.map((entry) => <div key={entry.chapterPlanId}><span>第 {entry.chapterNumber} 章 · {entry.publicTitle || entry.title || "未命名"}</span>{entry.chapterPlanId && <button type="button" className="secondary-button" disabled={dirty || lineageStatus !== "current" || boundPlanIds.has(entry.chapterPlanId)} onClick={() => void onMaterialize(entry.chapterPlanId!, entry.chapterNumber)}>{boundPlanIds.has(entry.chapterPlanId) ? "已创建" : "创建正文"}</button>}</div>)}</details>}
        {plans.map((item, index) => <div className="plan-editor-card" data-planning-node-id={item.chapterPlanId ?? item.id} key={item.id}>
          <div className="plan-card-head">
            <span className="plan-number">第 <input aria-label={`第 ${item.chapterNumber} 章序号`} type="number" min="1" value={item.chapterNumber} onChange={(event) => planningDraft.editPlan(index, { chapterNumber: Number(event.target.value) })} /> 章</span>
            <input className="plan-title" aria-label={`第 ${item.chapterNumber} 章标题`} placeholder="章节标题" value={item.title} onChange={(event) => planningDraft.editPlan(index, { title: event.target.value })} />
            <label className="plan-lock"><input type="checkbox" checked={item.locked === true} onChange={(event) => planningDraft.editPlan(index, { locked: event.target.checked })} /><span>锁定</span></label>
          </div>
          <label className="plan-field"><span>本章目标</span><textarea aria-label={`第 ${item.chapterNumber} 章目标`} placeholder="这一章要完成什么" value={item.goal} onChange={(event) => planningDraft.editPlan(index, { goal: event.target.value })} /></label>
          <label className="plan-field"><span>核心冲突</span><input aria-label={`第 ${item.chapterNumber} 章核心冲突`} placeholder="谁和什么对着干" value={item.conflict} onChange={(event) => planningDraft.editPlan(index, { conflict: event.target.value })} /></label>
          <label className="plan-field"><span>给正文的指引</span><textarea aria-label={`第 ${item.chapterNumber} 章写作指引`} placeholder="写这一章时需要知道的安排" value={item.writerGuidance ?? ""} onChange={(event) => planningDraft.editPlan(index, { writerGuidance: event.target.value })} /></label>
          <details className="plan-advanced"><summary>正文标题、承诺与更多设置</summary><div className="plan-advanced-fields">
          <label className="plan-field"><span>正文标题</span><input aria-label={`第 ${item.chapterNumber} 章公开标题`} placeholder="写正文时使用的标题，可留空" value={item.publicTitle ?? ""} onChange={(event) => planningDraft.editPlan(index, { publicTitle: event.target.value })} /></label>
            <label><span>必须兑现</span><input aria-label={`第 ${item.chapterNumber} 章公开承诺`} placeholder="以分号分隔" value={(item.publicCommitments ?? []).join("；")} onChange={(event) => planningDraft.editPlan(index, { publicCommitments: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            <label><span>可以自由发挥</span><input aria-label={`第 ${item.chapterNumber} 章创作余地`} placeholder="以分号分隔" value={(item.allowances ?? []).join("；")} onChange={(event) => planningDraft.editPlan(index, { allowances: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            {outlineNodes.length > 0 && <label><span>服务的上级规划</span><select value={item.primaryNodeId} onChange={(event) => planningDraft.editPlan(index, { primaryNodeId: event.target.value })}><option value="">请选择全书／卷／单元</option>{(outlineNodes).map((node) => <option key={node.id} value={node.id}>{node.id ? outlinePaths.get(node.id) : undefined}</option>)}</select></label>}
            <label><span>故事时间</span><input aria-label={`第 ${item.chapterNumber} 章故事时间`} type="number" min="0" placeholder="未指定" value={item.storyTime ?? ""} onChange={(event) => planningDraft.editPlan(index, { storyTime: event.target.value === "" ? null : Number(event.target.value) })} /></label>
            <label><span>依赖事实键</span><input aria-label={`第 ${item.chapterNumber} 章依赖事实`} placeholder="以逗号分隔" value={item.dependencyKeys} onChange={(event) => planningDraft.editPlan(index, { dependencyKeys: event.target.value })} /></label>
            <PlanningCharacterSelect label={`第 ${item.chapterNumber} 章视角人物`} value={item.povCharacterId ?? null}
              disabled={disabled} onChange={value => planningDraft.editPlan(index, { povCharacterId: value })} />
          </div></details>
          <details className="plan-writer-note"><summary>写正文时会看到</summary><div aria-label={`第 ${item.chapterNumber} 章正文可见内容`}><span>{item.publicTitle || "（无公开标题）"}</span><p>{item.writerGuidance || "（无公开写作指引）"}</p><small>{(item.publicCommitments ?? []).length > 0 ? `公开承诺：${item.publicCommitments?.join("；")}` : "无公开承诺"}</small></div></details>
          {(item.scenes?.length ?? 0) > 0 && <details className="plan-scenes"><summary>场景与节拍 · {item.scenes?.length}</summary>{item.scenes?.map((scene) => <article key={scene.id}><div><b>{scene.title || scene.id}</b>{scene.locked && <span>已锁定</span>}</div><p>{scene.goal}</p>{scene.beats.map((beat) => <small key={beat.id}>· {beat.description}{beat.locked ? " · 已锁定" : ""}</small>)}</article>)}</details>}
        </div>)}
        <button className="text-button" type="button" onClick={planningDraft.appendPlan}>＋ 添加下一章</button>
      </fieldset></PlanningCharacterProvider>;
}
