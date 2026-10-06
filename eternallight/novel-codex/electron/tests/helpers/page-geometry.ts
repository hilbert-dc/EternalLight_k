export const layoutFrame = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const check = (value, message) => { if (!value) throw Error(message); };
export function visibleElement(element: Element) {
  if (!element.getClientRects().length || element.closest(".sr-only")) return false;
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    if (parent instanceof HTMLDetailsElement && !parent.open && !parent.querySelector(":scope > summary")?.contains(element)) return false;
  }
  return true;
}

/** Uses the isolated Electron driver's viewport; does not simulate a narrower DOM wrapper. */
export async function resizeTestViewport(width: number, height: number) {
  window.testViewport = { width, height };
  for (let index = 0; index < 150 && (window.testViewport || innerWidth !== width || innerHeight !== height); index++) await new Promise(resolve => setTimeout(resolve, 20));
  check(!window.testViewport && innerWidth === width && innerHeight === height, "Electron viewport resize did not settle");
  await layoutFrame();
}

export async function surfaceGeometry(selector: string, label: string) {
  const surface = document.querySelector<HTMLElement>(selector); check(surface, `${label}: missing surface`);
  await layoutFrame();
  const bounds = surface.getBoundingClientRect();
  check(bounds.width >= 200 && bounds.left >= -2 && bounds.right <= innerWidth + 2, `${label}: surface outside viewport`);
  check(surface.scrollWidth <= surface.clientWidth + 2, `${label}: horizontal surface overflow`);
  check(document.scrollingElement.scrollWidth <= innerWidth + 2 && document.scrollingElement.scrollHeight <= innerHeight + 2 && scrollY === 0, `${label}: outer canvas scroll ${document.scrollingElement.scrollWidth}×${document.scrollingElement.scrollHeight} vs ${innerWidth}×${innerHeight}, y=${scrollY}`);
  const fields = [...surface.querySelectorAll<HTMLInputElement>("input:not([type=radio]):not([type=checkbox]),textarea,select")].filter(visibleElement);
  check(fields.every(element => { const box = element.getBoundingClientRect(); return box.width >= 120 && box.left >= bounds.left - 2 && box.right <= bounds.right + 2; }), `${label}: clipped or collapsed form field`);
  const controls = [...surface.querySelectorAll<HTMLElement>("button,input,textarea,select")].filter(visibleElement);
  const last = controls.at(-1); check(last, `${label}: no reachable final control`); last.scrollIntoView({ block: "nearest" }); await layoutFrame();
  const lastBox = last.getBoundingClientRect();
  check(lastBox.top >= -2 && lastBox.bottom <= innerHeight + 2 && scrollY === 0, `${label}: final control is not reachable`);
  return { surface: label, width: innerWidth, height: innerHeight, fields: fields.length, fitsHorizontally: true, finalControlReachable: true, outerScroll: 0 };
}

export async function captureTestSurface(name: string) {
  window.testScreenshot = name;
  for (let index = 0; index < 100 && window.testScreenshot; index++) await new Promise(resolve => setTimeout(resolve, 20));
  check(!window.testScreenshot, "Isolated screenshot did not settle");
}

export async function authorSurfaceMatrix(selector: string, label: string, setTheme: (theme: string) => Promise<void>) {
  const initial = { width: innerWidth, height: innerHeight, theme: document.documentElement.dataset.theme ?? "light" }, results = [];
  for (const [width, height] of [[1400, 900], [900, 600]]) for (const theme of ["light", "dark"]) {
    await resizeTestViewport(width, height); await setTheme(theme);
    results.push({ theme, ...await surfaceGeometry(selector, label) });
    await captureTestSurface(`${label}-${theme}-${width}x${height}`);
  }
  await resizeTestViewport(initial.width, initial.height); await setTheme(initial.theme);
  return results;
}
