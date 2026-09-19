# Codex App Server integration

Use this reference when implementing the bridge between Electron's main process and Codex.

## Transport and handshake

The local GUI should start:

```text
codex app-server
```

The default transport is newline-delimited JSON over stdin/stdout. On each connection:

```json
{"method":"initialize","id":0,"params":{"clientInfo":{"name":"my_electron_client","title":"My Electron Client","version":"0.1.0"}}}
{"method":"initialized","params":{}}
```

Then create or resume a thread and start a turn:

```json
{"method":"thread/start","id":1,"params":{"cwd":"/absolute/project/path"}}
{"method":"turn/start","id":2,"params":{"threadId":"thr_123","input":[{"type":"text","text":"Inspect this project."}]}}
```

The actual fields must be generated or checked against the app-server schema shipped with the selected Codex version. Do not hard-code examples as an alternative to schema validation.

## Notifications and requests

Route notifications by method and keep the raw message available for diagnostics without exposing it directly to the renderer. The initial adapter should cover:

- `thread/started`, `thread/status/changed`
- `turn/started`, `turn/completed`
- `item/started`, `item/completed`
- `item/agentMessage/delta`
- command execution and file-change progress
- `item/commandExecution/requestApproval`
- `item/fileChange/requestApproval`
- server errors and process termination

App-server can initiate JSON-RPC requests to ask the client for approval or user input. These are not ordinary notifications: preserve the request ID and send the matching response exactly once. Reject stale, duplicate, or unknown approval IDs.

## Adapter design

Keep a narrow adapter in the Electron main process:

- `start(workspace) -> Connection`
- `initialize() -> ServerCapabilities`
- `startThread(options) -> Thread`
- `resumeThread(threadId) -> Thread`
- `startTurn(threadId, input) -> Turn`
- `interruptTurn(threadId, turnId) -> void`
- `resolveApproval(requestId, decision) -> void`
- `dispose() -> void`

The adapter owns request IDs, pending promises, line parsing, backpressure, process lifecycle, and redacted diagnostics. The renderer receives typed domain events, not JSON-RPC plumbing.

## Compatibility

- Pin the Codex executable version used by the GUI.
- Generate TypeScript types with `codex app-server generate-ts --out ./schemas` for that version.
- Keep experimental API use behind an explicit capability flag and a compatibility module.
- Prefer local stdio for a desktop app. Use WebSocket only when a remote architecture is intentional and its authentication, TLS, retry, and lifecycle behavior are specified.
- Preserve the server's sandbox and approval semantics; the GUI is a client, not a replacement policy engine.

Primary reference: [Codex App Server documentation](https://learn.chatgpt.com/zh-Hans/docs/app-server)
