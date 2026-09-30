import type { DesiredOutcome } from "./types";

export function DesiredOutcomes({ outcomes, approved = false }: { outcomes: DesiredOutcome[] | undefined; approved?: boolean }) {
  if (!outcomes?.length) return null;
  return <section aria-label={approved ? "已批准的修改标准" : "待批准的修改标准"}>
    <strong>{approved ? "已批准的修改标准" : "模型提出的修改标准"}</strong>
    <p>{approved ? "以下标准来自已冻结的任务目标。" : "批准操作或确认范围时，将一并冻结以下标准与适用场景。"}</p>
    <ul>{outcomes.map(outcome => <li key={outcome.id}>
      <p>{outcome.criterion}</p><small>标准 {outcome.id} · 场景 {outcome.groupIds.join("、")}</small>
    </li>)}</ul>
  </section>;
}
