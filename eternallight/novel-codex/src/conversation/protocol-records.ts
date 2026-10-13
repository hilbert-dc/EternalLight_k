export function isResolvedUserInputEvent(event: unknown): event is { method: "serverRequest/resolved"; params: { requestId: string | number } } {
  if (event === null || typeof event !== "object") return false;
  const message = event as { method?: unknown; params?: { requestId?: unknown } };
  return message.method === "serverRequest/resolved" && (typeof message.params?.requestId === "string" || typeof message.params?.requestId === "number");
}

export function eventRecord(value: unknown): Record<string, unknown> | null { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null; }
export function eventText(value: unknown): string | null { return typeof value === "string" && value.trim() ? value.slice(0, 240) : null; }
export function eventThreadId(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const thread = eventRecord(params?.thread); const id = typeof params?.threadId === "string" ? params.threadId : typeof thread?.id === "string" ? thread.id : null; return id && id.trim() && id.length <= 240 && !/[\r\n]/.test(id) ? id : null; }

