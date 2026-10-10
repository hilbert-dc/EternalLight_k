export function subtitleFontCoverage(bytes: Uint8Array): { mimeType: string; covers(codePoint: number): boolean };
export function subtitleCaptionError(caption: string, font: ReturnType<typeof subtitleFontCoverage>): string | null;
