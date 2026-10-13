const key = (author: string, projectId: string) => `novel-codex:selected-edition:${JSON.stringify([author, projectId])}`;
export function readSelectedEdition(author: string, projectId: string) { try { return localStorage.getItem(key(author, projectId)); } catch { return null; } }
export function rememberSelectedEdition(author: string, projectId: string, editionId: string) { try { localStorage.setItem(key(author, projectId), editionId); } catch { /* Current selection remains usable. */ } }
