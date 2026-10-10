export function safeRuntimeDiagnostic(value) {
  if (typeof value !== "string") return "";
  return Array.from(value).slice(0, 2_000).join("")
    .replace(/sk-[A-Za-z0-9_-]{8,}/gu, "[已隐藏凭据]")
    .replace(/Bearer\s+[A-Za-z0-9._-]{8,}/gu, "Bearer [已隐藏凭据]");
}

// Only the HTTP adapter constructs this type. Provider prose is never parsed
// into a Native permission, accounting or revision fact.
export class NativeRuntimeError extends Error {
  constructor(detail) {
    const raw = typeof detail?.message === "string" ? detail.message : "Creative Runtime request failed";
    super(safeRuntimeDiagnostic(raw));
    this.name = "NativeRuntimeError";
    this.issuer = "native";
    this.phase = "domainRequest";
    this.code = typeof detail?.code === "string" && /^[a-z][a-z0-9_]{0,79}$/u.test(detail.code) ? detail.code : "runtime_request_failed";
    this.details = { diagnostic: safeRuntimeDiagnostic(typeof detail?.details?.diagnostic === "string" ? detail.details.diagnostic : raw) };
    for (const key of ["instancePath", "code", "recovery"]) {
      if (typeof detail?.details?.[key] === "string") this.details[key] = safeRuntimeDiagnostic(detail.details[key]);
    }
    const source = detail?.details?.sourceRef;
    if (source && typeof source === "object") this.details.sourceRef = Object.fromEntries(
      ["groupId", "draftRevisionId"].filter(key => typeof source[key] === "string").map(key => [key, safeRuntimeDiagnostic(source[key])]),
    );
  }
}
export const nativeRuntimeError = payload => new NativeRuntimeError(payload?.error);
