import { useState } from "react";
import { authorInitial, type AuthorProfile } from "./author-session";

export function UserSettings({ profile, theme, onTheme, onSaveProfile, onChangePassword, onSignOut, onDeleteAccount }: { profile: AuthorProfile; theme: "light" | "dark"; onTheme: (theme: "light" | "dark") => void; onSaveProfile: (profile: Pick<AuthorProfile, "name" | "email">) => string | null; onChangePassword: (current: string, next: string) => Promise<string | null>; onSignOut: () => void; onDeleteAccount: () => void }) {
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

  function saveProfile() {
    const message = onSaveProfile({ name: name.trim(), email: email.trim() });
    setProfileError(message !== null);
    setProfileMessage(message ?? "作者资料已保存。");
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
      <div className="user-settings-actions"><button type="button" className="primary-button" onClick={saveProfile}>保存资料</button>{profileMessage && <small role="status" className={profileError ? "error" : ""}>{profileMessage}</small>}</div>
    </section>
    <section>
      <h3>外观</h3>
      <div className="theme-choice" role="radiogroup" aria-label="外观">
        <button type="button" role="radio" aria-checked={theme === "light"} className={theme === "light" ? "selected" : ""} onClick={() => onTheme("light")}>浅色</button>
        <button type="button" role="radio" aria-checked={theme === "dark"} className={theme === "dark" ? "selected" : ""} onClick={() => onTheme("dark")}>深色</button>
      </div>
    </section>
    <section>
      <h3>密码</h3>
      <label className="login-field"><span>当前密码</span><input type="password" value={currentPassword} autoComplete="current-password" onChange={(event) => { setCurrentPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <label className="login-field"><span>新密码</span><input type="password" value={nextPassword} autoComplete="new-password" onChange={(event) => { setNextPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <label className="login-field"><span>再输入一次新密码</span><input type="password" value={confirmPassword} autoComplete="new-password" onChange={(event) => { setConfirmPassword(event.target.value); setPasswordMessage(null); }} /></label>
      <div className="user-settings-actions"><button type="button" className="secondary-button" disabled={changingPassword} onClick={() => void changePassword()}>{changingPassword ? "正在更新…" : "更新密码"}</button>{passwordMessage && <small role="status" className={passwordError ? "error" : ""}>{passwordMessage}</small>}</div>
    </section>
    <section className="user-settings-danger">
      <h3>这台电脑</h3>
      <p>作者身份只保存在本机。退出后需要密码才能再进入。删除账号会清掉登录资料，作品文件仍留在本机。</p>
      <div className="user-settings-actions">
        <button type="button" className="secondary-button" onClick={onSignOut}>退出登录</button>
        {confirmDelete ? <button type="button" className="text-button danger" onClick={onDeleteAccount}>确认删除本机账号</button> : <button type="button" className="text-button danger" onClick={() => setConfirmDelete(true)}>删除本机账号</button>}
      </div>
    </section>
  </div>;
}
