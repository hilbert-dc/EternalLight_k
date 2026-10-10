/** Current multimedia contract. Revision identities, rather than display order, locate edits. */
export type CreativeMedium = "novel" | "comic" | "animation";
export interface Rational { numerator: number; denominator: number }
export interface MediaTime { value: number; rate: Rational }
export interface MediaRect { x: number; y: number; width: number; height: number }
export interface EditionScope {
  projectId: string; editionId: string; scopeId: string; branchId: string;
}
export interface CreativeEdition extends EditionScope {
  id: string; medium: CreativeMedium; title: string;
  canonSnapshotId: string | null; sources: MediaSourceRef[];
}
export interface MediaProject {
  id: string; title: string; logline: string; created_at?: number | string; updated_at: number | string;
}
export interface CreativeProjectCreated {
  project: MediaProject; editions: CreativeEdition[]; activeEditionId: string;
}
export interface ComicBalloon {
  id: string; text: string; speaker: string; rect: MediaRect;
  kind: "speech" | "thought" | "caption" | "sound";
}
export interface ComicPanel {
  id: string; description: string; rect: MediaRect;
  assetRevisionId: string | null; referenceAssetRevisionIds: string[];
  balloons: ComicBalloon[]; locked: boolean;
}
export interface ComicPage {
  id: string; title: string; width: number; height: number;
  readingDirection: "leftToRight" | "rightToLeft" | "vertical";
  panels: ComicPanel[];
}
export interface ComicDocument {
  kind: "comic"; title: string; format: "pages" | "strip"; pages: ComicPage[];
}
export interface AnimationShot {
  id: string; title: string; description: string; assetRevisionId: string | null;
  referenceAssetRevisionIds: string[]; locked: boolean;
}
export interface AnimationClip {
  id: string; shotId: string | null; assetRevisionId: string | null;
  start: MediaTime; duration: MediaTime; sourceStart: MediaTime;
  playbackRate: Rational; caption: string;
}
export interface AnimationTrack {
  id: string; title: string; kind: "video" | "audio" | "subtitle";
  muted: boolean; clips: AnimationClip[];
}
export interface AnimationDocument {
  kind: "animation"; title: string; width: number; height: number;
  frameRate: Rational; shots: AnimationShot[];
  timeline: { tracks: AnimationTrack[] };
}
export type MultimediaDocument = ComicDocument | AnimationDocument;
export interface MediaAsset {
  id: string; assetRevisionId: string; projectId: string; scopeId: string;
  name: string; kind: "image" | "audio" | "video"; mimeType: string;
  digest: string; bytes: number;
  width: number | null; height: number | null; duration: MediaTime | null;
  sampleRate: number | null; channels: number | null;
  previewUrl: string; createdAt: number;
}
/** Character coordinates use Unicode scalar values, as Array.from(text), never UTF-16 indexes. */
export type MediaTextField =
  | { kind: "balloonText"; pageId: string; panelId: string; balloonId: string }
  | { kind: "panelDescription"; pageId: string; panelId: string }
  | { kind: "shotDescription"; shotId: string }
  | { kind: "clipCaption"; trackId: string; clipId: string }
  | { kind: "canonStatement"; entryId: string };
export interface MediaTextEdit {
  startChar: number; endChar: number; expectedSliceDigest: string; replacement: string;
}
export interface MediaTextPage {
  section: "text"; kind: "text"; unit: "unicodeScalar"; partialView: true;
  version: "source" | "working"; revisionId: string; field: MediaTextField;
  source: MediaSourceRef; returnedSource: MediaSourceRef;
  text: string; textDigest: string; sliceDigest: string;
  startChar: number; endChar: number; totalChars: number;
  authorizedStartChar: number; authorizedEndChar: number;
  charOffset: number; nextCharOffset: number | null;
}
export type MediaSelector =
  | { kind: "whole" }
  | { kind: "fieldText"; field: MediaTextField; startChar: number; endChar: number; quote: string }
  | { kind: "text"; startByte: number; endByte: number; quote: string }
  | { kind: "panel"; pageId: string; panelId: string; region?: MediaRect }
  | { kind: "shot"; shotId: string; range?: { start: MediaTime; duration: MediaTime } }
  | { kind: "clip"; trackId: string; clipId: string; range?: { start: MediaTime; duration: MediaTime } }
  | { kind: "region"; region: MediaRect }
  | { kind: "time"; start: MediaTime; duration: MediaTime };
// Text and fieldText quote values are exact, bounded excerpts of at most 8000 Unicode scalars.
export interface MediaSourceRef {
  projectId: string; scopeId: string; branchId: string;
  objectId: string; revisionId: string; digest: string; selector: MediaSelector;
}
export interface MediaReadDependency {
  source: MediaSourceRef; freshness: "pinned" | "mustBeCurrent";
}
export interface MediaSourceLink {
  relation: "adapts" | "depicts" | "usesReference" | "derivedFrom";
  source: MediaSourceRef; targetObjectId: string; targetSelector: MediaSelector;
}
export interface MultimediaWorkspace {
  project: MediaProject; edition: CreativeEdition;
  revision: number; sourceDigest: string;
  revisionId: string | null; documentSource: MediaSourceRef | null; headSnapshotId: string;
  document: MultimediaDocument; documentEncoding: string | null; assets: MediaAsset[]; sources: MediaSourceLink[];
}
export interface SaveMultimediaDocument {
  expectedRevisionId: string | null; document: MultimediaDocument;
  idempotencyKey: string; readSet: MediaReadDependency[]; sources: MediaSourceLink[];
}
export type MediaDocumentOperation =
  | { kind: "replaceTextRanges"; field: Exclude<MediaTextField, { kind: "canonStatement" }>; expectedTextDigest: string; edits: MediaTextEdit[] }
  | { kind: "adoptCanonSnapshot"; canonSnapshotId: string }
  | { kind: "setTitle"; title: string }
  | { kind: "setComicFormat"; format: ComicDocument["format"] }
  | { kind: "upsertPage"; page: Omit<ComicPage, "panels"> }
  | { kind: "removePage"; pageId: string }
  | { kind: "movePage"; pageId: string; beforeId: string | null }
  | { kind: "upsertPanel"; pageId: string; panel: Omit<ComicPanel, "balloons" | "locked"> }
  | { kind: "removePanel"; pageId: string; panelId: string }
  | { kind: "movePanel"; pageId: string; panelId: string; beforeId: string | null }
  | { kind: "setPanelLock"; pageId: string; panelId: string; locked: boolean }
  | { kind: "upsertBalloon"; pageId: string; panelId: string; balloon: ComicBalloon }
  | { kind: "removeBalloon"; pageId: string; panelId: string; balloonId: string }
  | { kind: "moveBalloon"; pageId: string; panelId: string; balloonId: string; beforeId: string | null }
  | { kind: "setAnimationSettings"; width: number; height: number; frameRate: Rational }
  | { kind: "upsertShot"; shot: Omit<AnimationShot, "locked"> }
  | { kind: "removeShot"; shotId: string }
  | { kind: "moveShot"; shotId: string; beforeId: string | null }
  | { kind: "setShotLock"; shotId: string; locked: boolean }
  | { kind: "upsertTrack"; track: Omit<AnimationTrack, "clips"> }
  | { kind: "removeTrack"; trackId: string }
  | { kind: "moveTrack"; trackId: string; beforeId: string | null }
  | { kind: "upsertClip"; trackId: string; clip: AnimationClip }
  | { kind: "removeClip"; trackId: string; clipId: string }
  | { kind: "moveClip"; trackId: string; clipId: string; beforeId: string | null }
  | { kind: "removeSourceLink"; link: MediaSourceLink };
export interface PatchMultimediaDocument {
  expectedRevisionId: string | null; idempotencyKey: string;
  operations: MediaDocumentOperation[]; readSet: MediaReadDependency[]; sources: MediaSourceLink[];
}
export interface CanonEntry {
  id: string; category: "character" | "world" | "rule" | "timeline" | "style";
  statement: string; sources: MediaSourceRef[];
}
export interface CanonDocument { title: string; entries: CanonEntry[] }
export interface CanonWorkspace {
  projectId: string; scopeId: string; branchId: string; snapshotId: string;
  revisionId: string | null; document: CanonDocument; documentSource: MediaSourceRef | null;
}
export interface SaveCanonDocument { expectedRevisionId: string | null; document: CanonDocument; idempotencyKey: string }
export interface MediaSourceImpact {
  source: MediaSourceRef; latest: MediaSourceRef | null;
  status: "current" | "changed" | "selectionMissing" | "removed";
  targets: { objectId: string; selector: MediaSelector }[];
}
export interface MediaSourceImpactPage {
  projectId: string; editionId: string; branchId: string; revisionId: string | null;
  items: MediaSourceImpact[]; offset: number; total: number; nextOffset: number | null;
  canon: { pinnedSnapshotId: string; currentSnapshotId: string; changed: boolean };
}
export interface MediaSelection {
  scope: EditionScope; revisionId: string | null; label: string;
  assetRevisionId: string | null; selector: MediaSelector;
}
export interface MediaTurnInput {
  scope: EditionScope; expectedRevisionId: string | null;
  selection: MediaSelection | null; imageAssetRevisionIds: string[];
}
export interface MediaImportBytes {
  name: string; bytes: Uint8Array;
}
export interface MediaExportRequest {
  expectedRevisionId: string; format: "png" | "pdf" | "mp4" | "project";
  pageIds?: string[];
}
export interface MediaExportResult {
  id: string; editionId: string; revisionId: string;
  name: string; format: MediaExportRequest["format"]; bytes: number;
  assetRevisionIds: string[]; manifestId: string;
}
export interface MediaCapability {
  kind: "image.generate" | "image.edit" | "audio.speech" | "audio.transcribe" | "video.generate" | "local.render";
  providerId: string; available: boolean; reason: string | null;
}
export interface MediaServiceConnection {
  id: string; title: string; protocol: "openai-images" | "openai-audio" | "runway";
  baseUrl: string; model: string; credentialConfigured: boolean;
  capabilities: MediaCapability[];
}
export interface MultimediaApi {
  listEditions(projectId: string): Promise<{ editions: CreativeEdition[] }>;
  createEdition(projectId: string, request: { title: string; medium: CreativeMedium; idempotencyKey: string; sources: MediaSourceRef[] }): Promise<CreativeEdition>;
  readWorkspace(projectId: string, editionId: string, branchId?: string): Promise<MultimediaWorkspace>;
  saveDocument(projectId: string, editionId: string, request: SaveMultimediaDocument): Promise<MultimediaWorkspace>;
  patchDocument(projectId: string, editionId: string, request: PatchMultimediaDocument): Promise<MultimediaWorkspace>;
  readTextPage(projectId: string, editionId: string, request: { revisionId: string; field: Exclude<MediaTextField, { kind: "canonStatement" }>; charOffset?: number; charLimit?: number; branchId?: string }): Promise<MediaTextPage>;
  readSubtitleFont(): Promise<{ previewUrl: string; digest: string } | null>;
  readCanon(projectId: string, snapshotId?: string): Promise<CanonWorkspace>;
  saveCanon(projectId: string, request: SaveCanonDocument): Promise<CanonWorkspace>;
  sourceImpacts(projectId: string, editionId: string, offset?: number, limit?: number): Promise<MediaSourceImpactPage>;
  importFiles(projectId: string, editionId: string, files?: MediaImportBytes[]): Promise<{ assets: MediaAsset[] }>;
  exportEdition(projectId: string, editionId: string, request: MediaExportRequest): Promise<MediaExportResult>;
  openExport(projectId: string, exportId: string): Promise<{ opened: boolean }>;
  listConnections(): Promise<{ connections: MediaServiceConnection[]; capabilities: MediaCapability[] }>;
  saveConnection(request: Omit<MediaServiceConnection, "credentialConfigured" | "capabilities"> & { apiKey?: string }): Promise<MediaServiceConnection>;
}
