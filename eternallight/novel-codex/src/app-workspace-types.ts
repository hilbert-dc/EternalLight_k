import type { NovelCodexApi } from "./types";
export type AgentStatus = { executionHealth?: import("./AgentStatus").ExecutionHealth[]; configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; reasoningEffort?: string | null; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null };
export type AgentConnection = Awaited<ReturnType<NovelCodexApi["agent"]["listConnections"]>>["connections"][number];
export type UserInputRequest = { projectId: string; executionId: string; requestId: string | number; method: "item/tool/requestUserInput"; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null; itemId?: string | null };
