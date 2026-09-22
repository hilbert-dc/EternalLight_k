import { nativeBinding } from "./native.mjs";

/** Fetch-shaped boundary; Rust owns sockets, deadlines, byte caps and backpressure. */
export async function nativeFetch(url, { method = "GET", headers = {}, body = "", signal, timeoutMs = 120_000, maxBytes = 32 * 1024 * 1024 } = {}) {
  signal?.throwIfAborted();
  const transport = new (nativeBinding().NativeHttpTransport)(JSON.stringify({ url: String(url), method, headers: Object.fromEntries(new Headers(headers)), body, timeoutMs, maxBytes }));
  let closed = false;
  let streamController;
  const cleanup = () => {
    closed = true;
    signal?.removeEventListener("abort", abort);
    transport.cancel();
  };
  const abort = () => {
    cleanup();
    streamController?.error(signal.reason ?? new DOMException("HTTP request cancelled", "AbortError"));
  };
  signal?.addEventListener("abort", abort, { once: true });
  const next = async () => {
    for (;;) {
      signal?.throwIfAborted();
      if (closed) throw new DOMException("HTTP request cancelled", "AbortError");
      const event = transport.poll();
      if (event?.kind === "error") throw Object.assign(new Error(event.error), { name: event.error === "engine HTTP request timed out" ? "TimeoutError" : "Error" });
      if (event) return event;
      await new Promise(resolve => setTimeout(resolve, 4));
    }
  };
  try {
    const head = await next();
    if (head.kind !== "headers") throw new Error("engine HTTP response headers missing");
    const responseBody = new ReadableStream({
      start(controller) { streamController = controller; },
      async pull(controller) {
        try {
          const event = await next();
          if (event.kind === "end") { cleanup(); controller.close(); }
          else if (event.kind === "chunk") controller.enqueue(event.chunk);
          else throw new Error("invalid engine HTTP response event");
        } catch (error) { cleanup(); controller.error(error); }
      },
      cancel() { cleanup(); },
    }, { highWaterMark: 0 });
    const responseHeaders = new Headers(JSON.parse(head.headers));
    if ([204, 205, 304].includes(head.status) || method === "HEAD") {
      await responseBody.cancel();
      return new Response(null, { status: head.status, headers: responseHeaders });
    }
    return new Response(responseBody, { status: head.status, headers: responseHeaders });
  } catch (error) { cleanup(); throw error; }
}
