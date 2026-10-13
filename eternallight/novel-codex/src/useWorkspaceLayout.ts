import { useEffect, useState, type CSSProperties } from "react";
import { LayoutPanes, loadPanes, PANE_KEY } from "./WorkspaceLayout";

export function useWorkspaceLayout() {
  const [panes, setPanes] = useState<LayoutPanes>(loadPanes);
  useEffect(() => { try { localStorage.setItem(PANE_KEY, JSON.stringify(panes)); } catch { /* The layout still follows the current drag if storage is unavailable. */ } }, [panes]);
  const [viewport, setViewport] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));
  useEffect(() => {
    const resize = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const sidebarWidth = Math.min(panes.sidebar, Math.max(72, viewport.width - 670));
  const railWidth = Math.min(panes.rail, Math.max(300, viewport.width - sidebarWidth - 370));
  const inspectorMax = Math.max(160, viewport.height - 360);
  const layoutStyle = { "--nc-sidebar": `${sidebarWidth}px`, "--nc-rail": `${railWidth}px`, "--nc-inspector": `${Math.min(panes.inspector, inspectorMax)}px` } as CSSProperties;
  const sidebarMax = Math.min(480, Math.max(72, viewport.width - railWidth - 370));
  const railMax = Math.min(840, Math.max(300, viewport.width - sidebarWidth - 370));

  return { layoutStyle, sidebarWidth, sidebarMax, setPanes, railWidth, railMax, panes, inspectorMax };
}
