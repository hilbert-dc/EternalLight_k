import type { AgentRunSummary } from "./agent-events";
import type { ThreadDetail } from "./types";

export type RestoredAgentHistory = AgentRunSummary & { commentary: string };

// The host read contract supplies bounded recent turns in chronological order.
// This is presentation only: restoring history never infers or executes a task.
export function threadHistory(detail: ThreadDetail): RestoredAgentHistory[] {
  return detail.turns.flatMap(turn => {
    const prompt = turn.items.filter(item => item.type === "userMessage" && item.text).map(item => item.text).join("\n\n");
    const messages = turn.items.filter(item => item.type === "agentMessage");
    const finals = messages.filter(item => item.phase === "final_answer");
    const answer = (finals.length ? finals : messages.filter(item => item.phase === null)).at(-1)?.text ?? "";
    const commentary = messages.filter(item => item.phase === "commentary").map(item => item.text).join("\n\n");
    const hasAnswer = answer.trim().length > 0;
    if (!prompt && !hasAnswer && !commentary && !finals.length) return [];
    return [{ id: turn.id, threadId: detail.thread.id, turnId: turn.id, mode: "novel-agent-engine" as const, state: turn.status === "failed" ? "error" as const : turn.status === "interrupted" ? "cancelled" as const : "completed" as const,
      prompt, answer, commentary, summary: hasAnswer ? "" : finals.length ? "此回合最终回复为空。" : commentary ? "此回合只有过程记录，暂无最终回复。" : "此回合没有文本回复。", artifactCount: 0,
      createdAt: (turn.startedAt ?? turn.completedAt ?? 0) * 1000 }];
  }).reverse();
}
