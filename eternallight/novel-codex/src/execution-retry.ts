import type { ConversationFocus } from "./types";
import type { Chapter, ViewId } from "./types";

// A retry is bound to one execution, not inferred from its display prompt.
// Domain operations must use persisted task recovery, where source revisions,
// partial writes and frozen inputs are checked before any model call.
export type ExecutionRetry =
  | { kind: "discussion"; prompt: string; conversationFocus?: ConversationFocus | null; threadId?: string | null }
  | { kind: "taskRecovery"; operation: string; view: ViewId;
      chapterId?: Chapter["id"]; intent?: string };

export type BoundExecutionRetry = {
  projectId: string;
  scope: object;
  history: object | null;
  execution: ExecutionRetry;
};
