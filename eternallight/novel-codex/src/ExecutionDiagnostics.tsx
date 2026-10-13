import type { AgentRun, AgentStep } from "./agent-events";
import { isDiagnosticExecutionStep } from "./conversation-process";
import "./conversation-presentation.css";

const statusLabels = { running: "进行中", waiting: "等待中", completed: "已完成", failed: "失败", cancelled: "已停止" };

function Records({ steps }: { steps: AgentStep[] }) {
  const records = steps.filter(isDiagnosticExecutionStep);
  return <>{records.slice(-96).map(step => <details className="execution-diagnostic-record" key={step.id}>
    <summary>{step.title} · {statusLabels[step.status]}</summary>
    {step.detail && <pre>{step.detail}</pre>}
  </details>)}{records.length > 96 && <p className="inspector-note">此处仅显示最近 96 条记录。</p>}</>;
}

export function ExecutionDiagnostics({ run }: { run: Pick<AgentRun, "steps" | "history"> }) {
  const currentCount = run.steps.filter(isDiagnosticExecutionStep).length;
  const history = run.history.filter(entry => entry.displayExecution?.steps.some(isDiagnosticExecutionStep));
  return <section className="execution-diagnostics" aria-label="执行诊断记录">
    <p className="inspector-note">用于排查任务运行问题。仅显示已保留的过程记录，日常写作无需查看。</p>
    <details><summary>本轮运行记录 · {currentCount} 条</summary>
      {currentCount ? <Records steps={run.steps} /> : <p className="inspector-note">本轮暂无运行记录。</p>}
    </details>
    {history.map((entry, index) => <details key={entry.id}>
      <summary>较早记录 {index + 1} · {entry.state === "completed" ? "已完成" : entry.state === "cancelled" ? "已停止" : "未完成"}</summary>
      <Records steps={entry.displayExecution!.steps} />
      {(entry.displayExecution!.detailsTruncated || entry.displayExecution!.totalPublicSteps > entry.displayExecution!.steps.length)
        && <p className="inspector-note">此轮部分过程记录未保留。</p>}
    </details>)}
  </section>;
}
