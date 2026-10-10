export const workStopLabels: Readonly<Record<string, string>>;
export function workStopDescription(reason: string | null | undefined): string;
export type UsageCheck = { state: "current" } | { state: "unavailable"; phase: "taskRead" | "usageSettlement"; code: string | null };
export function generationUsageBlocked(run: { usageCheck?: UsageCheck } | null | undefined): boolean;
export function generationUsageDescription(run: { usageCheck?: UsageCheck } | null | undefined): string | null;
