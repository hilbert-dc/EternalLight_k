import type { AuthorProfile, AuthorSession } from "../../../src/author-session";

// Renderer-only fixtures implement the same asynchronous desktop contract.
// Native persistence and preload are exercised separately in new processes.
let snapshot: AuthorSession = { profile: null, signedIn: false };
window.novelAuthorSession = {
  read: async () => structuredClone(snapshot),
  save: async profile => { saveAuthor(profile); return structuredClone(snapshot); },
  signOut: async () => { markSignedOut(); return structuredClone(snapshot); },
  clear: async () => { clearAuthor(); return structuredClone(snapshot); },
};
export function saveAuthor(profile: AuthorProfile) { snapshot = { profile, signedIn: true }; }
export function markSignedOut() { snapshot = { ...snapshot, signedIn: false }; }
export function clearAuthor() { snapshot = { profile: null, signedIn: false }; }
