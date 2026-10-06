export declare const MAX_GENERATION_CHAPTERS: 100;
export type GenerationPermit = Readonly<{
  targetChapters: number;
  maxTaskAttempts: null;
}>;
export declare function generationPermitForChapters(targetChapters: number): GenerationPermit;
