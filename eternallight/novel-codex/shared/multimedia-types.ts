/** Current multimedia contract. Revision identities, rather than display order, locate edits. */
export type CreativeMedium = import("./generated/CreativeMedium").CreativeMedium;
export type Rational = import("./generated/Rational").Rational;
export type MediaTime = import("./generated/MediaTime").MediaTime;
export type MediaRect = import("./generated/MediaRect").MediaRect;
export interface EditionScope {
  projectId: string; editionId: string; scopeId: string; branchId: string;
}
export type CreativeEdition = import("./generated/CreativeEdition").CreativeEdition;
export type MediaProject = import("./generated/MediaProject").MediaProject;
export type CreativeProjectCreated = import("./generated/CreativeProjectCreated").CreativeProjectCreated;
export type ComicBalloon = import("./generated/ComicBalloon").ComicBalloon;
export type ComicPanel = import("./generated/ComicPanel").ComicPanel;
export type ComicPage = import("./generated/ComicPage").ComicPage;
export type ComicDocument = Extract<import("./generated/MultimediaDocument").MultimediaDocument, { kind: "comic" }>;
export type AnimationShot = import("./generated/AnimationShot").AnimationShot;
export type AnimationClip = import("./generated/AnimationClip").AnimationClip;
export type AnimationTrack = import("./generated/AnimationTrack").AnimationTrack;
export type ScreenForm = "shortFilm" | "featureFilm" | "episode";
export type ScreenExpression = "animation" | "liveAction" | "hybrid";
export type ScreenDeliverable = "screenplay" | "storyboard" | "animatic" | "finishedVideo";
export type ScreenProduction = import("./generated/ScreenProduction").ScreenProduction;
export type AnimationDocument = Extract<import("./generated/MultimediaDocument").MultimediaDocument, { kind: "animation" }>;
export type MultimediaDocument = ComicDocument | AnimationDocument;
/** Runtime owns asset identity; Host replaces only previewUrl with its scoped preview capability. */
export type MediaAsset = Omit<import("./generated/MediaAsset").MediaAsset, "previewUrl"> & { previewUrl: string };
/** Character coordinates use Unicode scalar values, as Array.from(text), never UTF-16 indexes. */
export type MediaTextField = import("./generated/MediaTextField").MediaTextField;
export interface MediaTextEdit {
  startChar: number; endChar: number; expectedSliceDigest: string; replacement: string;
}
export type MediaTextPage = import("./generated/responses/RendererMediaReadTextPageResponse").RendererMediaReadTextPageResponse;
export type MediaSelector = import("./generated/MediaSelector").MediaSelector;
// Text and fieldText quote values are exact, bounded excerpts of at most 8000 Unicode scalars.
export type MediaSourceRef = import("./generated/MediaSourceRef").MediaSourceRef;
export type MediaReadDependency = import("./generated/MediaReadDependency").MediaReadDependency;
export type MediaSourcePurpose = "plot" | "character" | "visual" | "audio" | "timing" | "reference";
export type MediaSourceLink = import("./generated/MediaSourceLink").MediaSourceLink;
/** Host decorates only assets after Runtime wire validation. */
export type HostMediaWorkspace<T extends { assets: import("./generated/MediaAsset").MediaAsset[] }> = Omit<T, "assets"> & { assets: MediaAsset[] };
export type MultimediaWorkspace = HostMediaWorkspace<import("./generated/responses/RendererMediaReadWorkspaceResponse").RendererMediaReadWorkspaceResponse>;
export interface SaveMultimediaDocument {
  expectedRevisionId: string | null; document: MultimediaDocument;
  idempotencyKey: string; readSet: MediaReadDependency[]; sources: MediaSourceLink[];
}
export type MediaDocumentOperation =
  | { kind: "replaceTextRanges"; field: Exclude<MediaTextField, { kind: "canonStatement" }>; expectedTextDigest: string; edits: MediaTextEdit[] }
  | { kind: "replaceEditionSource"; previous: MediaSourceRef; next: MediaSourceRef }
  | { kind: "adoptCanonSnapshot"; canonSnapshotId: string }
  | { kind: "setCreativeDesign"; text: string | null }
  | { kind: "setScreenProduction"; production: ScreenProduction | null }
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
export type CanonEntry = import("./generated/CanonEntry").CanonEntry;
export type CanonDocument = import("./generated/CanonDocument").CanonDocument;
export type CanonWorkspace = import("./generated/responses/RendererMediaReadCanonResponse").RendererMediaReadCanonResponse;
export interface SaveCanonDocument { expectedRevisionId: string | null; document: CanonDocument; idempotencyKey: string }
export type MediaSourceImpact = import("./generated/responses/MediaImpactItem").MediaImpactItem;
export type MediaSourceImpactPage = import("./generated/responses/RendererMediaSourceImpactsResponse").RendererMediaSourceImpactsResponse;
export type MediaSourceCatalogPage = import("./generated/responses/RendererMediaSourceCatalogResponse").RendererMediaSourceCatalogResponse;
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
  sourceCatalog(projectId: string, editionId: string, snapshotId?: string, offset?: number, limit?: number): Promise<import("./generated/responses/RendererMediaSourceCatalogResponse").RendererMediaSourceCatalogResponse>;
  listEditions(projectId: string): Promise<import("./generated/responses/RendererMediaListEditionsResponse").RendererMediaListEditionsResponse>;
  createEdition(projectId: string, request: { title: string; medium: CreativeMedium; idempotencyKey: string; sources: MediaSourceRef[] }): Promise<import("./generated/responses/RendererMediaCreateEditionResponse").RendererMediaCreateEditionResponse>;
  readWorkspace(projectId: string, editionId: string, branchId?: string, revisionId?: string): Promise<HostMediaWorkspace<import("./generated/responses/RendererMediaReadWorkspaceResponse").RendererMediaReadWorkspaceResponse>>;
  saveDocument(projectId: string, editionId: string, request: SaveMultimediaDocument): Promise<HostMediaWorkspace<import("./generated/responses/RendererMediaSaveDocumentResponse").RendererMediaSaveDocumentResponse>>;
  patchDocument(projectId: string, editionId: string, request: PatchMultimediaDocument): Promise<HostMediaWorkspace<import("./generated/responses/RendererMediaPatchDocumentResponse").RendererMediaPatchDocumentResponse>>;
  readTextPage(projectId: string, editionId: string, request: { revisionId: string; field: Exclude<MediaTextField, { kind: "canonStatement" }>; charOffset?: number; charLimit?: number; branchId?: string }): Promise<import("./generated/responses/RendererMediaReadTextPageResponse").RendererMediaReadTextPageResponse>;
  readSubtitleFont(): Promise<{ previewUrl: string; digest: string } | null>;
  readCanon(projectId: string, snapshotId?: string): Promise<import("./generated/responses/RendererMediaReadCanonResponse").RendererMediaReadCanonResponse>;
  saveCanon(projectId: string, request: SaveCanonDocument): Promise<import("./generated/responses/RendererMediaSaveCanonResponse").RendererMediaSaveCanonResponse>;
  sourceImpacts(projectId: string, editionId: string, offset?: number, limit?: number): Promise<import("./generated/responses/RendererMediaSourceImpactsResponse").RendererMediaSourceImpactsResponse>;
  importFiles(projectId: string, editionId: string, files?: MediaImportBytes[]): Promise<{ assets: MediaAsset[] }>;
  exportEdition(projectId: string, editionId: string, request: MediaExportRequest): Promise<MediaExportResult>;
  openExport(projectId: string, exportId: string): Promise<{ opened: boolean }>;
  listConnections(): Promise<{ connections: MediaServiceConnection[]; capabilities: MediaCapability[] }>;
  saveConnection(request: Omit<MediaServiceConnection, "credentialConfigured" | "capabilities"> & { apiKey?: string }): Promise<MediaServiceConnection>;
}
