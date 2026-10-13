import { EventEmitter } from "node:events";
import { nativeBinding } from "../native.mjs";

// Node-shaped callbacks only: listening, HTTP parsing, admission, uploads and
// socket response queues are owned by Rust, not node:http.
class ResponseAdapter extends EventEmitter {
  constructor(native, id) {
    super();
    Object.assign(this, { native, id, headersSent: false, destroyed: false, writableEnded: false, pending: [], pendingBytes: 0, ending: false });
  }
  writeHead(status, headers) {
    this.native.head(this.id, status, JSON.stringify(headers));
    this.headersSent = true;
  }
  write(value) {
    if (this.destroyed || this.writableEnded) throw new Error("bridge response closed");
    const chunk = Buffer.from(value);
    if (!this.pending.length && this.native.write(this.id, chunk)) return true;
    if (this.pendingBytes + chunk.length > 32 * 1024 * 1024) throw new Error("bridge response exceeds limit");
    this.pending.push(chunk);
    this.pendingBytes += chunk.length;
    return false;
  }
  end(value) {
    if (value !== undefined) this.write(value);
    this.ending = true;
    this.flush();
  }
  flush() {
    if (this.destroyed || this.writableEnded) return;
    if (this.native.closed(this.id)) { this.destroy(); return; }
    const hadPending = this.pending.length > 0;
    while (this.pending.length && this.native.write(this.id, this.pending[0])) {
      this.pendingBytes -= this.pending.shift().length;
    }
    if (this.pending.length) return;
    if (this.ending) {
      this.native.end(this.id);
      this.writableEnded = true;
      this.emit("finish");
    } else if (hadPending) this.emit("drain");
  }
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true;
    this.pending = [];
    this.pendingBytes = 0;
    this.native.abort(this.id);
    this.emit("close");
  }
}

export class LocalHttpServer {
  constructor(options, handler) {
    this.native = new (nativeBinding().NativeLocalHttpServer)(JSON.stringify(options));
    this.responses = new Set();
    this.listening = true;
    this.timer = setInterval(() => {
      for (let input; (input = this.native.poll()) != null;) {
        const value = JSON.parse(input);
        const response = new ResponseAdapter(this.native, value.id);
        const request = Object.assign(new EventEmitter(), value, { destroy: () => response.destroy() });
        this.responses.add(response);
        Promise.resolve().then(() => handler(request, response)).catch(() => response.destroy());
      }
      for (const response of this.responses) {
        try { response.flush(); } catch { response.destroy(); }
        if (response.destroyed || response.writableEnded) this.responses.delete(response);
      }
    }, 2);
  }
  address() { return { port: this.native.port }; }
  cancelUploads() { this.native.cancelUploads(); }
  close(callback) {
    this.listening = false;
    clearInterval(this.timer);
    for (const response of this.responses) response.destroy();
    this.responses.clear();
    this.native.close();
    callback?.();
  }
}
