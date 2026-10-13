export type QualityKind = import("../shared/generated/responses/HttpQualityKind").HttpQualityKind;
export type QualityNote = import("../shared/generated/responses/HttpQualityNote").HttpQualityNote;
export type QualityRecord = import("../shared/generated/responses/HttpQualityRecord").HttpQualityRecord;
export type QualityPacketSummary = import("../shared/generated/responses/HttpQualityPacketSummary").HttpQualityPacketSummary;
export type QualityNotebook = import("../shared/generated/responses/HttpQualityNotebook").HttpQualityNotebook;
export type QualityPacket = import("../shared/generated/responses/HttpQualityPacket").HttpQualityPacket;
export type QualityReaderReport = import("../shared/generated/responses/HttpQualityReaderReport").HttpQualityReaderReport;
export type QualityReview = import("../shared/generated/responses/HttpQualityReview").HttpQualityReview;
export type QualityChange = { action: "note"; note: QualityNote } | { action: "decide"; id: string; status: "confirmed" | "withdrawn"; reason: string } | { action: "packet"; kind: "reading" | "comparison"; sources: { revisionId: string | null; quote: string | null; noteId: string | null }[] } | { action: "feedback"; id: string; preference: string; reason: string };
export interface QualityMutation { expectedVersion: number; idempotencyKey: string; change: QualityChange; }
