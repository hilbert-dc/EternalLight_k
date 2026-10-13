import assert from "node:assert/strict";
import test from "node:test";

import { isAllowedRendererNavigation, isTrustedIpcSender } from "../navigation-policy.mjs";

const options = { isPackaged: false, rendererUrl: "http://127.0.0.1:5173", packagedEntry: "/app/dist/index.html" };

test("allows only the exact development renderer origin", () => {
  assert.equal(isAllowedRendererNavigation("http://127.0.0.1:5173/", options), true);
  assert.equal(isAllowedRendererNavigation("http://127.0.0.1:5173/@vite/client", options), true);
  assert.equal(isAllowedRendererNavigation("http://127.0.0.1:5173@evil.example/", options), false);
  assert.equal(isAllowedRendererNavigation("http://evil.example/?next=http://127.0.0.1:5173", options), false);
});

test("packaged navigation is restricted to the exact application entry", () => {
  const packaged = { ...options, isPackaged: true };
  assert.equal(isAllowedRendererNavigation("file:///app/dist/index.html", packaged), true);
  assert.equal(isAllowedRendererNavigation("file:///app/dist/other.html", packaged), false);
  assert.equal(isAllowedRendererNavigation("https://example.com/", packaged), false);
});

test("privileged IPC requires the expected live WebContents and trusted frame URL", () => {
  const trusted = { isDestroyed: () => false, getURL: () => options.rendererUrl };
  assert.equal(isTrustedIpcSender({ sender: trusted, senderFrame: { url: `${options.rendererUrl}/` } }, trusted, options), true);
  assert.equal(isTrustedIpcSender({ sender: trusted, senderFrame: { url: "http://127.0.0.1:5173@evil.example/" } }, trusted, options), false);
  assert.equal(isTrustedIpcSender({ sender: { ...trusted } }, trusted, options), false);
  assert.equal(isTrustedIpcSender({ sender: { ...trusted, isDestroyed: () => true } }, trusted, options), false);
});
