import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import ClassificationTabs from "./ClassificationTabs.svelte";
import { app } from "../stores/app.svelte";
import { TAB_COLORS } from "../tabAppearance";
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

  it("opens the create editor with the full color row and validates the name", async () => {
    render(ClassificationTabs);
    await fireEvent.click(
      screen.getByRole("button", { name: "Add classification tab" })
    );

    expect(screen.getByRole("dialog", { name: "New tab" })).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /Use color/ })
    ).toHaveLength(TAB_COLORS.length);
    expect(screen.getByLabelText("Custom tab color")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Create tab" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Give this tab a name.");
    expect(screen.getByLabelText("Name")).toHaveFocus();
  });

  it("marks the custom color control when a custom color is chosen", async () => {
    const { container } = render(ClassificationTabs);
    await fireEvent.click(
      screen.getByRole("button", { name: "Add classification tab" })
    );

    await fireEvent.input(screen.getByLabelText("Custom tab color"), {
      target: { value: "#ff5500" }
    });

    expect(container.querySelector(".custom-color")).toHaveClass("active");
    expect(screen.getByText("#FF5500")).toBeInTheDocument();
  });
});
