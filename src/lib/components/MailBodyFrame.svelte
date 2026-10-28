<script lang="ts">
  import { onMount } from "svelte";
  import { bodyCsp } from "../reader";
  import { appearance } from "../stores/appearance.svelte";

  const SKELETON = `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${bodyCsp()}"><style></style></head><body></body></html>`;

  let {
    html,
    onReady,
    onOpenLink,
    showQuotes = false,
    visible = true,
    transitionKey = ""
  }: {
    html: string;
    onReady?: () => void;
    onOpenLink: (href: string) => void;
    showQuotes?: boolean;
    visible?: boolean;
    transitionKey?: string | number;
  } = $props();

  let frame = $state<HTMLIFrameElement | null>(null);
  let wrapper = $state<HTMLDivElement | null>(null);
  let documentRef = $state<Document | null>(null);
  let mounted = true;
  let observer: ResizeObserver | null = null;
  let measuring = false;
  let lastHeight = 0;
  let appliedHtml = "";
  let lastTransitionKey: string | null = null;
  let fadeAnimation: Animation | null = null;

  function contentHeight(doc: Document): number {
    return Math.max(
      80,
      doc.documentElement.scrollHeight,
      doc.body?.scrollHeight ?? 0
    );
  }

  function resize(force = false) {
    const doc = frame?.contentDocument;
    if (!frame || !doc || measuring) return;
    measuring = true;
    frame.style.height = "80px";
    const height = contentHeight(doc);
    if (force || Math.abs(height - lastHeight) >= 2) {
      frame.style.height = `${height}px`;
      lastHeight = height;
    } else {
      frame.style.height = `${lastHeight}px`;
    }
    measuring = false;
  }

  function applyAnimationPreference(doc: Document): void {
    let style = doc.getElementById("auramail-disable-animations");
    if (!appearance.reduceMotion) {
      style?.remove();
      return;
    }
    if (!style) {
      style = doc.createElement("style");
      style.id = "auramail-disable-animations";
      style.textContent =
        "*,*::before,*::after{animation:none!important;transition:none!important;scroll-behavior:auto!important}";
      (doc.head ?? doc.documentElement).append(style);
    }
  }

  function applyTextSpacing(doc: Document): void {
    const id = "auramail-text-spacing";
    let style = doc.getElementById(id);
    if (!appearance.textSpacing) {
      style?.remove();
      return;
    }
    if (!style) {
      style = doc.createElement("style");
      style.id = id;
      style.textContent =
        "body,body *:not(svg):not(svg *){line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}body{overflow-wrap:anywhere}p{margin-block-end:2em!important}";
      (doc.head ?? doc.documentElement).append(style);
    }
  }

  $effect(() => {
    appearance.textSpacing;
    if (documentRef) applyTextSpacing(documentRef);
  });

  function onClick(event: MouseEvent) {
    const target = event.target as Element | null;
    const anchor = target?.closest?.("a");
    if (!anchor) return;
    event.preventDefault();
    const href = (anchor.getAttribute("href") ?? "").trim();
    if (href) onOpenLink(href);
  }

  function applyDocument(nextHtml: string): void {
    const doc = documentRef;
    if (!doc || !doc.body || appliedHtml === nextHtml) return;
    appliedHtml = nextHtml;

    const parsed = new DOMParser().parseFromString(nextHtml, "text/html");
    const parsedStyle = parsed.head.querySelector("style");
    let baseStyle = doc.head.querySelector("style");
    if (!baseStyle) {
      baseStyle = doc.createElement("style");
      doc.head.append(baseStyle);
    }
    baseStyle.textContent = parsedStyle?.textContent ?? "";
    doc.body.innerHTML = parsed.body.innerHTML;
    doc.body.classList.toggle("auramail-show-quotes", showQuotes);
    applyAnimationPreference(doc);
    applyTextSpacing(doc);

    requestAnimationFrame(() => {
      resize(true);
      onReady?.();
    });
  }

  function fadeInFrame(): void {
    const current = wrapper;
    if (!current) return;
    fadeAnimation?.cancel();
    fadeAnimation = null;
    if (appearance.reduceMotion || typeof current.animate !== "function") return;
    fadeAnimation = current.animate(
      [{ opacity: 0 }, { opacity: 1 }],
      { duration: 70, easing: "ease-out" }
    );
  }

  async function loaded() {
    const loadedFrame = frame;
    const doc = loadedFrame?.contentDocument;
    if (!loadedFrame || !doc) return;
    documentRef = doc;
    applyAnimationPreference(doc);
    applyTextSpacing(doc);
    doc.addEventListener("click", onClick, true);
    doc.addEventListener("submit", (event) => event.preventDefault());
    doc.addEventListener("load", () => resize(true), true);
    observer?.disconnect();
    observer = new ResizeObserver(() => resize());
    if (doc.body) observer.observe(doc.body);
    applyDocument(html);
    lastTransitionKey = String(transitionKey);
    fadeInFrame();

    await new Promise((resolve) => requestAnimationFrame(resolve));
    if (!mounted || frame !== loadedFrame) return;
    resize(true);
    onReady?.();
  }

  $effect(() => {
    const next = html;
    if (documentRef) applyDocument(next);
  });

  $effect(() => {
    if (!visible || !documentRef) return;
    requestAnimationFrame(() => resize(true));
  });

  $effect(() => {
    const key = String(transitionKey);
    if (lastTransitionKey === null) {
      lastTransitionKey = key;
      return;
    }
    if (key === lastTransitionKey) return;
    lastTransitionKey = key;
    fadeInFrame();
  });

  $effect(() => {
    const quotes = showQuotes;
    const doc = documentRef;
    if (!doc?.body) return;
    doc.body.classList.toggle("auramail-show-quotes", quotes);
    requestAnimationFrame(() => resize(true));
  });

  $effect(() => {
    appearance.reduceMotion;
    const doc = documentRef;
    if (doc) applyAnimationPreference(doc);
    if (appearance.reduceMotion) {
      fadeAnimation?.cancel();
      fadeAnimation = null;
    }
  });

  onMount(() => {
    mounted = true;
    return () => {
      mounted = false;
      observer?.disconnect();
      fadeAnimation?.cancel();
      fadeAnimation = null;
    };
  });
</script>

<div class="frame" bind:this={wrapper}>
  <iframe
    bind:this={frame}
    sandbox="allow-same-origin allow-scripts"
    referrerpolicy="no-referrer"
    srcdoc={SKELETON}
    title="Message body"
    onload={loaded}
  ></iframe>
</div>

<style>
  .frame {
    display: block;
  }
  iframe {
    display: block;
    width: 100%;
    min-height: 80px;
    border: 0;
    background: transparent;
  }
</style>
