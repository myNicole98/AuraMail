import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import AttachmentThumbnail from "./AttachmentThumbnail.svelte";
import type { Attachment } from "../types";

const attachment = (overrides: Partial<Attachment> = {}): Attachment => ({
  id: 7,
  message_id: 1,
  filename: "photo.png",
  mime_type: "image/png",
  size: 1024,
  content_id: null,
  ...overrides
});

const mocks = vi.hoisted(() => ({ attachmentPreview: vi.fn() }));
vi.mock("../api", () => ({
  api: { attachmentPreview: mocks.attachmentPreview }
}));

afterEach(() => {
  cleanup();
  mocks.attachmentPreview.mockReset();
});

describe("AttachmentThumbnail", () => {
  it("loads and shows an image preview", async () => {
    mocks.attachmentPreview.mockResolvedValue({
      mime_type: "image/png",
      data_url: "data:image/png;base64,YWJj"
    });
    render(AttachmentThumbnail, {
      props: { attachment: attachment(), enabled: true }
    });
    const image = await screen.findByAltText("Preview of photo.png");
    expect(image).toHaveAttribute("src", "data:image/png;base64,YWJj");
  });

  it("shows the plain chip when previews are disabled", () => {
    render(AttachmentThumbnail, {
      props: { attachment: attachment({ id: 8 }), enabled: false }
    });
    expect(mocks.attachmentPreview).not.toHaveBeenCalled();
    expect(screen.queryByAltText("Preview of photo.png")).not.toBeInTheDocument();
  });

  it("falls back silently when no preview is available", async () => {
    mocks.attachmentPreview.mockResolvedValue(null);
    render(AttachmentThumbnail, {
      props: { attachment: attachment({ id: 9 }), enabled: true }
    });
    await tick();
    expect(mocks.attachmentPreview).toHaveBeenCalledWith(9);
    expect(screen.queryByAltText("Preview of photo.png")).not.toBeInTheDocument();
  });
});
