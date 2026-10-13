# Electron security checklist

Use this reference when adding a window, preload API, navigation rule, external content, or native capability.

## BrowserWindow baseline

```ts
new BrowserWindow({
  webPreferences: {
    preload: PRELOAD_PATH,
    nodeIntegration: false,
    contextIsolation: true,
    sandbox: true,
  },
});
```

If a capability cannot work with the sandbox, isolate that capability in the main process or a dedicated utility process and document the exception. Do not enable Node integration in the renderer to avoid designing an IPC boundary.

## Preload boundary

Expose named operations, not Electron primitives:

```ts
contextBridge.exposeInMainWorld("codex", {
  startThread: (input: StartThreadInput) => ipcRenderer.invoke("codex:start-thread", input),
  sendTurn: (input: SendTurnInput) => ipcRenderer.invoke("codex:send-turn", input),
  onEvent: (listener: (event: CodexEvent) => void) => {
    const handler = (_event: IpcRendererEvent, payload: CodexEvent) => listener(payload);
    ipcRenderer.on("codex:event", handler);
    return () => ipcRenderer.removeListener("codex:event", handler);
  },
});
```

The main process must validate `StartThreadInput`, `SendTurnInput`, and every approval decision again. TypeScript types are not runtime validation.

## Navigation and content

- Load only packaged local UI in the privileged application window.
- Allow navigation only to known application URLs.
- Deny unexpected `window.open`, redirects, permission requests, and protocol handlers.
- Open external links with an explicit allowlist and the system browser.
- Add a restrictive CSP that permits only the resources required by the bundled UI.

## Codex-specific boundaries

- Keep the app-server child process and its stdout parser in the main process.
- Do not pass raw shell strings from the renderer to `spawn`, `exec`, or a terminal.
- Treat workspace paths, MCP responses, command output, and file contents as untrusted data when rendering them.
- Do not log authorization headers, API keys, OAuth callbacks, or full environment variables.
- Require a visible approval action for server requests that can execute commands or modify files.

Primary references:

- [Electron security](https://www.electronjs.org/docs/latest/tutorial/security)
- [Electron process sandboxing](https://www.electronjs.org/docs/latest/tutorial/sandbox)
- [Electron process model](https://www.electronjs.org/docs/latest/tutorial/process-model)
