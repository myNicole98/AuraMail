import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "./AppShell.svelte";
import { markOnboardingComplete, ONBOARDING_STORAGE_KEY } from "../onboarding";
import { app } from "../stores/app.svelte";
import { appearance } from "../stores/appearance.svelte";

function pointer(type: string, clientX: number): MouseEvent {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX,
    button: 0
  });
  Object.defineProperty(event, "isPrimary", { value: true });
  Object.defineProperty(event, "pointerId", { value: 1 });
  return event;
}

beforeEach(() => {
  Object.defineProperty(document.documentElement, "clientWidth", {
    configurable: true,
    value: 1200
  });
  if (!Element.prototype.animate) {
    Object.defineProperty(Element.prototype, "animate", {
      configurable: true,
      value: () => ({
        cancel() {},
        finish() {},
        onfinish: null as (() => void) | null
      })
    });
  }
  localStorage.clear();
  markOnboardingComplete();
});
afterEach(() => {
  cleanup();
  appearance.setLayoutMode("classic");
  appearance.setMessagePreview("preview");
  app.clearSelection();
});

describe("panel splitters", () => {
  it("shows onboarding for a fresh profile", async () => {
    localStorage.removeItem(ONBOARDING_STORAGE_KEY);
    render(AppShell);
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: /welcome to auramail/i })
      ).toBeInTheDocument()
    );
  });

  it("resizes the message list while dragging", async () => {
    const { container } = render(AppShell);
    await tick();
    const splitters = container.querySelectorAll(".splitter");
    expect(splitters.length).toBe(2);

    splitters[1].dispatchEvent(pointer("pointerdown", 400));
    await tick();
    const overlay = container.querySelector(".drag-overlay");
    expect(overlay).not.toBeNull();

    overlay!.dispatchEvent(pointer("pointermove", 500));
    await tick();
    expect((container.querySelector(".shell") as HTMLElement).style.gridTemplateColumns)
      .toContain("580px");

    overlay!.dispatchEvent(pointer("pointerup", 500));
    await tick();
    expect(container.querySelector(".drag-overlay")).toBeNull();
  });

  it("shrinks immediately from the visible width instead of a clamped stored width", async () => {
    localStorage.setItem(
      "auramail.layout",
      JSON.stringify({ sidebarWidth: 200, listWidth: 640, sidebarCollapsed: false })
    );
    const { container } = render(AppShell);
    await tick();
    const splitters = container.querySelectorAll(".splitter");

    splitters[1].dispatchEvent(pointer("pointerdown", 400));
    await tick();
    const overlay = container.querySelector(".drag-overlay");

    overlay!.dispatchEvent(pointer("pointermove", 380));
    await tick();
    expect((container.querySelector(".shell") as HTMLElement).style.gridTemplateColumns)
      .toContain("560px");
  });

  it("puts the search field above the tabs", async () => {
    const { container } = render(AppShell);
    await tick();
    await tick();
    const search = container.querySelector(".list-head .search-row");
    const tabs = container.querySelector(".list-head .tabs-bar");
    expect(search).not.toBeNull();
    expect(tabs).not.toBeNull();
    expect(
      Boolean(search!.compareDocumentPosition(tabs!) & Node.DOCUMENT_POSITION_FOLLOWING)
    ).toBe(true);
  });

  it("applies compact message rows", async () => {
    const { container } = render(AppShell);
    await tick();
    await tick();
    appearance.setMessagePreview("compact");
    await tick();
    expect(container.querySelector(".message-list")?.classList.contains("compact-preview"))
      .toBe(true);
  });

  it("applies the stacked layout", async () => {
    const { container } = render(AppShell);
    await tick();
    await tick();
    appearance.setLayoutMode("stacked");
    await tick();
    const shell = container.querySelector(".shell") as HTMLElement;
    expect(shell.classList.contains("stacked")).toBe(true);
    expect(shell.style.gridTemplateRows).toContain("280px 1px");
  });

  it("applies reader focus only while reading", async () => {
    const { container } = render(AppShell);
    await tick();
    await tick();
    appearance.setLayoutMode("reader");
    await tick();
    const shell = container.querySelector(".shell") as HTMLElement;
    expect(shell.classList.contains("reader-focus")).toBe(false);

    app.selectedMessageId = 1;
    await tick();
    expect(shell.classList.contains("reader-focus")).toBe(true);
    expect(shell.style.gridTemplateColumns).toContain("0px 0px");
  });

  it("toggles the Aura rail with Ctrl+K", async () => {
    render(AppShell);
    await tick();
    await tick();
    app.selectedMessageId = 1;

    await fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(app.auraRailOpen).toBe(true);

    await fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(app.auraRailOpen).toBe(false);
  });

  it("closes the Aura rail before the open message on Escape", async () => {
    render(AppShell);
    await tick();
    await tick();
    app.selectedMessageId = 1;
    app.auraRailOpen = true;

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(app.auraRailOpen).toBe(false);
    expect(app.selectedMessageId).toBe(1);
  });

  it("opens settings from the Aura setup event", async () => {
    render(AppShell);
    await tick();
    await tick();

    window.dispatchEvent(new CustomEvent("auramail:open-settings"));
    await tick();

    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("closes the open message on Escape", async () => {
    render(AppShell);
    await tick();
    await tick();
    app.selectedMessageId = 1;
    await tick();

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(app.selectedMessageId).toBeNull();
  });

  it("lets a bulk selection consume Escape before closing the reader", async () => {
    render(AppShell);
    await tick();
    await tick();
    app.selectedMessageId = 1;
    const consume = (event: Event) => event.preventDefault();
    window.addEventListener("auramail:clear-mail-selection", consume);
    try {
      await fireEvent.keyDown(window, { key: "Escape" });
    } finally {
      window.removeEventListener("auramail:clear-mail-selection", consume);
    }

    expect(app.selectedMessageId).toBe(1);
  });

  it("leaves the reader open when Escape is pressed in an input", async () => {
    render(AppShell);
    await tick();
    await tick();
    app.selectedMessageId = 1;
    await tick();

    await fireEvent.keyDown(screen.getByLabelText("Search all mail"), {
      key: "Escape"
    });

    expect(app.selectedMessageId).toBe(1);
  });

  it("resizes the sidebar while dragging", async () => {
    const { container } = render(AppShell);
    await tick();
    const splitters = container.querySelectorAll(".splitter");

    splitters[0].dispatchEvent(pointer("pointerdown", 400));
    await tick();
    const overlay = container.querySelector(".drag-overlay");

    overlay!.dispatchEvent(pointer("pointermove", 450));
    await tick();
    expect((container.querySelector(".shell") as HTMLElement).style.gridTemplateColumns)
      .toContain("250px");
  });
});
