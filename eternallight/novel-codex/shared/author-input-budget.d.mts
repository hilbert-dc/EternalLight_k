export const MAX_AUTHOR_INTENT_CHARACTERS: number;
export const MAX_AUTHOR_INTENT_BYTES: number;
export function modelInputTokenBudget(text: string): number;
export function authorIntentError(text: unknown, options?: { imageAssetRevisionIds?: string[] }): string | null;
export function validateAuthorIntent(text: unknown, options?: { imageAssetRevisionIds?: string[] }): asserts text is string;
