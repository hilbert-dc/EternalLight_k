import { spawn } from "node:child_process";

// Only the OS startup boundary is delayed; the actual Rust MCP serves all IO.
const [delay, command, ...args] = process.argv.slice(2);
if (delay === "fail") { console.error("FIXTURE_MCP_STARTUP_FAILURE"); process.exit(78); }
const delayMs = Number(delay);
if (!Number.isSafeInteger(delayMs) || delayMs < 0 || delayMs > 40_000 || !command) process.exit(64);
console.error(`FIXTURE_MCP_STARTUP_DELAY:${delayMs}`);
await new Promise(resolve => setTimeout(resolve, delayMs));
const child = spawn(command, args, { stdio: "inherit" });
child.once("error", () => process.exit(70));
child.once("exit", (code) => process.exit(code ?? 1));
for (const signal of ["SIGTERM", "SIGINT"]) process.once(signal, () => child.kill(signal));
