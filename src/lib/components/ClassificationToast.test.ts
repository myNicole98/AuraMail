import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import ClassificationToast from "./ClassificationToast.svelte";
import { appearance } from "../stores/appearance.svelte";

const progress = {
  total: 3,
  done: 1,
  aborting: false,
  abortable: true
};

beforeEach(() => {
  appearance.setReduceMotion(false);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  appearance.setReduceMotion(false);
});

describe("ClassificationToast", () => {
  it("shows progress with a loading icon and abort button", async () => {
    const onAbort = vi.fn();
    const { container } = render(ClassificationToast, {
      props: {
        message: "Classified 1 of 3 messages",
        progress,
        action: null,
        onAbort,
        onReclassify: vi.fn(),
        onDismiss: vi.fn()
      }
    });

    expect(screen.getByText("Classified 1 of 3 messages")).toBeInTheDocument();
    expect(container.querySelector(".toast-spinner")).not.toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Abort" }));
    expect(onAbort).toHaveBeenCalledTimes(1);
  });

  it("expands after the icon entrance delay", async () => {
    vi.useFakeTimers();
    const { container } = render(ClassificationToast, {
      props: {
        message: "Classified 3 messages.",
        progress: null,
        action: null,
        onAbort: vi.fn(),
        onReclassify: vi.fn(),
        onDismiss: vi.fn()
      }
    });
    const toast = container.querySelector(".toast");
    expect(toast?.classList.contains("expanded")).toBe(false);

    await vi.advanceTimersByTimeAsync(260);
    await tick();
    expect(toast?.classList.contains("expanded")).toBe(true);
  });

  it("shows the completion message with its action and dismiss control", async () => {
    const onDismiss = vi.fn();
    render(ClassificationToast, {
      props: {
        message: "Classified 3 messages.",
        progress: null,
        action: "reclassify",
        onAbort: vi.fn(),
        onReclassify: vi.fn(),
        onDismiss
      }
    });

    expect(screen.getByText("Classified 3 messages.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reclassify" })).toBeInTheDocument();
    await fireEvent.click(
      screen.getByRole("button", { name: "Dismiss classification notice" })
    );
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
