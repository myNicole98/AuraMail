import { beforeEach, describe, expect, it } from "vitest";
import {
  defaultSpaceAppearance,
  loadSpaceAppearance,
  loadSpaceGeneral,
  loadSpaceWorkflow,
  migrateLegacyAppearance,
  saveSpaceAppearance,
  saveSpaceGeneral,
  saveSpaceWorkflow
} from "./spaces";

describe("space storage", () => {
  beforeEach(() => localStorage.clear());

  it("uses the default appearance for a new space", () => {
    expect(loadSpaceAppearance(99)).toEqual(defaultSpaceAppearance());
    expect(defaultSpaceAppearance().theme_choice).toBe("system");
    expect(defaultSpaceAppearance().theme_background).toBe("solid");
    expect(defaultSpaceAppearance().theme_panes).toBe("straight");
  });

  it("stores appearance and workflow per space", () => {
    saveSpaceAppearance(1, {
      ...defaultSpaceAppearance(),
      theme_choice: "dark",
      theme_background: "glass"
    });
    saveSpaceAppearance(2, {
      ...defaultSpaceAppearance(),
      theme_choice: "light"
    });
    saveSpaceWorkflow(1, {
      selectedFolderId: 12,
      selectedTabId: 3,
      selectedMessageId: 7,
      unified: "inbox",
      draftsOpen: false
    });

    expect(loadSpaceAppearance(1).theme_choice).toBe("dark");
    expect(loadSpaceAppearance(2).theme_choice).toBe("light");
    expect(loadSpaceWorkflow(1).selectedFolderId).toBe(12);
    expect(loadSpaceWorkflow(2).selectedFolderId).toBeNull();
  });

  it("stores General settings per space", () => {
    saveSpaceGeneral(1, {
      ...loadSpaceGeneral(1),
      mark_read_delay: 5,
      message_order: "oldest"
    });
    saveSpaceGeneral(2, {
      ...loadSpaceGeneral(2),
      mark_read_delay: -1
    });
    expect(loadSpaceGeneral(1).mark_read_delay).toBe(5);
    expect(loadSpaceGeneral(1).message_order).toBe("oldest");
    expect(loadSpaceGeneral(2).mark_read_delay).toBe(-1);
    expect(loadSpaceGeneral(2).message_order).toBe("newest");
  });

  it("copies legacy appearance into the default space once", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        theme_choice: "dark",
        theme_background: "glass",
        layout_mode: "reader",
        message_preview: "compact",
        dark_email: true
      })
    );
    migrateLegacyAppearance(1);
    expect(loadSpaceAppearance(1)).toMatchObject({
      theme_choice: "dark",
      theme_background: "glass",
      layout_mode: "reader",
      message_preview: "compact",
      dark_email: true
    });
  });
});
