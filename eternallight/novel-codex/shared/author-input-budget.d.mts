export const MAX_AUTHOR_INTENT_CHARACTERS: number;
export const MAX_AUTHOR_INTENT_BYTES: number;
export const MAX_AUTHOR_INTENT_TOKENS: number;
export function modelInputTokenBudget(text: string): number;
export function authorIntentError(text: unknown): string | null;
export function validateAuthorIntent(text: unknown): asserts text is string;
