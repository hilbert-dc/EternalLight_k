import type { TimelineRun } from "./conversation-process";
import { collaborationProgress } from "./collaboration-progress";
import "./collaboration-progress.css";

const labels: Record<string, string> = { pendingInit: "准备中", running: "进行中", completed: "已完成",
  errored: "未完成", interrupted: "已停止", shutdown: "已关闭", notFound: "未找到", unknown: "状态未确认", turnStopped: "本轮已停止" };

export function CollaborationProgress({ run }: { run: TimelineRun }) {
  const agents = collaborationProgress(run);
  if (!agents.length) return null;
  return <div className="collaboration-progress" aria-label="协作任务">
    {agents.map((agent, index) => {
      const name = agent.task?.split(/\r?\n/)[0] || `协作任务 ${index + 1}`;
      const moving = agent.status === "running" || agent.status === "pendingInit";
      return <details key={agent.id} className="collaboration-row" data-conversation-detail={`collaboration:${run.turnId}:${agent.id}`}>
        <summary>
          <span className={`collaboration-symbol${moving ? " is-active" : ""}`} aria-hidden="true">{agent.status === "completed" ? "✓" : agent.status === "errored" ? "!" : "◌"}</span>
          <span className="collaboration-name" title={name}>{name}</span>
          <span className="collaboration-status">{labels[agent.status] ?? labels.unknown}</span>
        </summary>
        <div className="collaboration-detail">
          {agent.task && <p>{agent.task}</p>}
          {agent.result ? <p>{agent.result}</p> : <p className="collaboration-empty">{agent.status === "completed" ? "任务已完成，未返回文字摘要。" : "暂无返回结果。"}</p>}
        </div>
      </details>;
    })}
  </div>;
}
