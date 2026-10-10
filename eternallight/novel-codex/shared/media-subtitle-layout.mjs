/** Pixel geometry is shared by the live preview and the exported raster. */
export const MEDIA_SUBTITLE_FONT_FAMILY = "MediaSubtitleFont";
export function mediaSubtitleLayout(width, height) {
  if (![width, height].every(value => Number.isInteger(value) && value > 0 && value <= 8192)) throw Error("字幕画面尺寸无效");
  return { fontSize: Math.max(12, Math.round(height / 24)), lineHeight: 1.35,
    horizontalMargin: Math.round(width * .06), bottomMargin: Math.round(height * .05),
    bandHeight: Math.ceil(height * .4), paddingX: Math.max(3, Math.round(height / 150)),
    paddingY: Math.max(2, Math.round(height / 240)), gap: Math.max(2, Math.round(height / 160)) };
}
