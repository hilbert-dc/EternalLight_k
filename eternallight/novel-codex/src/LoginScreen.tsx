import { useState, type FormEvent } from "react";
import { hashPassword, type AuthorProfile } from "./author-session";

export function LoginScreen({ existing, theme, onToggleTheme, onSignedIn }: { existing: AuthorProfile | null; theme: "light" | "dark"; onToggleTheme: () => void; onSignedIn: (profile: AuthorProfile) => Promise<void> }) {
  const creating = existing === null;
  const [name, setName] = useState(existing?.name ?? "");
  const [email, setEmail] = useState(existing?.email ?? "");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (submitting) return;
    const nextEmail = email.trim();
    const nextName = name.trim();
    if (creating && !nextName) { setError("请填写笔名。"); return; }
    if (!/^\S+@\S+\.\S+$/.test(nextEmail)) { setError("请填写有效邮箱。"); return; }
    if (password.length < 8) { setError("密码至少 8 位。"); return; }
    if (creating && password !== confirm) { setError("两次输入的密码不一致。"); return; }
    setSubmitting(true);
    setError(null);
    try {
      if (!creating && existing) {
        const passwordHash = await hashPassword(password, existing.passwordSalt);
        if (nextEmail.toLowerCase() !== existing.email.toLowerCase() || passwordHash !== existing.passwordHash) {
          setError("邮箱或密码不正确。");
          return;
        }
        await onSignedIn(existing);
        return;
      }
      const passwordSalt = crypto.randomUUID();
      await onSignedIn({ name: nextName, email: nextEmail, passwordSalt, passwordHash: await hashPassword(password, passwordSalt) });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "无法保存本机作者会话，请重试。");
    } finally {
      setSubmitting(false);
    }
  }

  return <div className="login-screen">
    <button type="button" className="icon-button login-theme" title={theme === "light" ? "切换深色" : "切换浅色"} aria-label={theme === "light" ? "切换深色" : "切换浅色"} onClick={onToggleTheme}>{theme === "light" ? "☾" : "☼"}</button>
    <form className="login-card" onSubmit={(event) => void submit(event)}>
      <div className="login-brand"><span className="brand-mark">✦</span><strong>小说工坊</strong></div>
      <h1>{creating ? "创建作者" : "登录"}</h1>
      <p>{creating ? "先建立一个只保存在这台电脑上的作者身份，然后进入作品。" : `继续以 ${existing.name} 进入这台电脑上的作品。`}</p>
      {creating && <label className="login-field"><span>笔名</span><input autoFocus value={name} maxLength={40} autoComplete="nickname" onChange={(event) => { setName(event.target.value); setError(null); }} /></label>}
      <label className="login-field"><span>邮箱</span><input autoFocus={!creating} type="email" value={email} autoComplete="username" onChange={(event) => { setEmail(event.target.value); setError(null); }} /></label>
      <label className="login-field"><span>密码</span><input type="password" value={password} autoComplete={creating ? "new-password" : "current-password"} onChange={(event) => { setPassword(event.target.value); setError(null); }} /></label>
      {creating && <label className="login-field"><span>再输入一次密码</span><input type="password" value={confirm} autoComplete="new-password" onChange={(event) => { setConfirm(event.target.value); setError(null); }} /></label>}
      {error && <p className="dialog-error" role="alert">{error}</p>}
      <button className="primary-button" type="submit" disabled={submitting}>{submitting ? "正在进入…" : creating ? "创建并进入" : "进入作品"}</button>
      <small>密码只在本机保存为摘要，不会明文留下，也不会发送出去。</small>
      <small>默认保持登录，关闭或重启应用无需再次输入密码。</small>
    </form>
  </div>;
}
