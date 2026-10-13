// Bound the complete serialized selection, including scope, revision and label.
export const MAX_MEDIA_SELECTION_BYTES = 8_000;
export function mediaSelectionFits(value) {
  return new TextEncoder().encode(JSON.stringify(value)).byteLength <= MAX_MEDIA_SELECTION_BYTES;
}
