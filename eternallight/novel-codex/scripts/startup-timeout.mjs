// Release diagnostics may need longer cold executable loading on the host OS.
// This is a bounded wait, never a replacement for actual success evidence.
export function startupTimeout(value = 30000) {
  if (!Number.isSafeInteger(value) || value < 1000 || value > 300000) throw new Error("startupTimeoutMs must be an integer between 1000 and 300000");
  return value;
}
