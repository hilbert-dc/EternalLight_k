import type { AnimationClip, AnimationDocument, ComicDocument, MediaRect, MediaTime, MultimediaDocument, Rational } from "../../shared/multimedia-types";

export const newMediaId = (kind: string) => `${kind}-${crypto.randomUUID()}`;
export const timeSeconds = (time: MediaTime) => time.value * time.rate.denominator / time.rate.numerator;
export const secondsTime = (seconds: number, rate: Rational): MediaTime => ({ value: Math.round(seconds * rate.numerator / rate.denominator), rate: { ...rate } });
export const frameSeconds = (rate: Rational) => rate.denominator / rate.numerator;
export const clipEnd = (clip: AnimationClip) => timeSeconds(clip.start) + timeSeconds(clip.duration);
export const animationLength = (document: AnimationDocument) => Math.max(0, ...document.timeline.tracks.flatMap(track => track.clips.map(clipEnd)));
export const mediaLabel = (document: MultimediaDocument) => document.kind === "comic" ? "漫画" : "动画";
export const sameDocument = (a: MultimediaDocument, b: MultimediaDocument) => JSON.stringify(a) === JSON.stringify(b);
export function boundedRect(rect: MediaRect, width: number, height: number): MediaRect {
  const x = Math.max(0, Math.min(Math.max(0, width - 1), Math.round(rect.x)));
  const y = Math.max(0, Math.min(Math.max(0, height - 1), Math.round(rect.y)));
  return { x, y, width: Math.max(1, Math.min(width - x, Math.round(rect.width))), height: Math.max(1, Math.min(height - y, Math.round(rect.height))) };
}
export function changeComic(document: ComicDocument, change: (draft: ComicDocument) => void): ComicDocument {
  const draft = structuredClone(document); change(draft); return draft;
}
export function changeAnimation(document: AnimationDocument, change: (draft: AnimationDocument) => void): AnimationDocument {
  const draft = structuredClone(document); change(draft); return draft;
}
export function formatTime(value: number): string {
  return `${Math.floor(value / 60)}:${(value % 60).toFixed(2).padStart(5, "0")}`;
}
