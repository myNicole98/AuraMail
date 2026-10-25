import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import ClassificationTabs from "./ClassificationTabs.svelte";
import { app } from "../stores/app.svelte";
import type { Tab } from "../types";

function tab(id: number, name: string, color: string, icon: string): Tab {
  return {
    id,
    space_id: 1,
    name,
    description: `${name} mail.`,
    position: id - 1,
    removable: true,
    primary: id === 1,
    color,
    icon
  };
}

beforeEach(() => {
  app.tabs = [
    tab(1, "Primary", "#6da4ff", "inbox"),
    tab(2, "Updates", "#58c98b", "bell")
  ];
  app.selectedTabId = null;
  app.tabUnreadCounts = { all: 5, tabs: { 1: 3, 2: 2 } };
});

afterEach(cleanup);

describe("ClassificationTabs", () => {
  it("shows the All tab and unread-thread counts", () => {
    const { container } = render(ClassificationTabs);
    expect(container.textContent).toContain("All");
    expect(container.textContent).toContain("Primary");
    expect(container.textContent).toContain("Updates");
    expect(container.textContent).toContain("5");
    expect(container.textContent).toContain("3");
    expect(container.textContent).toContain("2");
  });

  it("opens the editor directly on right-click without a context menu", async () => {
    render(ClassificationTabs);
    const primary = screen.getByText("Primary").closest("button");
    expect(primary).not.toBeNull();

    await fireEvent.contextMenu(primary!);

    expect(screen.getByRole("dialog", { name: "Edit tab" })).toBeInTheDocument();
    expect(screen.getByLabelText("Description")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Delete tab/ })).toBeInTheDocument();
  });

  it("keeps All fixed outside the scrollable tabs", () => {
    const { container } = render(ClassificationTabs);
    const all = screen.getByText("All").closest(".tab-wrap");
    expect(all?.closest(".tab-scroll")).toBeNull();
    expect(container.querySelector(".tab-scroll .tab-wrap.all")).toBeNull();
  });
});
