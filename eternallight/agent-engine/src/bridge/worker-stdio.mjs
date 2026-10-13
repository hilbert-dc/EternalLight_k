/** Worker-side binding for the native parent's bounded JSONL transport. */
export function workerChannel(onMessage, onClose = () => process.exit(0)) {
  let pending = Buffer.alloc(0);
  let closed = false;
  const fail = () => { closed = true; process.exit(1); };
  process.stdin.on("data", chunk => {
    if (closed) return;
    pending = Buffer.concat([pending, chunk]);
    let newline;
    while ((newline = pending.indexOf(10)) >= 0) {
      if (newline > 2 * 1024 * 1024) return fail();
      const line = pending.subarray(0, newline);
      pending = pending.subarray(newline + 1);
      try { onMessage(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(line))); }
      catch { return fail(); }
    }
    if (pending.length > 2 * 1024 * 1024) fail();
  });
  process.stdin.once("end", () => { closed = true; onClose(); });
  process.stdin.once("error", fail);
  process.stdout.once("error", fail);
  return (message, callback) => {
    if (closed) { callback?.(new Error("bridge parent disconnected")); return; }
    const line = `${JSON.stringify(message)}\n`;
    if (Buffer.byteLength(line) > 256 * 1024 || process.stdout.writableLength > 8 * 1024 * 1024) return fail();
    process.stdout.write(line, callback);
  };
}
