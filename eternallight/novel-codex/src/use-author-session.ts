import { useCallback, useEffect, useRef, useState } from "react";
import { type AuthorProfile, type AuthorSession } from "./author-session";

export function useAuthorSession() {
  const api = window.novelAuthorSession;
  const [snapshot, setSnapshot] = useState<AuthorSession>({ profile: null, signedIn: false });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(false);
  const mounted = useRef(false);
  const restoreEpoch = useRef(0);
  const apply = useCallback(async (operation: () => Promise<AuthorSession>) => {
    if (active.current) throw new Error("正在保存作者会话，请稍后再试。");
    active.current = true;
    try {
      const next = await operation();
      if (mounted.current) { setSnapshot(next); setError(null); }
    } finally { active.current = false; }
  }, []);
  const restore = useCallback(async () => {
    const epoch = ++restoreEpoch.current;
    setLoading(true); setError(null);
    try {
      if (!api) throw new Error("无法连接本机作者会话，请在桌面应用中打开。");
      const next = await api.read();
      if (mounted.current && restoreEpoch.current === epoch) setSnapshot(next);
    } catch (cause) {
      if (mounted.current && restoreEpoch.current === epoch) setError(cause instanceof Error ? cause.message : "无法恢复本机作者会话。");
    } finally { if (mounted.current && restoreEpoch.current === epoch) setLoading(false); }
  }, [api]);
  useEffect(() => {
    mounted.current = true;
    void restore();
    return () => { mounted.current = false; restoreEpoch.current++; };
  }, [restore]);
  return {
    author: snapshot.profile, signedIn: snapshot.signedIn, loading, error, restore,
    signIn: (profile: AuthorProfile) => apply(() => api!.save(profile)),
    signOut: () => apply(() => api!.signOut()),
    deleteAccount: () => apply(() => api!.clear()),
  };
}
