// UI projection only. The original Codex history/context is never rewritten.
export function presentAuthorHistory(detail, authored) {
  if (authored?.threadId !== detail.thread.id || !Array.isArray(authored.messages)) throw new Error("作者对话记录与线程不匹配");
  const messages = new Map(authored.messages.map(message => [message.turnId, message.text]));
  return { ...detail, turns: detail.turns.map(turn => {
    const text = messages.get(turn.id);
    const items = turn.items.filter(item => item.type !== "userMessage" && item.type !== "reasoning")
      .map(({ id, type, status, phase, text }) => Object.fromEntries(
        Object.entries({ id, type, status, phase, text }).filter(([, value]) => value !== undefined)));
    if (typeof text === "string" && text) items.unshift({ id: `author:${turn.id}`, type: "userMessage", status: "completed", phase: null, text });
    return { ...turn, items };
  }) };
}
