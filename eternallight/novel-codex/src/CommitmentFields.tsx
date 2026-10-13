import type { CommitmentEvidence, NarrativeCommitment } from "./types";

export type CommitmentSource = { chapterId: string; title: string; objectId: string; revisionId: string; body: string };
export type CommitmentChapter = { id: string; title: string };
export function CommitmentFields({ value, sources, chapters, disabled, onChange }: {
  value: NarrativeCommitment; sources: CommitmentSource[]; chapters: CommitmentChapter[]; disabled: boolean; onChange: (value: NarrativeCommitment) => void;
}) {
  function anchors(field: "setupEvidenceRefs" | "fulfillmentEvidenceRefs", label: string) {
    const items = value[field];
    const update = (index: number, next: CommitmentEvidence) => onChange({ ...value, [field]: items.map((item, position) => position === index ? next : item) });
    return <section className="commitment-anchors"><strong>{label}</strong>{items.map((anchor, index) => {
      const selected = sources.find((source) => source.objectId === anchor.objectId && source.revisionId === anchor.revisionId);
      const exact = selected && Array.from(selected.body).slice(anchor.start, anchor.end).join("") === anchor.quote;
      return <div className="commitment-anchor" key={index}>
        <label>正文版本<select aria-label={`${label} ${index + 1} 来源`} value={selected?.revisionId ?? "historical"} onChange={(event) => {
          const source = sources.find((item) => item.revisionId === event.target.value);
          if (source) update(index, { ...anchor, objectId: source.objectId, revisionId: source.revisionId });
        }}>
          {!selected && <option value="historical">原证据版本 · {anchor.revisionId}（不自动重绑）</option>}
          {sources.map((source) => <option key={source.revisionId} value={source.revisionId}>{source.title} · {source.revisionId}</option>)}
        </select></label>
        <label>原文引用<textarea aria-label={`${label} ${index + 1} 引用`} value={anchor.quote} onChange={(event) => update(index, { ...anchor, quote: event.target.value, end: anchor.start + Array.from(event.target.value).length })} /></label>
        <label>起始字符<input aria-label={`${label} ${index + 1} 起始字符`} type="number" min="0" step="1" value={anchor.start} onChange={(event) => {
          const start = Number(event.target.value);
          update(index, { ...anchor, start, end: start + Array.from(anchor.quote).length });
        }} /></label>
        <small>{exact ? "引用与所选原文精确匹配" : "尚未匹配当前原文，保存时仍需领域校验"} · {anchor.start}–{anchor.end}</small>
        <button type="button" className="text-button" onClick={() => onChange({ ...value, [field]: items.filter((_, position) => position !== index) })}>移除{label} {index + 1}</button>
      </div>;
    })}<button type="button" className="secondary-button" disabled={items.length >= 8 || !sources.length} onClick={() => {
      const source = sources[0];
      if (source) onChange({ ...value, [field]: [...items, { objectId: source.objectId, revisionId: source.revisionId, quote: "", start: 0, end: 0 }] });
    }}>添加{label}</button></section>;
  }
  return <fieldset className="commitment-fields" disabled={disabled}>
    <legend>调整独立承诺</legend>
    <label>类型<select aria-label="承诺类型" value={value.kind} onChange={(event) => onChange({ ...value, kind: event.target.value as NarrativeCommitment["kind"] })}>
      <option value="foreshadowing">伏笔</option><option value="suspense">悬念</option><option value="characterTransformation">人物转变</option><option value="goal">目标</option><option value="setPiece">关键场面</option>
    </select></label>
    <label>摘要<textarea aria-label="承诺摘要" value={value.summary} onChange={(event) => onChange({ ...value, summary: event.target.value })} /></label>
    <label>兑现标准<textarea aria-label="承诺兑现标准" value={value.fulfillmentCriteria} onChange={(event) => onChange({ ...value, fulfillmentCriteria: event.target.value })} /></label>
    <label>状态<select aria-label="承诺状态" value={value.status} onChange={(event) => onChange({ ...value, status: event.target.value as NarrativeCommitment["status"] })}>
      <option value="open">开放</option><option value="partiallyFulfilled">部分兑现</option><option value="fulfilled">已兑现</option><option value="abandoned">已放弃</option>
    </select></label>
    <label>作者理由<textarea aria-label="承诺作者理由" value={value.decisionReason ?? ""} onChange={(event) => onChange({ ...value, decisionReason: event.target.value || null })} /></label>
    <p>部分／全部兑现需要正文证据；放弃需要理由。改为开放不会自动删除原兑现证据，请明确移除。</p>
    <label>关联实体 ID（逗号分隔）<input aria-label="承诺关联实体" maxLength={5120} value={value.relatedEntityIds.join(",")} onChange={(event) => onChange({ ...value, relatedEntityIds: event.target.value ? event.target.value.split(",") : [] })} /></label>
    <label>线索 ID<input aria-label="承诺线索" value={value.plotThreadId ?? ""} onChange={(event) => onChange({ ...value, plotThreadId: event.target.value || null })} /></label>
    <label>人物弧 ID<input aria-label="承诺人物弧" value={value.characterArcId ?? ""} onChange={(event) => onChange({ ...value, characterArcId: event.target.value || null })} /></label>
    <label>目标窗口<select aria-label="承诺目标窗口" value={value.targetWindow.kind} onChange={(event) => {
      const first = chapters[0]?.id;
      if (event.target.value === "open") onChange({ ...value, targetWindow: { kind: "open" } });
      else if (first) onChange({ ...value, targetWindow: { kind: "chapterRange", fromChapterId: first, throughChapterId: first } });
    }}><option value="open">不指定章范围</option><option value="chapterRange" disabled={!chapters.length}>指定章范围</option></select></label>
    {value.targetWindow.kind === "chapterRange" && (["fromChapterId", "throughChapterId"] as const).map((field) => {
      const window = value.targetWindow;
      if (window.kind !== "chapterRange") return null;
      return <label key={field}>{field === "fromChapterId" ? "起始章" : "结束章"}<select aria-label={`承诺${field === "fromChapterId" ? "起始章" : "结束章"}`} value={window[field]} onChange={(event) => onChange({ ...value, targetWindow: { ...window, [field]: event.target.value } })}>
        {!chapters.some((chapter) => chapter.id === window[field]) && <option value={window[field]}>原章节 · {window[field]}</option>}
        {chapters.map((chapter) => <option key={chapter.id} value={chapter.id}>{chapter.title}</option>)}
      </select></label>;
    })}
    {anchors("setupEvidenceRefs", "建立证据")}{anchors("fulfillmentEvidenceRefs", "兑现证据")}
  </fieldset>;
}
