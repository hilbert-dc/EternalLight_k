/** Delete local, author-owned recovery state without touching saved works or another author. */
export function clearAuthorDrafts(authorKey: string, storage: Storage = localStorage) {
  const prefixes = ["novel-codex:conversation-draft:", "novel-codex:media-message:", "novel-codex:media-editor-draft:", "novel-codex:media-editor-view:", "novel-codex:selected-conversation:", "novel-codex:selected-edition:"];
  const owned: string[] = [];
  for (let index = 0; index < storage.length; index++) {
    const key = storage.key(index);
    if (!key) continue;
    const prefix = prefixes.find(value => key.startsWith(value));
    if (!prefix) continue;
    let identity = key.slice(prefix.length);
    if (prefix === "novel-codex:media-message:" && identity.endsWith(":images")) identity = identity.slice(0, -7);
    if (prefix === "novel-codex:media-editor-view:" && identity.endsWith(":playback")) identity = identity.slice(0, -9);
    try {
      const scope: unknown = JSON.parse(identity);
      if (!Array.isArray(scope) || typeof scope[0] !== "string") continue;
      let owner = scope[0];
      if (prefix === "novel-codex:selected-conversation:" && owner !== authorKey) {
        const mediaScope: unknown = JSON.parse(owner);
        if (Array.isArray(mediaScope) && typeof mediaScope[0] === "string") owner = mediaScope[0];
      }
      if (owner === authorKey) owned.push(key);
    } catch { /* Unrecognized state has no provable author ownership. */ }
  }
  for (const key of owned) storage.removeItem(key);
}
