import { useState, type ReactNode } from "react";
import { authorInitial, type AuthorProfile } from "./author-session";
import "./account-layout.css";

export function UserSettings({ profile, theme, diagnostics, onTheme, onSaveProfile, onChangePassword, onSignOut, onDeleteAccount }: { profile: AuthorProfile; theme: "light" | "dark"; diagnostics?: ReactNode; onTheme: (theme: "light" | "dark") => void; onSaveProfile: (profile: Pick<AuthorProfile, "name" | "email">) => Promise<string | null>; onChangePassword: (current: string, next: string) => Promise<string | null>; onSignOut: () => Promise<void>; onDeleteAccount: () => Promise<void> }) {
  const [name, setName] = useState(profile.name);
  const [email, setEmail] = useState(profile.email);
  const [profileMessage, setProfileMessage] = useState<string | null>(null);
  const [profileError, setProfileError] = useState(false);
  const [currentPassword, setCurrentPassword] = useState("");
  const [nextPassword, setNextPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordMessage, setPasswordMessage] = useState<string | null>(null);
  const [passwordError, setPasswordError] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [changingPassword, setChangingPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const [accountError, setAccountError] = useState<string | null>(null);
  const busy = saving || changingPassword;

  async function saveProfile() {
    setSaving(true);
    try {
      const message = await onSaveProfile({ name: name.trim(), email: email.trim() });
      setProfileError(message !== null);
      setProfileMessage(message ?? "作者资料已保存。");
    } catch (cause) {
      setProfileError(true); setProfileMessage(cause instanceof Error ? cause.message : "无法保存作者资料。");
    } finally { setSaving(false); }
  }

  async function accountAction(action: () => Promise<void>) {
    setSaving(true); setAccountError(null);
    try { await action(); }
    catch (cause) { setAccountError(cause instanceof Error ? cause.message : "无法保存本机作者会话。"); }
    finally { setSaving(false); }
  }

  async function changePassword() {
    if (nextPassword.length < 8) { setPasswordError(true); setPasswordMessage("新密码至少 8 位。"); return; }
    if (nextPassword !== confirmPassword) { setPasswordError(true); setPasswordMessage("两次输入的新密码不一致。"); return; }
    setChangingPassword(true);
    setPasswordMessage(null);
    setPasswordError(false);
    try {
      const message = await onChangePassword(currentPassword, nextPassword);
      setPasswordError(message !== null);
      setPasswordMessage(message ?? "密码已更新。");
      if (!message) { setCurrentPassword(""); setNextPassword(""); setConfirmPassword(""); }
    } catch (cause) {
      setPasswordError(true); setPasswordMessage(cause instanceof Error ? cause.message : "无法保存新密码。");
    } finally {
      setChangingPassword(false);
    }
  }

  return <div className="user-settings">
    <header className="user-settings-heading">
      <span className="avatar" aria-hidden="true">{authorInitial(profile.name)}</span>
      <div><h2>{profile.name}</h2><p>{profile.email}</p></div>
    </header>
    <section>
      <h3>作者资料</h3>
      <label className="login-field"><span>笔名</span><input value={name} maxLength={40} onChange={(event) => { setName(event.target.value); setProfileMessage(null); setProfileError(false); }} /></label>
      <label className="login-field"><span>邮箱</span><input type="email" value={email} onChange={(event) => { setEmail(event.target.value); setProfileMessage(null); setProfileError(false); }} /></label>
      <div className="user-settings-actions"><button type="button" className="primary-button" disabled={busy} onClick={() => void saveProfile()}>保存资料</button>{profileMessage && <small role="status" className={profileError ? "error" : ""}>{profileMessage}</small>}</div>
    </section>
    <section>
      <h3>外观</h3>
      <div className="theme-choice" role="radiogroup" aria-label="外观">
        <label><input type="radio" name="appearance" value="light" checked={theme === "light"} onChange={() => onTheme("light")} /><span>浅色</span></label>
        <label><input type="radio" name="appearance" value="dark" checked={theme === "dark"} onChange={() => onTheme("dark")} /><span>深色</span></label>
      </div>
    </section>
    <section>
      <h3>密码</h3>
      <label className="login-field"><span>当前密码</span><input type="password" value={currentPassword} autoComplete="current-password" onChange={(event) => { setCurrentPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <label className="login-field"><span>新密码</span><input type="password" value={nextPassword} autoComplete="new-password" onChange={(event) => { setNextPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <label className="login-field"><span>再输入一次新密码</span><input type="password" value={confirmPassword} autoComplete="new-password" onChange={(event) => { setConfirmPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <div className="user-settings-actions"><button type="button" className="secondary-button" disabled={busy} onClick={() => void changePassword()}>{changingPassword ? "正在更新…" : "更新密码"}</button>{passwordMessage && <small role="status" className={passwordError ? "error" : ""}>{passwordMessage}</small>}</div>
    </section>
    {diagnostics && <section aria-label="当前作品诊断">
      <h3>高级排障</h3>
      {diagnostics}
    </section>}
    <section className="user-settings-danger">
      <h3>这台电脑</h3>
      <p>作者身份只保存在本机，关闭或重启应用保持登录。只有主动退出后才需要密码重新进入。删除账号会清掉登录资料，作品文件仍留在本机。</p>
      <div className="user-settings-actions">
        <button type="button" className="secondary-button" disabled={busy} onClick={() => void accountAction(onSignOut)}>退出登录</button>
        {confirmDelete ? <button type="button" className="text-button danger" disabled={busy} onClick={() => void accountAction(onDeleteAccount)}>确认删除本机账号</button> : <button type="button" className="text-button danger" disabled={busy} onClick={() => setConfirmDelete(true)}>删除本机账号</button>}
      </div>
      {accountError && <p className="dialog-error" role="alert">{accountError}</p>}
    </section>
  </div>;
}
