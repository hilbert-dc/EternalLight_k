/// <reference types="vite/client" />

import type { NovelCodexApi } from "./types";

declare global { interface Window { novelCodex?: NovelCodexApi; } }

export {};
