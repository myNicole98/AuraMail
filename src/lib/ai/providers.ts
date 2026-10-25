/// <reference types="vite/client" />

export type AiProviderId =
  | "openai"
  | "anthropic"
  | "google"
  | "deepseek"
  | "openrouter"
  | "groq"
  | "moonshot"
  | "perplexity"
  | "zai"
  | "ollama"
  | "lmstudio"
  | "custom";

export type AiProvider = {
  id: AiProviderId;
  label: string;
  baseUrl: string;
  keyPlaceholder: string;
  requiresKey: boolean;
  fixed: boolean;
  hosts: string[];
};

export const providers: AiProvider[] = [
  {
    id: "openai",
    label: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    keyPlaceholder: "sk-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.openai.com"]
  },
  {
    id: "anthropic",
    label: "Anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    keyPlaceholder: "sk-ant-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.anthropic.com"]
  },
  {
    id: "google",
    label: "Google",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    keyPlaceholder: "AIzaSyxxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["generativelanguage.googleapis.com"]
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    keyPlaceholder: "dk-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.deepseek.com"]
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    keyPlaceholder: "sk-or-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["openrouter.ai"]
  },
  {
    id: "groq",
    label: "Groq",
    baseUrl: "https://api.groq.com/openai/v1",
    keyPlaceholder: "gsk_xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.groq.com"]
  },
  {
    id: "moonshot",
    label: "Moonshot",
    baseUrl: "https://api.moonshot.cn/v1",
    keyPlaceholder: "sk-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.moonshot.cn", "api.moonshot.ai"]
  },
  {
    id: "perplexity",
    label: "Perplexity",
    baseUrl: "https://api.perplexity.ai",
    keyPlaceholder: "pplx-xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.perplexity.ai"]
  },
  {
    id: "zai",
    label: "Z.ai",
    baseUrl: "https://api.z.ai/api/paas/v4",
    keyPlaceholder: "xxxxxxxxxxxx",
    requiresKey: true,
    fixed: true,
    hosts: ["api.z.ai"]
  },
  {
    id: "ollama",
    label: "Ollama",
    baseUrl: "http://localhost:11434/v1",
    keyPlaceholder: "",
    requiresKey: false,
    fixed: false,
    hosts: ["localhost:11434", "127.0.0.1:11434"]
  },
  {
    id: "lmstudio",
    label: "LM Studio",
    baseUrl: "http://localhost:1234/v1",
    keyPlaceholder: "",
    requiresKey: false,
    fixed: false,
    hosts: ["localhost:1234", "127.0.0.1:1234"]
  },
  {
    id: "custom",
    label: "Custom",
    baseUrl: "",
    keyPlaceholder: "API key (optional)",
    requiresKey: false,
    fixed: false,
    hosts: []
  }
];

const lightProviderIcons = import.meta.glob("../../../resources/icons/light/*.svg", {
  eager: true,
  query: "?url",
  import: "default"
}) as Record<string, string>;

const darkProviderIcons = import.meta.glob("../../../resources/icons/dark/*.svg", {
  eager: true,
  query: "?url",
  import: "default"
}) as Record<string, string>;

const modelIcons = import.meta.glob("../../../resources/icons/models/*.svg", {
  eager: true,
  query: "?url",
  import: "default"
}) as Record<string, string>;

const modelPatterns: [RegExp, string][] = [
  [/claude/i, "claude"],
  [/deepseek/i, "deepseek"],
  [/gemma/i, "gemma"],
  [/gemini/i, "gemini"],
  [/kimi|moonshot/i, "kimi"],
  [/llama/i, "llama"],
  [/minimax|abab/i, "minimax"],
  [/mistral|mixtral|codestral|magistral/i, "mistral"],
  [/nemotron/i, "nemotron"],
  [/qwen/i, "qwen"],
  [/groq/i, "groq"],
  [/gpt|openai|^o[1-4]/i, "openai"]
];

function iconUrl(modules: Record<string, string>, name: string): string | null {
  for (const [path, url] of Object.entries(modules)) {
    if (path.endsWith(`/${name}.svg`)) return url;
  }
  return null;
}

export function providerById(id: AiProviderId): AiProvider {
  return providers.find((provider) => provider.id === id) ?? providers[providers.length - 1];
}

export function providerForUrl(baseUrl: string): AiProviderId {
  const trimmed = baseUrl.trim();
  if (!trimmed) return "openai";
  try {
    const host = new URL(trimmed).host.toLowerCase();
    const match = providers.find((provider) =>
      provider.hosts.some((candidate) => host === candidate || host.endsWith(`.${candidate}`))
    );
    return match?.id ?? "custom";
  } catch {
    return "custom";
  }
}

export function providerIconUrls(id: AiProviderId): { light: string | null; dark: string | null } {
  return {
    light: iconUrl(lightProviderIcons, id),
    dark: iconUrl(darkProviderIcons, id)
  };
}

export function modelIconUrl(model: string): string | null {
  const name = model.trim();
  if (!name) return null;
  const match = modelPatterns.find(([pattern]) => pattern.test(name));
  return match ? iconUrl(modelIcons, match[1]) : null;
}
