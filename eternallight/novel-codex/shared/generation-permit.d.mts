export type GenerationPermit = Readonly<{
  targetChapters: number;
  maxTaskAttempts: number;
  maxProviderRequests: number;
  maxProviderTokens: number;
  maxDurationMinutes: number;
}>;
export declare function generationPermitForChapters(targetChapters: number): GenerationPermit;
