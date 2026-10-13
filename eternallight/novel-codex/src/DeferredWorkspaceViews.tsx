import { lazy, Suspense, type ComponentProps } from "react";

const Media = lazy(() => import("./multimedia/MediaApplication").then(module => ({ default: module.MediaApplication })));
const Settings = lazy(() => import("./UserSettings").then(module => ({ default: module.UserSettings })));
const Diagnostics = lazy(() => import("./WorkspaceDiagnostics").then(module => ({ default: module.WorkspaceDiagnostics })));
function Loading({ children }: { children: string }) { return <p role="status" aria-live="polite">{children}</p>; }

// Boundaries surround only the requested region. Loading a panel never suspends
// the App's active conversation, draft storage or execution subscriptions.
export function MediaApplication(props: ComponentProps<typeof Media>) {
  return <Suspense fallback={<Loading>正在打开作品版本…</Loading>}><Media {...props} /></Suspense>;
}
export function UserSettings(props: ComponentProps<typeof Settings>) {
  return <Suspense fallback={<Loading>正在打开用户设置…</Loading>}><Settings {...props} /></Suspense>;
}
export function WorkspaceDiagnostics(props: ComponentProps<typeof Diagnostics>) {
  return <Suspense fallback={<Loading>正在读取诊断界面…</Loading>}><Diagnostics {...props} /></Suspense>;
}
