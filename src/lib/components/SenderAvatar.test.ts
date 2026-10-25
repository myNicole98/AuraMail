import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import SenderAvatar from "./SenderAvatar.svelte";

const mocks = vi.hoisted(() => ({ getSenderAvatar: vi.fn() }));
vi.mock("../api", () => ({ api: { getSenderAvatar: mocks.getSenderAvatar } }));

afterEach(() => {
  cleanup();
  mocks.getSenderAvatar.mockReset();
});

describe("SenderAvatar", () => {
  it("shows initials immediately and swaps in the resolved image", async () => {
    let resolve: (value: string | null) => void = () => {};
    mocks.getSenderAvatar.mockReturnValue(
      new Promise<string | null>((done) => {
        resolve = done;
      })
    );

    const { container } = render(SenderAvatar, {
      props: { name: "Ada Lovelace", email: "ada@example.com", enabled: true }
    });

    expect(screen.getByText("AL")).toBeInTheDocument();
    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(mocks.getSenderAvatar).toHaveBeenCalledWith("ada@example.com");

    resolve("data:image/png;base64,YWJj");
    await tick();
    await Promise.resolve();

    expect(container.querySelector("img")).toHaveAttribute(
      "src",
      "data:image/png;base64,YWJj"
    );
    expect(screen.queryByText("AL")).not.toBeInTheDocument();
  });

  it("keeps initials when no picture is available", async () => {
    mocks.getSenderAvatar.mockResolvedValue(null);
    const { container } = render(SenderAvatar, {
      props: { name: "Ada Lovelace", email: "ada@example.com", enabled: true }
    });

    await tick();
    await Promise.resolve();

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(screen.getByText("AL")).toBeInTheDocument();
  });

  it("does not look up pictures when disabled and hides an already-loaded one", async () => {
    mocks.getSenderAvatar.mockResolvedValue("data:image/png;base64,YWJj");
    const { container, rerender } = render(SenderAvatar, {
      props: { name: "Ada Lovelace", email: "ada@example.com", enabled: true }
    });

    await tick();
    await Promise.resolve();
    expect(container.querySelector("img")).toBeInTheDocument();

    await rerender({
      name: "Ada Lovelace",
      email: "ada@example.com",
      enabled: false
    });

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(screen.getByText("AL")).toBeInTheDocument();
  });

  it("does not call the backend when disabled", () => {
    render(SenderAvatar, {
      props: { name: "Ada Lovelace", email: "ada@example.com", enabled: false }
    });

    expect(mocks.getSenderAvatar).not.toHaveBeenCalled();
    expect(screen.getByText("AL")).toBeInTheDocument();
  });

  it("falls back to initials when the image fails to decode", async () => {
    mocks.getSenderAvatar.mockResolvedValue("data:image/png;base64,bad");
    const { container } = render(SenderAvatar, {
      props: { name: "Ada Lovelace", email: "ada@example.com", enabled: true }
    });

    await tick();
    await Promise.resolve();
    const image = container.querySelector("img");
    expect(image).toBeInTheDocument();

    await fireEvent.error(image!);
    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(screen.getByText("AL")).toBeInTheDocument();
  });
});
