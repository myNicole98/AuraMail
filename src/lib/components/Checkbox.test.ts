import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import Checkbox from "./Checkbox.svelte";

afterEach(cleanup);

describe("Checkbox", () => {
  it("reports the new checked state", async () => {
    const onchange = vi.fn();
    render(Checkbox, {
      props: {
        checked: false,
        label: "Select all",
        onchange
      }
    });

    await fireEvent.click(screen.getByRole("checkbox", { name: "Select all" }));
    expect(onchange).toHaveBeenCalledWith(true);
  });

  it("shows the mixed state when some items are selected", () => {
    render(Checkbox, {
      props: {
        checked: false,
        indeterminate: true,
        label: "2 selected",
        onchange: vi.fn()
      }
    });

    const input = screen.getByRole("checkbox", { name: "2 selected" }) as HTMLInputElement;
    expect(input.indeterminate).toBe(true);
  });

  it("is disabled when requested", () => {
    render(Checkbox, {
      props: {
        checked: false,
        disabled: true,
        label: "Select all",
        onchange: vi.fn()
      }
    });

    expect(screen.getByRole("checkbox", { name: "Select all" })).toBeDisabled();
  });
});
