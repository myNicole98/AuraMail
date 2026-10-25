export type Pane = "sidebar" | "list";

export type LayoutState = {
  sidebarWidth: number;
  listWidth: number;
  sidebarCollapsed: boolean;
  stackHeight: number;
};

export type LayoutColumns = {
  sidebar: number;
  list: number;
  reader: number;
  collapsed: boolean;
};

type LegacyLayout = Partial<LayoutState> & {
  sidebarRatio?: number;
  listRatio?: number;
};

export const defaultLayout: LayoutState = {
  sidebarWidth: 200,
  listWidth: 540,
  sidebarCollapsed: false,
  stackHeight: 280
};

export const NARROW_BREAKPOINT = 760;
export const SIDEBAR_MIN = 200;
export const SIDEBAR_MAX = 380;
export const LIST_MIN = 540;
export const LIST_MAX = 640;
const READER_MIN = 420;
const STACK_MIN = 140;
const STACK_BOTTOM_MIN = 280;
const RAIL = 64;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function storedNumber(value: unknown, fallback: number): number {
  if (value === undefined || value === null) return fallback;
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

export function layoutColumns(state: LayoutState, width: number): LayoutColumns {
  const collapsed = state.sidebarCollapsed || width <= 1100;
  const sidebar = collapsed
    ? RAIL
    : Math.round(clamp(state.sidebarWidth, SIDEBAR_MIN, SIDEBAR_MAX));
  const list = Math.round(clamp(state.listWidth, LIST_MIN, LIST_MAX));
  const available = Math.max(0, width - sidebar);
  const listFinal = Math.min(list, Math.max(0, available - READER_MIN));
  return {
    sidebar,
    list: listFinal,
    reader: Math.max(0, width - sidebar - listFinal),
    collapsed
  };
}

export function narrowPane(
  width: number,
  hasSelection: boolean
): "list" | "reader" | null {
  if (width > NARROW_BREAKPOINT) return null;
  return hasSelection ? "reader" : "list";
}

export function updateStackHeight(
  state: LayoutState,
  delta: number,
  availableHeight: number
): LayoutState {
  const max = Math.max(STACK_MIN, availableHeight - STACK_BOTTOM_MIN);
  return {
    ...state,
    stackHeight: clamp(state.stackHeight + delta, STACK_MIN, max)
  };
}

export function updateSize(state: LayoutState, pane: Pane, delta: number): LayoutState {
  const current = pane === "sidebar" ? state.sidebarWidth : state.listWidth;
  const next =
    pane === "sidebar"
      ? clamp(current + delta, SIDEBAR_MIN, SIDEBAR_MAX)
      : clamp(current + delta, LIST_MIN, LIST_MAX);
  return pane === "sidebar"
    ? { ...state, sidebarWidth: next }
    : { ...state, listWidth: next };
}

export function loadLayout(): LayoutState {
  try {
    const raw = localStorage.getItem("auramail.layout");
    if (!raw) return { ...defaultLayout };
    const parsed = JSON.parse(raw) as LegacyLayout;
    const sidebarWidth =
      parsed.sidebarWidth === undefined
        ? storedNumber(parsed.sidebarRatio, defaultLayout.sidebarWidth / 1200) * 1200
        : storedNumber(parsed.sidebarWidth, defaultLayout.sidebarWidth);
    const listWidth =
      parsed.listWidth === undefined
        ? storedNumber(parsed.listRatio, defaultLayout.listWidth / 1200) * 1200
        : storedNumber(parsed.listWidth, defaultLayout.listWidth);
    return {
      sidebarWidth: Math.round(clamp(sidebarWidth, SIDEBAR_MIN, SIDEBAR_MAX)),
      listWidth: Math.round(clamp(listWidth, LIST_MIN, LIST_MAX)),
      sidebarCollapsed: Boolean(parsed.sidebarCollapsed),
      stackHeight: Math.round(clamp(
        storedNumber(parsed.stackHeight, defaultLayout.stackHeight),
        STACK_MIN,
        Math.max(STACK_MIN, 900 - STACK_BOTTOM_MIN)
      ))
    };
  } catch {
    return { ...defaultLayout };
  }
}

export function saveLayout(state: LayoutState): void {
  try {
    localStorage.setItem("auramail.layout", JSON.stringify(state));
  } catch {
    return;
  }
}
