// UI projection only. The original Codex history/context is never rewritten.
export function presentAuthorHistory(detail, authored) {
  if (authored?.threadId !== detail.thread.id || !Array.isArray(authored.messages)) throw new Error("作者对话记录与线程不匹配");
  const messages = new Map(authored.messages.map(message => [message.turnId, message]));
  return { ...detail, turns: detail.turns.map(turn => {
    const message = messages.get(turn.id), text = message?.text;
    const mediaAttachments = Array.isArray(message?.mediaAttachments) ? message.mediaAttachments.slice(0, 5).map(({ assetRevisionId, name, previewUrl }) => ({ assetRevisionId, name, previewUrl })) : [];
    const items = turn.items.filter(item => item.type !== "userMessage" && item.type !== "reasoning")
      .map(({ id, type, status, phase, text }) => Object.fromEntries(
        Object.entries({ id, type, status, phase, text }).filter(([, value]) => value !== undefined)));
    if (typeof text === "string" && (text || mediaAttachments.length)) items.unshift({ id: `author:${turn.id}`, type: "userMessage", status: "completed", phase: null, text,
      ...(mediaAttachments.length ? { mediaAttachments } : {}) });
    return { ...turn, items };
  }) };
}

/** Only Runtime's turn-bound immutable asset IDs may become previews. Never
 * project embedded provider URLs or base64 from the model's original history. */
export async function enrichAuthorHistoryImages(authored, { projectId, readAsset, rememberAsset }) {
  if (!Array.isArray(authored?.messages)) throw new Error("作者对话记录无效");
  const messages = await Promise.all(authored.messages.map(async message => {
    const ids = message.imageAssetRevisionIds ?? [];
    if (!Array.isArray(ids) || ids.length > 5 || ids.some(id => typeof id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(id))) throw new Error("作者图片引用无效");
    const mediaAttachments = await Promise.all(ids.map(async assetRevisionId => {
      const asset = await readAsset(assetRevisionId);
      if (asset.assetRevisionId !== assetRevisionId || asset.projectId !== projectId || asset.kind !== "image") throw new Error("作者图片不属于当前作品");
      const preview = rememberAsset(asset);
      return { assetRevisionId, name: asset.name, previewUrl: preview.previewUrl };
    }));
    return { ...message, mediaAttachments };
  }));
  return { ...authored, messages };
}
