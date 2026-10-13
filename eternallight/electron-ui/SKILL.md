---
name: electron-ui
description: Build or modify secure cross-platform Electron desktop UIs, including main/renderer IPC, native OS integration, Codex app-server clients, packaging, and release verification.
---

# Electron UI development

Use this skill when building or modifying an Electron desktop application, especially a macOS/Windows Codex client. Keep the GUI as a separate client layer and preserve the existing Codex Rust backend unless the task explicitly requires changes to it.

## Architecture

- Main process: own application lifecycle, windows, menus, tray, dialogs, deep links, child processes, and privileged OS operations.
- Preload: expose a small, typed API with `contextBridge`; do not expose `ipcRenderer`, Node globals, or filesystem handles directly.
- Renderer: use React/TypeScript or the existing frontend stack for views only. Treat all data received from the renderer as untrusted in the main process.
- Codex integration: launch the version-pinned `codex app-server` as a child process and communicate over its default stdin/stdout JSONL transport. Do not scrape TUI output or reimplement Codex by calling the model API directly.

For this repository, the normal flow is:

```text
renderer -> typed preload API -> main process -> codex app-server -> Codex core/tools/sandbox
```

## Codex app-server lifecycle

1. Resolve the bundled `codex` executable for the current platform without assuming a Unix shell or a fixed absolute path.
2. Spawn `codex app-server` with an argument array and keep stdin/stdout/stderr separate.
3. Send exactly one `initialize` request, then the `initialized` notification.
4. Start or resume a thread with `thread/start` or `thread/resume`.
5. Send user input with `turn/start`; use `turn/steer` for input added to an active turn and `turn/interrupt` for cancellation.
6. Parse each JSONL message independently and route streamed notifications to the renderer. At minimum handle agent-message deltas, item start/completion, turn completion, errors, and server-initiated approval requests.
7. Render command execution and file-change approval requests as explicit user decisions. Never auto-approve because a request originated in the renderer.
8. Handle process exit, malformed messages, broken pipes, overloaded queues, and reconnect/resume states without losing the displayed transcript.

Generate protocol types from the exact Codex binary used by the application when possible:

```bash
codex app-server generate-ts --out ./schemas
```

Keep the generated schema version aligned with the bundled binary. Treat experimental app-server methods and fields as opt-in and isolate them behind a small compatibility layer.

## IPC rules

- Set `nodeIntegration: false`, `contextIsolation: true`, and `sandbox: true` where the application permits it.
- Use `ipcMain.handle`/`ipcRenderer.invoke` for request-response operations and narrow event channels for server notifications.
- Wrap each exposed operation (`codex.start`, `codex.sendTurn`, `codex.interrupt`, `project.choose`, `file.read`) instead of exposing generic `send`, `invoke`, `fs`, or `child_process` objects.
- Validate and normalize every IPC payload in the main process. Use explicit schemas for paths, thread IDs, turn IDs, approval decisions, and user text.
- Keep long-running app-server streams in the main process. Forward bounded, typed events to the renderer and clean up listeners when a window closes.
- Do not use synchronous IPC for Codex work, file access, or process management.

## Security requirements

- Never enable `nodeIntegration` in a renderer and never disable context isolation as a convenience.
- Do not load remote content into a privileged window. If a remote page is unavoidable, keep it in a separate restricted window with a strict navigation policy.
- Set a restrictive Content Security Policy and reject unexpected navigation, new-window requests, permissions, and external protocols.
- Keep API keys, OAuth tokens, cookies, and app-server credentials out of renderer state, URLs, logs, crash reports, and IPC error strings. Use OS-backed storage such as Electron `safeStorage` when local secret persistence is required.
- The renderer must not construct shell commands. Pass structured arguments to the main process and use `spawn`-style APIs without shell interpolation.
- Preserve Codex sandbox and approval boundaries. A GUI convenience feature must not silently widen filesystem or network access.

See [references/electron-security.md](references/electron-security.md) for the security checklist and [references/codex-app-server.md](references/codex-app-server.md) for the protocol boundary.

## Cross-platform behavior

- Use platform-aware path and executable resolution APIs; do not concatenate paths or assume `/tmp`, `/bin/sh`, or POSIX quoting on Windows.
- Pass child-process arguments as an array. Test spaces, Unicode, UNC paths, and long paths on Windows.
- Keep macOS deep-link registration and Windows protocol registration in the packaged application manifest. Validate `codex://` URLs before using their workspace path.
- Treat macOS signing/notarization and Windows code signing as release requirements, not post-release fixes.
- Pin Electron, Node, and packaging-tool versions. Build and smoke-test both platforms in CI; do not treat a successful renderer build as proof that the packaged application works.

## UI and product behavior

- Make the current project, thread, turn, sandbox mode, and approval state visible.
- Preserve streamed text and tool progress when the user switches views.
- Show a clear distinction between proposed changes, applied changes, command output, and errors.
- Disable duplicate submissions while a request is being acknowledged, but allow cancellation and recovery.
- Keep renderer state derived from typed app-server events rather than duplicating authoritative thread state in multiple components.

## Verification

Before handing off an Electron change:

1. Run the repository's formatter, lint, typecheck, unit tests, and frontend build using the package manager already present.
2. Run an integration test with a fake app-server that emits normal events, approval requests, malformed input, and an unexpected exit.
3. Run a packaged smoke test that opens a workspace, starts a thread, streams a turn, interrupts it, and exits cleanly.
4. Verify that renderer code has no direct imports of `electron`, `fs`, `child_process`, or secrets.
5. Verify macOS and Windows installers, deep links, code-signing configuration, and updater configuration separately.

Do not add broad Electron abstractions to Codex core. Keep Electron-specific code in the GUI project and communicate with Codex through app-server or a narrowly defined adapter.
