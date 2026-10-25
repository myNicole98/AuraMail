import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import BootSplash from "./BootSplash.svelte";

afterEach(() => {
  cleanup();
});

describe("BootSplash", () => {
  it("renders the brand with four concentric aura rings", () => {
    const { container } = render(BootSplash);

    expect(
      screen.getByRole("status", { name: "Starting AuraMail" })
    ).toBeInTheDocument();
    expect(screen.getByText("AuraMail")).toBeInTheDocument();
    expect(container.querySelectorAll(".ring")).toHaveLength(4);
  });

  it("disables motion when reduced motion is requested", () => {
    const { container } = render(BootSplash, {
      props: { reducedMotion: true }
    });

    expect(container.querySelector(".boot-splash")).toHaveClass("reduced-motion");
  });
});
