import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AuraRail from "./AuraRail.svelte";
import { api } from "../api";
import { app } from "../stores/app.svelte";
import type { AiAuraRail, Message, MessageDetail } from "../types";

vi.mock("../api", () => ({
  api: {
    aiAuraRail: vi.fn(),
    aiAuraAsk: vi.fn(),
    aiChat: vi.fn()
  }
}));

function message(id = 1): Message {
  return {
    id,
    account_id: 1,
    folder_id: 1,
    uid: id,
    message_id: `<${id}@example.com>`,
    references: null,
    thread_key: `thread-${id}`,
    from_addr: "priya@northstar.studio",
    from_name: "Priya Raman",
    to_addrs: ["me@example.com"],
    cc_addrs: [],
    subject: "Revised quote",
    date: 100,
    snippet: "Can you confirm the discount?",
    is_seen: true,
    is_starred: false,
    has_attachments: false,
    body_cached: true,
    tab_id: null
  };
}

function detail(id = 1): MessageDetail {
  return {
    message: message(id),
    text_body: "Can you confirm the discount?",
    html_body: null,
    image_html: null,
    attachments: []
  };
}

const initialRail: AiAuraRail = {
  summary: "Priya wants a discount confirmation before Thursday.",
  ask: "Confirm the 12% discount.",
  deadline: "Thursday",
  tone: "Direct",
  question: "Which package does the discount cover?",
  options: ["Full package", "Base shoot"],
  drafts: [],
  messageCount: 3
};

const rail: AiAuraRail = {
  ...initialRail,
  question: null,
  options: [],
  drafts: [
    { tone: "direct", text: "Yes, it applies to the full package." },
    { tone: "warm", text: "Happy to confirm — the discount still applies." },
    { tone: "concise", text: "Confirmed — it still applies." }
  ]
};

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  localStorage.clear();
  app.aiConfigured = true;
  vi.mocked(api.aiAuraRail).mockReset();
  vi.mocked(api.aiAuraAsk).mockReset();
  vi.mocked(api.aiChat).mockReset();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  app.aiConfigured = false;
});

describe("AuraRail", () => {
  it("starts wider and can be resized from the keyboard", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(rail);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    const railElement = screen.getByRole("complementary", { name: "Aura" });
    expect((railElement as HTMLElement).style.width).toBe("360px");

    const handle = screen.getByRole("separator", { name: "Resize Aura" });
    await fireEvent.keyDown(handle, { key: "ArrowLeft" });
    await tick();
    expect((railElement as HTMLElement).style.width).toBe("376px");

    await fireEvent.dblClick(handle);
    await tick();
    expect((railElement as HTMLElement).style.width).toBe("360px");
  });

  it("shows a reading status during the first load", async () => {
    const pending = deferred<AiAuraRail>();
    vi.mocked(api.aiAuraRail).mockReturnValueOnce(pending.promise);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(await screen.findByText("Reading the thread…")).toBeInTheDocument();
    pending.resolve(initialRail);
    await tick();
  });

  it("disables clarification controls while updating", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValueOnce(initialRail);
    const pending = deferred<AiAuraRail>();
    vi.mocked(api.aiAuraRail).mockReturnValueOnce(pending.promise);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.click(screen.getByRole("button", { name: "Full package" }));

    expect(await screen.findByText("Drafting replies…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Full package" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Ask Aura" })).toBeDisabled();
    pending.resolve(rail);
    await tick();
  });

  it("does not ask while a clarification is still drafting", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValueOnce(initialRail);
    const pending = deferred<AiAuraRail>();
    vi.mocked(api.aiAuraRail).mockReturnValueOnce(pending.promise);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.input(
      screen.getByLabelText("Ask Aura or say how you want to reply"),
      { target: { value: "Tell them I can attend." } }
    );
    await fireEvent.click(screen.getByRole("button", { name: "Full package" }));

    expect(await screen.findByText("Drafting replies…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ask Aura" })).toBeDisabled();

    pending.resolve(rail);
    await tick();
  });

  it("asks a focused question before drafting", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(initialRail);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(
      await screen.findByText("Which package does the discount cover?")
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Answer above, or describe your reply below."
      )
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Use .* reply/ })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Generate more replies" })
    ).not.toBeInTheDocument();
  });

  it("renders the brief, context, and answer drafts", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(rail);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(await screen.findByText("Confirm the 12% discount.")).toBeInTheDocument();
    expect(screen.getByText(rail.summary)).toBeInTheDocument();
    expect(screen.getByText("This thread · 3 messages")).toBeInTheDocument();
    expect(screen.getAllByText("Direct").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Warm").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Concise").length).toBeGreaterThan(0);
    expect(screen.getByText(rail.drafts[0]!.text)).toBeInTheDocument();
  });

  it("hands the chosen draft to the composer", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(rail);
    const onUseDraft = vi.fn().mockResolvedValue(undefined);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft, onClose: vi.fn() }
    });

    const button = await screen.findByRole("button", {
      name: "Use Direct reply"
    });
    await fireEvent.click(button);

    expect(onUseDraft).toHaveBeenCalledWith(rail.drafts[0]!.text);
  });

  it("regenerates with a clarification chip", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValueOnce(initialRail);
    vi.mocked(api.aiAuraRail).mockResolvedValueOnce(rail);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.click(screen.getByRole("button", { name: "Full package" }));

    await waitFor(() =>
      expect(api.aiAuraRail).toHaveBeenLastCalledWith(1, "Full package", false)
    );
    expect(await screen.findByText(rail.drafts[0]!.text)).toBeInTheDocument();
  });

  it("asks a thread-scoped follow-up and renders the answer", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(initialRail);
    vi.mocked(api.aiAuraAsk).mockResolvedValue({
      answer: "The thread says Thursday.",
      drafts: []
    });
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.input(
      screen.getByLabelText("Ask Aura or say how you want to reply"),
      { target: { value: "When is the deadline?" } }
    );
    await fireEvent.click(screen.getByRole("button", { name: "Ask Aura" }));

    expect(await screen.findByText("The thread says Thursday.")).toBeInTheDocument();
    expect(
      screen.getByText("Which package does the discount cover?")
    ).toBeInTheDocument();
    expect(api.aiAuraAsk).toHaveBeenCalledWith(
      1,
      "When is the deadline?",
      [],
      [],
      false
    );
  });

  it("turns reply guidance into new drafts", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(initialRail);
    vi.mocked(api.aiAuraAsk).mockResolvedValue({
      answer: "Added three options below.",
      drafts: [
        { tone: "direct", text: "I can attend; please send the agenda." },
        { tone: "warm", text: "Happy to join — could you share the agenda first?" },
        { tone: "concise", text: "Yes, if you send the agenda." }
      ]
    });
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.input(
      screen.getByLabelText("Ask Aura or say how you want to reply"),
      { target: { value: "I want to accept, but ask for the agenda first." } }
    );
    await fireEvent.click(screen.getByRole("button", { name: "Ask Aura" }));

    expect(await screen.findByText("Added three options below.")).toBeInTheDocument();
    expect(
      await screen.findByText("I can attend; please send the agenda.")
    ).toBeInTheDocument();
    expect(
      screen.getByRole("log").textContent
    ).toContain("I can attend; please send the agenda.");
    expect(
      screen.getAllByRole("button", { name: /Use .* reply/ })
    ).toHaveLength(3);
    expect(screen.queryByText("Suggested replies")).not.toBeInTheDocument();
    expect(
      screen.queryByText("Which package does the discount cover?")
    ).not.toBeInTheDocument();
    expect(api.aiAuraAsk).toHaveBeenCalledWith(
      1,
      "I want to accept, but ask for the agenda first.",
      [],
      [],
      false
    );
  });

  it("appends guidance drafts to existing options", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue(rail);
    vi.mocked(api.aiAuraAsk).mockResolvedValue({
      answer: "Added three alternatives below.",
      drafts: [
        { tone: "direct", text: "Another direct option." },
        { tone: "warm", text: "Another warm option." },
        { tone: "concise", text: "Another short option." }
      ]
    });
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    await screen.findByText("Confirm the 12% discount.");
    await fireEvent.input(
      screen.getByLabelText("Ask Aura or say how you want to reply"),
      { target: { value: "Give me a different angle." } }
    );
    await fireEvent.click(screen.getByRole("button", { name: "Ask Aura" }));

    expect(await screen.findByText("Another direct option.")).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /Use .* reply/ })
    ).toHaveLength(6);
    const conversation = screen.getByRole("log");
    expect(conversation.textContent).toContain("Another direct option.");
    expect(conversation.textContent).not.toContain(rail.drafts[0]!.text);
    expect(api.aiAuraAsk).toHaveBeenCalledWith(
      1,
      "Give me a different angle.",
      [],
      rail.drafts.map((draft) => draft.text),
      false
    );
  });

  it("shows a retryable error when the brief cannot be read", async () => {
    vi.mocked(api.aiAuraRail)
      .mockRejectedValueOnce(new Error("model offline"))
      .mockResolvedValueOnce(rail);
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(
      await screen.findByText("Aura couldn't read this thread.")
    ).toBeInTheDocument();
    expect(screen.getByText("model offline")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByText("Confirm the 12% discount.")).toBeInTheDocument();
    expect(api.aiAuraRail).toHaveBeenCalledTimes(2);
  });

  it("surfaces a string rejection from Tauri", async () => {
    vi.mocked(api.aiAuraRail).mockRejectedValue("AI request failed (401)");
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(
      await screen.findByText("AI request failed (401)")
    ).toBeInTheDocument();
  });

  it("shows a setup card instead of calling the model when AI is off", () => {
    app.aiConfigured = false;
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(screen.getByText(/isn.t configured yet/i)).toBeInTheDocument();
    expect(api.aiAuraRail).not.toHaveBeenCalled();
  });

  it("renders duplicate option and tone labels without duplicate keys", async () => {
    vi.mocked(api.aiAuraRail).mockResolvedValue({
      ...rail,
      question: "Which package?",
      options: ["Full package", "Full package"],
      drafts: [
        { tone: "direct", text: "First direct draft." },
        { tone: "direct", text: "Second direct draft." },
        { tone: "concise", text: "Short draft." }
      ]
    });
    render(AuraRail, {
      props: { detail: detail(), onUseDraft: vi.fn(), onClose: vi.fn() }
    });

    expect(
      await screen.findAllByRole("button", { name: /Use .* reply/ })
    ).toHaveLength(3);
    expect(
      screen.getAllByRole("button", { name: "Full package" })
    ).toHaveLength(2);
  });

  it("ignores a stale brief when the message changes", async () => {
    const first = deferred<AiAuraRail>();
    const second = deferred<AiAuraRail>();
    vi.mocked(api.aiAuraRail)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const onUseDraft = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(AuraRail, {
      props: { detail: detail(1), onUseDraft, onClose }
    });
    await tick();

    await rerender({ detail: detail(2), onUseDraft, onClose });
    second.resolve({ ...rail, ask: "Second thread ask" });
    await tick();
    first.resolve({ ...rail, ask: "First thread ask" });
    await tick();

    expect(screen.getByText("Second thread ask")).toBeInTheDocument();
    expect(screen.queryByText("First thread ask")).not.toBeInTheDocument();
  });
});
