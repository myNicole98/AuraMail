import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  applyAccessibilityPreferences,
  applyMatugenPalette,
  applyThemePreferences,
  applyThemeSettings,
  clearMatugenPalette,
  defaultPreferences,
  loadPreferences,
  parseExtraHeaders,
  reloadMatugenTheme,
  savePreferences,
  senderIsTrusted,
  snapUiScale,
  uiScale,
  updatePreferences,
  validateAccountForm,
  validateAiSettings,
  type AccountForm
} from "./settings";
import { base16Keys, type Base16 } from "./theme/base16";
import { auraDarkTheme } from "./theme/catalog";
import { shiftPalette } from "./theme/colorVision";

const matugenPalette = (base00 = "#111111") => ({
  scheme: "Matugen",
  ...Object.fromEntries(base16Keys.map((key) => [key, base00]))
});

const account = (): AccountForm => ({
  display_name: "Ada",
  username: "ada@example.com",
  password: "secret",
  imap_host: "imap.example.com",
  imap_port: "993",
  imap_security: "tls",
  smtp_host: "smtp.example.com",
  smtp_port: "465",
  smtp_security: "tls",
  color: "accent"
});

beforeEach(() => {
  localStorage.clear();
  clearMatugenPalette();
});

afterEach(() => {
  clearMatugenPalette();
});

describe("account validation", () => {
  it("accepts a complete account and rejects missing fields", () => {
    expect(validateAccountForm(account())).toEqual({});

    const form = account();
    form.display_name = " ";
    form.username = "not-an-email";
    form.password = "";
    form.imap_host = "";
    form.smtp_host = "";
    const errors = validateAccountForm(form);
    expect(errors.username).toBeDefined();
    expect(errors.password).toBeDefined();
    expect(errors.imap_host).toBeDefined();
    expect(errors.smtp_host).toBeDefined();
    expect(validateAccountForm({ ...account(), password: "" }, false)).toEqual({});
    expect(validateAccountForm({ ...account(), display_name: " " })).toEqual({});
  });

  it("rejects non-numeric and out-of-range ports", () => {
    const form = account();
    form.imap_port = "0";
    form.smtp_port = "70000";
    const errors = validateAccountForm(form);
    expect(errors.imap_port).toBeDefined();
    expect(errors.smtp_port).toBeDefined();
  });

  it("requires a supported security mode", () => {
    const form = account();
    form.imap_security = "plain";
    form.smtp_security = "starttls";
    const errors = validateAccountForm(form);
    expect(errors.imap_security).toBeDefined();
    expect(errors.smtp_security).toBeUndefined();
  });
});

describe("AI settings validation", () => {
  it("requires an http(s) base URL and a model", () => {
    expect(validateAiSettings("https://api.example.com/v1", "gpt-test")).toEqual({});
    expect(validateAiSettings("api.example.com", "gpt-test").base_url).toBeDefined();
    expect(validateAiSettings("https://api.example.com/v1", " ").model).toBeDefined();
  });

  it("parses extra headers one per line", () => {
    expect(parseExtraHeaders("X-Org: auramail\n\nX-Trace: 1")).toEqual({
      headers: [
        ["X-Org", "auramail"],
        ["X-Trace", "1"]
      ],
      error: null
    });
    expect(parseExtraHeaders("").headers).toEqual([]);
  });

  it("rejects malformed or unsafe headers", () => {
    expect(parseExtraHeaders("no colon").error).toBeTruthy();
    expect(parseExtraHeaders("Bad Name: value").error).toBeTruthy();
    expect(parseExtraHeaders("X-Test: bad\rvalue").error).toBeTruthy();
  });
});

describe("settings preferences", () => {
  it("starts from defaults when storage is empty or corrupt", () => {
    expect(loadPreferences()).toEqual(defaultPreferences());
    localStorage.setItem("auramail.settings", "{not json");
    expect(loadPreferences()).toEqual(defaultPreferences());
  });

  it("persists and merges known values", () => {
    savePreferences({
      ...defaultPreferences(),
      auto_classify: true,
      theme_choice: "dark"
    });
    const loaded = loadPreferences();
    expect(loaded.auto_classify).toBe(true);
    expect(loaded.theme_choice).toBe("dark");
    expect(loaded.theme_accent).toBe("");
    expect(loaded.dark_email).toBe(false);
    expect(loaded.match_voice).toBe(false);

    savePreferences({ ...loaded, dark_email: true, match_voice: true });
    expect(loadPreferences().dark_email).toBe(true);
    expect(loadPreferences().match_voice).toBe(true);
  });

  it("normalizes trusted senders and persists the global image preference", () => {
    savePreferences({
      ...defaultPreferences(),
      always_load_images: true,
      trusted_senders: ["  ADA@Example.com ", "ada@example.com"]
    });
    const loaded = loadPreferences();
    expect(loaded.always_load_images).toBe(true);
    expect(loaded.trusted_senders).toEqual(["ada@example.com"]);
    expect(senderIsTrusted(loaded, "Ada@Example.com")).toBe(true);
    expect(senderIsTrusted(loaded, "other@example.com")).toBe(false);
  });

  it("defaults the interface preferences", () => {
    const defaults = defaultPreferences();
    expect(defaults.ui_scale).toBe(100);
    expect(defaults.layout_mode).toBe("classic");
    expect(defaults.message_preview).toBe("preview");
  });

  it("defaults sender profile pictures on and persists opt-out", () => {
    const defaults = defaultPreferences();
    expect(defaults.profile_pictures).toBe(true);

    savePreferences({ ...defaults, profile_pictures: false });

    expect(loadPreferences().profile_pictures).toBe(false);
  });

  it("defaults accessibility preferences", () => {
    const defaults = defaultPreferences();
    expect(defaults.reduce_motion).toBe(false);
    expect(defaults.high_contrast).toBe(false);
  });

  it("persists accessibility preferences", () => {
    savePreferences({
      ...defaultPreferences(),
      reduce_motion: true,
      high_contrast: true
    });
    const loaded = loadPreferences();
    expect(loaded.reduce_motion).toBe(true);
    expect(loaded.high_contrast).toBe(true);
  });

  it("applies accessibility preferences to the document root", () => {
    applyAccessibilityPreferences({
      ...defaultPreferences(),
      reduce_motion: true,
      high_contrast: true
    });
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    expect(document.documentElement.dataset.highContrast).toBe("true");

    applyAccessibilityPreferences(defaultPreferences());
    expect(document.documentElement.dataset.reduceMotion).toBe("false");
    expect(document.documentElement.dataset.highContrast).toBe("false");
  });

  it("normalizes unknown interface preferences", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        ...defaultPreferences(),
        ui_scale: 900,
        layout_mode: "unknown",
        message_preview: "wall"
      })
    );
    const loaded = loadPreferences();
    expect(loaded.ui_scale).toBe(300);
    expect(loaded.layout_mode).toBe("classic");
    expect(loaded.message_preview).toBe("preview");
  });

  it("migrates the old three UI sizes to percentages", () => {
    for (const [uiSize, expected] of [
      ["compact", 90],
      ["default", 100],
      ["large", 110]
    ] as const) {
      localStorage.setItem(
        "auramail.settings",
        JSON.stringify({ ui_size: uiSize })
      );
      expect(loadPreferences().ui_scale).toBe(expected);
    }
  });

  it("maps UI percentages to webview zoom", () => {
    expect(uiScale(50)).toBe(0.5);
    expect(uiScale(100)).toBe(1);
    expect(uiScale(300)).toBe(3);
    expect(uiScale(500)).toBe(3);
    expect(uiScale(10)).toBe(0.5);
  });

  it("snaps to major UI-size stops", () => {
    expect(snapUiScale(98)).toBe(100);
    expect(snapUiScale(102)).toBe(100);
    expect(snapUiScale(74)).toBe(75);
    expect(snapUiScale(76)).toBe(75);
    expect(snapUiScale(78)).toBe(78);
    expect(snapUiScale(148)).toBe(150);
    expect(snapUiScale(103)).toBe(103);
    expect(snapUiScale(249)).toBe(250);
    expect(snapUiScale(298)).toBe(300);
    expect(snapUiScale(50)).toBe(50);
  });

  it("ignores the legacy local-only sync interval", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ ...defaultPreferences(), sync_interval_secs: 1 })
    );
    expect("sync_interval_secs" in loadPreferences()).toBe(false);
    expect("ui_size" in loadPreferences()).toBe(false);
  });

  it("defaults and persists the clock format", () => {
    expect(defaultPreferences().time_format).toBe("system");
    savePreferences({ ...defaultPreferences(), time_format: "24h" });
    expect(loadPreferences().time_format).toBe("24h");
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ ...defaultPreferences(), time_format: "moon" })
    );
    expect(loadPreferences().time_format).toBe("system");
  });

  it("defaults and normalizes the mark-read delay", () => {
    expect(defaultPreferences().mark_read_delay).toBe(0);
    for (const value of [2, 5, 10, 30, -1]) {
      savePreferences({ ...defaultPreferences(), mark_read_delay: value });
      expect(loadPreferences().mark_read_delay).toBe(value);
    }
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ ...defaultPreferences(), mark_read_delay: 999 })
    );
    expect(loadPreferences().mark_read_delay).toBe(0);
  });

  it("defaults and normalizes conversation order", () => {
    expect(defaultPreferences().message_order).toBe("newest");
    savePreferences({ ...defaultPreferences(), message_order: "oldest" });
    expect(loadPreferences().message_order).toBe("oldest");
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ ...defaultPreferences(), message_order: "sideways" })
    );
    expect(loadPreferences().message_order).toBe("newest");
  });

  it("defaults and persists attachment preferences", () => {
    const defaults = defaultPreferences();
    expect(defaults.attachment_thumbnails).toBe(true);
    expect(defaults.reveal_download).toBe(false);

    savePreferences({
      ...defaultPreferences(),
      attachment_thumbnails: false,
      reveal_download: true
    });
    const loaded = loadPreferences();
    expect(loaded.attachment_thumbnails).toBe(false);
    expect(loaded.reveal_download).toBe(true);
  });

  it("falls back per field for invalid attachment preferences", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        ...defaultPreferences(),
        attachment_thumbnails: "yes",
        reveal_download: true
      })
    );
    let loaded = loadPreferences();
    expect(loaded.attachment_thumbnails).toBe(true);
    expect(loaded.reveal_download).toBe(true);

    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        ...defaultPreferences(),
        attachment_thumbnails: false,
        reveal_download: "yes"
      })
    );
    loaded = loadPreferences();
    expect(loaded.attachment_thumbnails).toBe(false);
    expect(loaded.reveal_download).toBe(false);
  });

  it("defaults to the Aura system theme", () => {
    const defaults = defaultPreferences();
    expect(defaults.theme_choice).toBe("system");
    expect(defaults.theme_accent).toBe("");
    expect(defaults.theme_background).toBe("solid");
    expect(defaults.theme_panes).toBe("straight");
  });

  it("persists independent background and pane choices", () => {
    savePreferences({
      ...defaultPreferences(),
      theme_background: "glass",
      theme_panes: "rounded"
    });
    let loaded = loadPreferences();
    expect(loaded.theme_background).toBe("glass");
    expect(loaded.theme_panes).toBe("rounded");

    savePreferences({
      ...defaultPreferences(),
      theme_background: "solid",
      theme_panes: "rounded"
    });
    loaded = loadPreferences();
    expect(loaded.theme_background).toBe("solid");
    expect(loaded.theme_panes).toBe("rounded");
  });

  it("falls back for unknown background and pane values", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        ...defaultPreferences(),
        theme_background: "liquid",
        theme_panes: "bubbles"
      })
    );
    const loaded = loadPreferences();
    expect(loaded.theme_background).toBe("solid");
    expect(loaded.theme_panes).toBe("straight");
  });

  it("migrates legacy theme styles during load", () => {
    for (const [style, background, panes] of [
      ["classic", "solid", "straight"],
      ["frosted", "glass", "straight"],
      ["fancy", "glass", "rounded"]
    ] as const) {
      localStorage.setItem(
        "auramail.settings",
        JSON.stringify({
          ...defaultPreferences(),
          theme_style: style,
          theme_background: undefined,
          theme_panes: undefined
        })
      );
      const loaded = loadPreferences();
      expect(loaded.theme_background).toBe(background);
      expect(loaded.theme_panes).toBe(panes);
    }
  });

  it("persists a theme choice and valid accent", () => {
    savePreferences({
      ...defaultPreferences(),
      theme_choice: "catppuccin",
      theme_accent: "red"
    });
    const loaded = loadPreferences();
    expect(loaded.theme_choice).toBe("catppuccin");
    expect(loaded.theme_accent).toBe("red");
  });

  it("normalizes unknown themes and accents", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        ...defaultPreferences(),
        theme_choice: "unknown",
        theme_accent: "red"
      })
    );
    let loaded = loadPreferences();
    expect(loaded.theme_choice).toBe("system");
    expect(loaded.theme_accent).toBe("");

    savePreferences({
      ...defaultPreferences(),
      theme_choice: "catppuccin",
      theme_accent: "not-a-color"
    });
    loaded = loadPreferences();
    expect(loaded.theme_accent).toBe("blue");
  });

  it("migrates legacy Matugen and preset settings", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        theme_matugen: true,
        theme_mode: "dark"
      })
    );
    expect(loadPreferences().theme_choice).toBe("matugen");

    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        theme_mode: "dark",
        dark_preset: "dracula"
      })
    );
    expect(loadPreferences().theme_choice).toBe("dracula");

    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({
        theme_mode: "light",
        light_preset: "rose-pine-dawn",
        dark_preset: "rose-pine-moon"
      })
    );
    expect(loadPreferences().theme_choice).toBe("rose-pine");

    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ theme_mode: "dark" })
    );
    expect(loadPreferences().theme_choice).toBe("dark");
  });
});

describe("color vision", () => {
  it("defaults color vision to none and persists a valid mode", () => {
    expect(defaultPreferences().color_vision).toBe("none");
    savePreferences({ ...defaultPreferences(), color_vision: "tritanopia" });
    expect(loadPreferences().color_vision).toBe("tritanopia");
  });

  it("falls back to none for an invalid color vision mode", () => {
    localStorage.setItem(
      "auramail.settings",
      JSON.stringify({ ...defaultPreferences(), color_vision: "rainbow" })
    );
    expect(loadPreferences().color_vision).toBe("none");
  });

  it("applies the shifted palette to the theme variables", () => {
    applyThemePreferences(
      { ...defaultPreferences(), theme_choice: "dark", color_vision: "deuteranopia" },
      true
    );
    const expected = shiftPalette(auraDarkTheme, "deuteranopia");
    expect(document.documentElement.style.getPropertyValue("--base08"))
      .toBe(expected.colors.base08);
    expect(document.documentElement.style.getPropertyValue("--base0D"))
      .toBe(expected.colors.base0D);
  });

  it("sets the color-vision root attribute", () => {
    applyAccessibilityPreferences({
      ...defaultPreferences(),
      color_vision: "achromatopsia"
    });
    expect(document.documentElement.dataset.colorVision).toBe("achromatopsia");
  });

  it("shifts the loaded Matugen palette", () => {
    const payload: Record<string, string> = matugenPalette("#123456");
    applyMatugenPalette(payload);
    const palette: Base16 = {
      scheme: payload.scheme,
      colors: Object.fromEntries(
        base16Keys.map((key) => [key, payload[key]])
      ) as Base16["colors"]
    };
    applyThemeSettings(
      { ...defaultPreferences(), theme_choice: "matugen", color_vision: "tritanopia" },
      false
    );
    const expected = shiftPalette(palette, "tritanopia");
    expect(document.documentElement.style.getPropertyValue("--base0D"))
      .toBe(expected.colors.base0D);
  });

  it("applies color vision immediately when a Matugen palette arrives", () => {
    savePreferences({ ...defaultPreferences(), color_vision: "tritanopia" });
    const payload: Record<string, string> = matugenPalette("#123456");
    applyMatugenPalette(payload);
    const palette: Base16 = {
      scheme: payload.scheme,
      colors: Object.fromEntries(
        base16Keys.map((key) => [key, payload[key]])
      ) as Base16["colors"]
    };
    const expected = shiftPalette(palette, "tritanopia");
    expect(document.documentElement.style.getPropertyValue("--base0D"))
      .toBe(expected.colors.base0D);
  });

  it("patches preferences without clobbering unrelated fields", () => {
    savePreferences({
      ...defaultPreferences(),
      dark_email: true,
      reduce_motion: true
    });
    updatePreferences({ theme_choice: "nord" });
    const loaded = loadPreferences();
    expect(loaded.dark_email).toBe(true);
    expect(loaded.reduce_motion).toBe(true);
    expect(loaded.theme_choice).toBe("nord");
  });
});

describe("theme application", () => {
  it("lets a Matugen palette override the active preset", () => {
    applyMatugenPalette(matugenPalette("#abcdef"));
    applyThemeSettings(
      { ...defaultPreferences(), theme_choice: "matugen" },
      false
    );
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#abcdef");
  });

  it("returns to the preset when Matugen is disabled", () => {
    applyMatugenPalette(matugenPalette("#abcdef"));
    applyThemeSettings(
      { ...defaultPreferences(), theme_choice: "catppuccin" },
      false
    );
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#1e1e2e");
  });

  it("rejects invalid Matugen payloads without changing the palette", () => {
    applyMatugenPalette(matugenPalette("#abcdef"));
    expect(() =>
      applyMatugenPalette({
        scheme: "Broken",
        ...Object.fromEntries(base16Keys.map((key) => [key, "#fff"]))
      })
    ).toThrow();
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#abcdef");
  });
});

describe("theme choices", () => {
  it("resolves Aura system and explicit Aura themes", () => {
    applyThemeSettings({ ...defaultPreferences(), theme_choice: "system" }, false);
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#F7F8FD");
    applyThemeSettings({ ...defaultPreferences(), theme_choice: "system" }, true);
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#0A0A0A");
    applyThemeSettings({ ...defaultPreferences(), theme_choice: "light" }, true);
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#F7F8FD");
  });

  it("applies a palette accent", () => {
    applyThemeSettings(
      { ...defaultPreferences(), theme_choice: "catppuccin", theme_accent: "red" },
      false
    );
    expect(document.documentElement.style.getPropertyValue("--base0D")).toBe("#f38ba8");
  });

  it("applies independent background and pane choices to the document", () => {
    applyThemeSettings(
      {
        ...defaultPreferences(),
        theme_background: "glass",
        theme_panes: "rounded"
      },
      false
    );
    expect(document.documentElement.dataset.themeBackground).toBe("glass");
    expect(document.documentElement.dataset.themePanes).toBe("rounded");
    expect(document.documentElement.dataset.themeGlass).toBe("true");

    applyThemeSettings(
      {
        ...defaultPreferences(),
        theme_background: "solid",
        theme_panes: "straight"
      },
      false
    );
    expect(document.documentElement.dataset.themeBackground).toBe("solid");
    expect(document.documentElement.dataset.themePanes).toBe("straight");
    expect(document.documentElement.dataset.themeGlass).toBe("false");
  });
});

describe("Matugen reload", () => {
  const prefs = (themeMatugen: boolean) => ({
    ...defaultPreferences(),
    theme_choice: themeMatugen ? ("matugen" as const) : ("catppuccin" as const)
  });
  const broken = () => ({
    scheme: "Broken",
    ...Object.fromEntries(base16Keys.map((key) => [key, "#fff"]))
  });

  it("skips loading entirely when Matugen is disabled", async () => {
    const load = vi.fn(async () => matugenPalette("#abcdef"));
    applyThemeSettings(prefs(false), false);

    const result = await reloadMatugenTheme(false, load, prefs(false), false);

    expect(result).toEqual({ status: "disabled" });
    expect(load).not.toHaveBeenCalled();
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#1e1e2e");
  });

  it("preserves the applied palette when the file is invalid", async () => {
    applyMatugenPalette(matugenPalette("#abcdef"));
    applyThemeSettings(prefs(true), false);

    const result = await reloadMatugenTheme(true, async () => broken(), prefs(true), false);

    expect(result.status).toBe("invalid");
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#abcdef");
  });

  it("preserves the applied palette when the file is missing", async () => {
    applyMatugenPalette(matugenPalette("#abcdef"));
    applyThemeSettings(prefs(true), false);

    const result = await reloadMatugenTheme(true, async () => null, prefs(true), false);

    expect(result).toEqual({ status: "missing" });
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#abcdef");
  });

  it("applies a valid reloaded palette", async () => {
    applyThemeSettings(prefs(true), false);

    const result = await reloadMatugenTheme(
      true,
      async () => matugenPalette("#123456"),
      prefs(true),
      false
    );

    expect(result.status).toBe("applied");
    expect(document.documentElement.style.getPropertyValue("--base00")).toBe("#123456");
  });
});
