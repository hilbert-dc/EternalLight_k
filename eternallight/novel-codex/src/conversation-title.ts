import type { ThreadSummary } from "./types";

export type ConversationTitleAuthor = {
  projectId: string;
  threadId: string | null;
  prompt: string;
};

// Engine preview contains composed model input, not an author-facing title.
export function conversationTitle(
  thread: Pick<ThreadSummary, "id" | "name">,
  projectId: string,
  author: ConversationTitleAuthor | null,
): string {
  const text = thread.name?.trim() || (projectId && author?.projectId === projectId
    && author.threadId === thread.id ? author.prompt.trim() : "");
  return Array.from(text.replace(/\s+/g, " ")).slice(0, 72).join("") || "创作对话";
}
