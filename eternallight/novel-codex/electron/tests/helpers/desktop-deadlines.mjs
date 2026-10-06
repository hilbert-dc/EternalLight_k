// The production Runtime and thread bootstrap each permit 90 seconds.
// The fixture's whole-process deadline must not expire before these stages.
export const DESKTOP_RUNTIME_STARTUP_MS = 90_000;
export const DESKTOP_JOURNEY_MS = 90_000 + 30_000 + 15_000;
export const DESKTOP_CHILD_MS = DESKTOP_RUNTIME_STARTUP_MS + DESKTOP_JOURNEY_MS + 10_000;
export const DESKTOP_TEST_MS = DESKTOP_CHILD_MS + 10_000;
