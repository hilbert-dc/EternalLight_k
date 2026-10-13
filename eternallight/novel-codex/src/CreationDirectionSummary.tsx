import type { CreativeWorkspace } from "./types";

const fields = [
  ["premise", "核心构想"], ["promise", "阅读体验"],
  ["stakes", "核心代价"], ["endingDirection", "结局方向"],
] as const;

export function CreationDirectionSummary({ artifact }: { artifact: CreativeWorkspace["artifacts"]["brief"] | undefined }) {
  const current = artifact?.working ?? artifact?.accepted;
  const shown = current ?? artifact?.candidate;
  const rows = fields.flatMap(([key, label]) => {
    const value = shown?.payload?.[key];
    return typeof value === "string" && value.trim() ? [{ key, label, value }] : [];
  });
  return <div className="overview-direction-summary" data-discussion-revision={shown?.id ?? ""}>
    <h2>创作方向</h2>
    {shown && <p className="overview-source">{shown.id === artifact?.accepted?.id ? "已确认方向" : current ? "当前后续创作依据 · 尚未正式采用" : "已保存的方向建议 · 尚未确认"}</p>}
    {rows.length ? <dl>{rows.map(row => <div key={row.key}><dt>{row.label}</dt><dd>{row.value}</dd></div>)}</dl>
      : <p>还没有确定创作方向。可以先在对话中聊聊构想，也可以直接开始写作。</p>}
    {current && artifact?.candidate && artifact.candidate.id !== current.id && <p className="overview-source">另有新的方向建议，展开“编辑创作方向”查看。讨论和建议不会自动替换当前方向。</p>}
  </div>;
}
