import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  defaultLayout,
  layoutColumns,
  loadLayout,
  narrowPane,
  saveLayout,
  updateSize,
  updateStackHeight
} from "./layout";

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("layoutColumns", () => {
  it("uses proportional columns on wide windows", () => {
    const columns = layoutColumns({ ...defaultLayout }, 1200);
    expect(columns.sidebar).toBe(200);
    expect(columns.list).toBe(540);
    expect(columns.reader).toBe(460);
  });

  it("collapses the sidebar below 1100px", () => {
    expect(layoutColumns({ ...defaultLayout }, 1000).collapsed).toBe(true);
  });

  it("clamps the stacked list height", () => {
    expect(updateStackHeight({ ...defaultLayout }, 500, 900).stackHeight).toBe(620);
    expect(updateStackHeight({ ...defaultLayout }, -500, 900).stackHeight).toBe(140);
  });

  it("keeps the sidebar minimum just above the collapse breakpoint", () => {
    expect(layoutColumns({ ...defaultLayout }, 1101).sidebar).toBe(200);
  });

  it("respects minimum pane widths", () => {
    const columns = layoutColumns({ sidebarWidth: 100, listWidth: 100, sidebarCollapsed: false, stackHeight: 280 }, 1200);
    expect(columns.sidebar).toBe(200);
    expect(columns.list).toBe(540);
  });

  it("clamps dragged pane sizes", () => {
    expect(updateSize({ ...defaultLayout }, "sidebar", 500).sidebarWidth).toBe(380);
    expect(updateSize({ ...defaultLayout }, "list", 500).listWidth).toBe(640);
  });
});

describe("layout persistence", () => {
  it("falls back to defaults for non-finite stored ratios", () => {
    localStorage.setItem(
      "auramail.layout",
      JSON.stringify({ sidebarRatio: "oops", listRatio: "Infinity" })
    );
    expect(loadLayout()).toEqual(defaultLayout);
  });

  it("migrates the old ratio layout to pixel sizes", () => {
    localStorage.setItem(
      "auramail.layout",
      JSON.stringify({ sidebarRatio: 0.28, listRatio: 0.45 })
    );
    expect(loadLayout()).toEqual({
      sidebarWidth: 336,
      listWidth: 540,
      sidebarCollapsed: false,
      stackHeight: 280
    });
  });

  it("ignores storage write failures", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => saveLayout(defaultLayout)).not.toThrow();
  });
});

describe("narrowPane", () => {
  it("swaps between list and reader at and below 760px", () => {
    expect(narrowPane(761, false)).toBeNull();
    expect(narrowPane(760, false)).toBe("list");
    expect(narrowPane(760, true)).toBe("reader");
    expect(narrowPane(480, true)).toBe("reader");
  });
});
