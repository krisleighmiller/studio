/** App-defined wire protocols. Users name instances; the kind selects the adapter. */
export type ProtocolKind =
  | "atlas-cloud"
  | "openai-chat"
  | "anthropic-messages"
  | "gemini"
  | "ollama"
  | "elevenlabs"
  | "kling"
  | "replicate"
  | "fal"
  | "stability"
  | "runway"
  | "luma"
  | "sync-labs";

export type ApiCapabilities = {
  chat: boolean;
  image: boolean;
  video: boolean;
  audio: boolean;
  voiceStudio: boolean;
};

export type ProtocolKindMeta = {
  id: ProtocolKind;
  label: string;
  help: string;
  defaultBaseUrl: string;
  /** Kling (and similar) need a second secret beyond the access key. */
  needsSecretKey: boolean;
  /** Empty key is OK (local Ollama). */
  apiKeyOptional: boolean;
  canFetchCatalog: boolean;
  capabilities: ApiCapabilities;
};

export const PROTOCOL_KINDS: ProtocolKindMeta[] = [
  {
    id: "atlas-cloud",
    label: "Atlas Cloud",
    help: "Chat LLM plus image / video / audio generation (current generative adapter).",
    defaultBaseUrl: "https://api.atlascloud.ai",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: true,
    capabilities: {
      chat: true,
      image: true,
      video: true,
      audio: true,
      voiceStudio: false,
    },
  },
  {
    id: "openai-chat",
    label: "OpenAI-compatible",
    help: "Chat via /v1/chat/completions plus Images API (/v1/images/generations|edits). OpenAI, OpenRouter chat, and compatible proxies.",
    defaultBaseUrl: "https://api.openai.com/v1",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: true,
    capabilities: {
      chat: true,
      image: true,
      video: false,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "anthropic-messages",
    label: "Anthropic Messages",
    help: "Native Anthropic /v1/messages chat API (not OpenAI-compatible).",
    defaultBaseUrl: "https://api.anthropic.com",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: true,
      image: false,
      video: false,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "gemini",
    label: "Google Gemini",
    help: "Gemini chat (generateContent) plus native image models / Imagen.",
    defaultBaseUrl: "https://generativelanguage.googleapis.com/v1beta",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: true,
    capabilities: {
      chat: true,
      image: true,
      video: false,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "ollama",
    label: "Ollama (local)",
    help: "Local models via Ollama. Use http://127.0.0.1:11434 (not /v1). Chat uses /v1/chat/completions; catalog from /api/tags. Fetch after pulling a model.",
    defaultBaseUrl: "http://127.0.0.1:11434",
    needsSecretKey: false,
    apiKeyOptional: true,
    canFetchCatalog: true,
    capabilities: {
      chat: true,
      image: false,
      video: false,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "elevenlabs",
    label: "ElevenLabs",
    help: "Voice cloning and TTS for Character Voice (library + design).",
    defaultBaseUrl: "https://api.elevenlabs.io",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: false,
      video: false,
      audio: false,
      voiceStudio: true,
    },
  },
  {
    id: "kling",
    label: "Kling AI",
    help: "Kling Open Platform video + image generation (Access Key + Secret Key JWT auth).",
    defaultBaseUrl: "https://api-singapore.klingai.com",
    needsSecretKey: true,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: true,
      video: true,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "replicate",
    label: "Replicate",
    help: "Replicate predictions API. Model ids like owner/name (Flux, video models, etc.).",
    defaultBaseUrl: "https://api.replicate.com/v1",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: true,
      video: true,
      audio: true,
      voiceStudio: false,
    },
  },
  {
    id: "fal",
    label: "fal.ai",
    help: "fal.ai queue API for image / video / audio models (fal-ai/… ids).",
    defaultBaseUrl: "https://queue.fal.run",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: true,
      video: true,
      audio: true,
      voiceStudio: false,
    },
  },
  {
    id: "stability",
    label: "Stability AI",
    help: "Stable Image API (core / ultra / SD3) via multipart generate endpoints.",
    defaultBaseUrl: "https://api.stability.ai",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: true,
      video: false,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "runway",
    label: "Runway",
    help: "Runway developer API for text/image → video (gen4, veo3, etc.).",
    defaultBaseUrl: "https://api.dev.runwayml.com",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: false,
      video: true,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "luma",
    label: "Luma Dream Machine",
    help: "Luma Dream Machine video generations (ray-2 / ray-3 family).",
    defaultBaseUrl: "https://api.lumalabs.ai/dream-machine/v1",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: false,
    capabilities: {
      chat: false,
      image: false,
      video: true,
      audio: false,
      voiceStudio: false,
    },
  },
  {
    id: "sync-labs",
    label: "Sync Labs",
    help: "sync.so lip sync / talking-head video (video or image + audio). Auth via x-api-key.",
    defaultBaseUrl: "https://api.sync.so",
    needsSecretKey: false,
    apiKeyOptional: false,
    canFetchCatalog: true,
    capabilities: {
      chat: false,
      image: false,
      video: true,
      audio: false,
      voiceStudio: false,
    },
  },
];

export function protocolMeta(kind: ProtocolKind): ProtocolKindMeta {
  return PROTOCOL_KINDS.find((item) => item.id === kind) ?? PROTOCOL_KINDS[0]!;
}

export type CapabilitySlotId =
  | "video.start_frame"
  | "video.character_reference"
  | "video.text_only"
  | "video.location_prop_other_reference"
  | "video.edit"
  | "video.lip_sync_text_narration"
  | "video.lip_sync_audio_included"
  | "image.edit"
  | "image.image_to_image"
  | "image.character_reference"
  | "image.text_to_image"
  | "audio.tts_clone"
  | "audio.tts_profile";

export type ModelRef = {
  apiId: string;
  modelId: string;
};

export type CatalogModel = {
  id: string;
  displayName: string;
  type: "Text" | "Image" | "Video" | "Audio" | string;
  organization?: string;
  profile?: string;
  /** Atlas (and similar) catalog tags — e.g. "spicy" for NSFW-capable models. */
  tags?: string[];
};

export type ApiEntry = {
  id: string;
  name: string;
  kind: ProtocolKind;
  baseUrl: string;
  apiKey: string;
  /** Kling Secret Key (and future dual-credential protocols). */
  secretKey: string;
  enabled: boolean;
  catalog: CatalogModel[];
  catalogFetchedAt: string | null;
  pinnedLlmModelIds: string[];
};

export type AppSettings = {
  apis: ApiEntry[];
  capabilityDefaults: Partial<Record<CapabilitySlotId, ModelRef | null>>;
  /** When on, specialists prefer adultCapabilityDefaults, then fall back to capabilityDefaults. */
  adultContentEnabled: boolean;
  /** Parallel generation defaults for adult / NSFW workflows (all models available). */
  adultCapabilityDefaults: Partial<Record<CapabilitySlotId, ModelRef | null>>;
  selectedChatModel: ModelRef | null;
};

/** Atlas marks NSFW-capable media models with a spicy tag / id segment. */
export function isSpicyModel(model: Pick<CatalogModel, "id" | "displayName" | "tags" | "profile">): boolean {
  const tags = (model.tags ?? []).map((t) => t.trim().toLowerCase());
  if (tags.some((t) => t === "spicy" || t.includes("spicy") || t === "nsfw")) {
    return true;
  }
  const hay = `${model.id} ${model.displayName ?? ""} ${model.profile ?? ""}`.toLowerCase();
  return (
    hay.includes("/spicy") ||
    hay.includes("-spicy") ||
    hay.includes("_spicy") ||
    hay.includes("spicy/") ||
    hay.includes(" spicy") ||
    hay.endsWith("spicy")
  );
}

export const LLM_DROPDOWN_LIMIT = 10;

export const CAPABILITY_SLOTS: {
  id: CapabilitySlotId;
  group: "video" | "image" | "audio";
  label: string;
  mediaType: "Video" | "Image" | "Audio";
  enabled: boolean;
  note?: string;
}[] = [
  { id: "video.start_frame", group: "video", label: "Start frame", mediaType: "Video", enabled: true },
  {
    id: "video.character_reference",
    group: "video",
    label: "Character reference",
    mediaType: "Video",
    enabled: true,
  },
  { id: "video.text_only", group: "video", label: "Text only", mediaType: "Video", enabled: true },
  {
    id: "video.location_prop_other_reference",
    group: "video",
    label: "Location / prop / other reference",
    mediaType: "Video",
    enabled: true,
  },
  {
    id: "video.edit",
    group: "video",
    label: "Focused edit",
    mediaType: "Video",
    enabled: true,
    note: "Upload a clip and change only what the brief asks for (video-to-video / edit models)",
  },
  {
    id: "video.lip_sync_text_narration",
    group: "video",
    label: "Text-sync (talking shot)",
    mediaType: "Video",
    enabled: true,
    note: "Van/Wan image-to-video + driving audio (Text-Sync specialist)",
  },
  {
    id: "video.lip_sync_audio_included",
    group: "video",
    label: "Lip sync (Sync Labs)",
    mediaType: "Video",
    enabled: true,
    note: "Post lip-sync of an existing video + audio via Sync Labs",
  },
  {
    id: "image.edit",
    group: "image",
    label: "Focused edit",
    mediaType: "Image",
    enabled: true,
    note: "Upload a still and change only what the brief asks for (e.g. nano-banana-2/edit)",
  },
  {
    id: "image.image_to_image",
    group: "image",
    label: "Image to image",
    mediaType: "Image",
    enabled: true,
    note: "Broader still conditioning / revisions when Focused edit is unset",
  },
  {
    id: "image.character_reference",
    group: "image",
    label: "Character reference",
    mediaType: "Image",
    enabled: true,
  },
  {
    id: "image.text_to_image",
    group: "image",
    label: "Text to image",
    mediaType: "Image",
    enabled: true,
  },
  { id: "audio.tts_clone", group: "audio", label: "TTS (voice clone)", mediaType: "Audio", enabled: true },
  {
    id: "audio.tts_profile",
    group: "audio",
    label: "TTS (voice profile)",
    mediaType: "Audio",
    enabled: true,
  },
];

export function newApiId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `api_${crypto.randomUUID().replace(/-/g, "").slice(0, 16)}`;
  }
  return `api_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}

export function emptyApiEntry(
  kind: ProtocolKind = "openai-chat",
  name = "New API",
): ApiEntry {
  const meta = protocolMeta(kind);
  return {
    id: newApiId(),
    name,
    kind,
    baseUrl: meta.defaultBaseUrl,
    apiKey: "",
    secretKey: "",
    enabled: true,
    catalog: seedCatalogForKind(kind),
    catalogFetchedAt: null,
    pinnedLlmModelIds: [],
  };
}

export function seedCatalogForKind(kind: ProtocolKind): CatalogModel[] {
  switch (kind) {
    case "anthropic-messages":
      return seedAnthropicCatalog();
    case "kling":
      return seedKlingCatalog();
    case "openai-chat":
      return seedOpenAiImageCatalog();
    case "gemini":
      return seedGeminiImageCatalog();
    case "replicate":
      return seedReplicateCatalog();
    case "fal":
      return seedFalCatalog();
    case "stability":
      return seedStabilityCatalog();
    case "runway":
      return seedRunwayCatalog();
    case "luma":
      return seedLumaCatalog();
    case "sync-labs":
      return seedSyncLabsCatalog();
    default:
      return [];
  }
}

export function seedAnthropicCatalog(): CatalogModel[] {
  return [
    { id: "claude-opus-4-5", displayName: "Claude Opus 4.5", type: "Text", organization: "anthropic" },
    { id: "claude-sonnet-4-5", displayName: "Claude Sonnet 4.5", type: "Text", organization: "anthropic" },
    { id: "claude-haiku-4-5", displayName: "Claude Haiku 4.5", type: "Text", organization: "anthropic" },
    { id: "claude-opus-4-1", displayName: "Claude Opus 4.1", type: "Text", organization: "anthropic" },
    { id: "claude-sonnet-4-0", displayName: "Claude Sonnet 4", type: "Text", organization: "anthropic" },
    { id: "claude-3-5-haiku-latest", displayName: "Claude 3.5 Haiku", type: "Text", organization: "anthropic" },
  ];
}

/** Known Kling Open Platform video / image model ids for capability pickers. */
export function seedKlingCatalog(): CatalogModel[] {
  return [
    { id: "kling-v2-6", displayName: "Kling 2.6", type: "Video", organization: "kling" },
    { id: "kling-v2-5-turbo", displayName: "Kling 2.5 Turbo", type: "Video", organization: "kling" },
    { id: "kling-v2-1", displayName: "Kling 2.1", type: "Video", organization: "kling" },
    { id: "kling-v2-1-master", displayName: "Kling 2.1 Master", type: "Video", organization: "kling" },
    { id: "kling-v2-master", displayName: "Kling 2 Master", type: "Video", organization: "kling" },
    { id: "kling-v1-6", displayName: "Kling 1.6", type: "Video", organization: "kling" },
    { id: "kling-v1", displayName: "Kling 1", type: "Video", organization: "kling" },
    { id: "kling-image-o1", displayName: "Kling Image O1", type: "Image", organization: "kling" },
    { id: "kling-v2-1", displayName: "Kling Image 2.1", type: "Image", organization: "kling" },
    { id: "kling-v2", displayName: "Kling Image 2", type: "Image", organization: "kling" },
    { id: "kling-v1-5", displayName: "Kling Image 1.5", type: "Image", organization: "kling" },
  ];
}

/** Seed GPT Image models so OpenAI APIs can fill image capability slots before a chat catalog fetch. */
export function seedOpenAiImageCatalog(): CatalogModel[] {
  return [
    { id: "gpt-image-2", displayName: "GPT Image 2", type: "Image", organization: "openai" },
    { id: "gpt-image-1.5", displayName: "GPT Image 1.5", type: "Image", organization: "openai" },
    { id: "gpt-image-1", displayName: "GPT Image 1", type: "Image", organization: "openai" },
    { id: "gpt-image-1-mini", displayName: "GPT Image 1 Mini", type: "Image", organization: "openai" },
  ];
}

export function seedGeminiImageCatalog(): CatalogModel[] {
  return [
    {
      id: "gemini-2.5-flash-image",
      displayName: "Gemini 2.5 Flash Image",
      type: "Image",
      organization: "google",
    },
    {
      id: "gemini-3.1-flash-image-preview",
      displayName: "Gemini 3.1 Flash Image",
      type: "Image",
      organization: "google",
    },
    {
      id: "gemini-3-pro-image-preview",
      displayName: "Gemini 3 Pro Image",
      type: "Image",
      organization: "google",
    },
    {
      id: "imagen-4.0-generate-001",
      displayName: "Imagen 4",
      type: "Image",
      organization: "google",
    },
    {
      id: "imagen-4.0-fast-generate-001",
      displayName: "Imagen 4 Fast",
      type: "Image",
      organization: "google",
    },
  ];
}

export function seedReplicateCatalog(): CatalogModel[] {
  return [
    {
      id: "black-forest-labs/flux-schnell",
      displayName: "FLUX Schnell",
      type: "Image",
      organization: "replicate",
    },
    {
      id: "black-forest-labs/flux-dev",
      displayName: "FLUX Dev",
      type: "Image",
      organization: "replicate",
    },
    {
      id: "minimax/video-01",
      displayName: "MiniMax Video-01",
      type: "Video",
      organization: "replicate",
    },
  ];
}

export function seedFalCatalog(): CatalogModel[] {
  return [
    { id: "fal-ai/flux/schnell", displayName: "FLUX Schnell", type: "Image", organization: "fal" },
    { id: "fal-ai/flux/dev", displayName: "FLUX Dev", type: "Image", organization: "fal" },
    {
      id: "fal-ai/kling-video/v2.1/master",
      displayName: "Kling 2.1 Master (fal)",
      type: "Video",
      organization: "fal",
    },
    {
      id: "fal-ai/minimax/video-01-live",
      displayName: "MiniMax Video 01 Live",
      type: "Video",
      organization: "fal",
    },
  ];
}

export function seedStabilityCatalog(): CatalogModel[] {
  return [
    {
      id: "stable-image-core",
      displayName: "Stable Image Core",
      type: "Image",
      organization: "stability",
    },
    {
      id: "stable-image-ultra",
      displayName: "Stable Image Ultra",
      type: "Image",
      organization: "stability",
    },
    { id: "sd3", displayName: "Stable Diffusion 3", type: "Image", organization: "stability" },
  ];
}

export function seedRunwayCatalog(): CatalogModel[] {
  return [
    { id: "gen4.5", displayName: "Gen-4.5", type: "Video", organization: "runway" },
    { id: "gen4_turbo", displayName: "Gen-4 Turbo", type: "Video", organization: "runway" },
    { id: "veo3.1", displayName: "Veo 3.1", type: "Video", organization: "runway" },
    { id: "veo3.1_fast", displayName: "Veo 3.1 Fast", type: "Video", organization: "runway" },
  ];
}

export function seedLumaCatalog(): CatalogModel[] {
  return [
    { id: "ray-2", displayName: "Ray 2", type: "Video", organization: "luma" },
    { id: "ray-3.2", displayName: "Ray 3.2", type: "Video", organization: "luma" },
  ];
}

/** Sync Labs lip-sync / talking-head models (https://api.sync.so). */
export function seedSyncLabsCatalog(): CatalogModel[] {
  return [
    {
      id: "sync-3",
      displayName: "Sync 3",
      type: "Video",
      organization: "sync",
    },
    {
      id: "lipsync-2-pro",
      displayName: "Lipsync 2 Pro",
      type: "Video",
      organization: "sync",
    },
    {
      id: "lipsync-2",
      displayName: "Lipsync 2",
      type: "Video",
      organization: "sync",
    },
    {
      id: "lipsync-1.9.0-beta",
      displayName: "Lipsync 1.9 Beta",
      type: "Video",
      organization: "sync",
    },
    {
      id: "react-1",
      displayName: "React 1",
      type: "Video",
      organization: "sync",
    },
  ];
}

export function defaultSettings(): AppSettings {
  return {
    apis: [emptyApiEntry("atlas-cloud", "Atlas Cloud")],
    capabilityDefaults: {},
    adultContentEnabled: false,
    adultCapabilityDefaults: {},
    selectedChatModel: null,
  };
}

function normalizeCapabilityMap(
  raw: unknown,
  apis: ApiEntry[],
): AppSettings["capabilityDefaults"] {
  const out: AppSettings["capabilityDefaults"] = {};
  const rawCaps = (raw && typeof raw === "object" ? raw : {}) as Record<
    string,
    { providerId?: string; apiId?: string; modelId?: string } | null
  >;
  for (const [slot, ref] of Object.entries(rawCaps)) {
    if (!ref?.modelId) {
      out[slot as CapabilitySlotId] = null;
      continue;
    }
    out[slot as CapabilitySlotId] = {
      apiId: ref.apiId || ref.providerId || apis[0]!.id,
      modelId: ref.modelId,
    };
  }
  return out;
}

function normalizeCatalogModel(raw: unknown): CatalogModel | null {
  if (!raw || typeof raw !== "object") return null;
  const src = raw as Record<string, unknown>;
  const id = typeof src.id === "string" ? src.id.trim() : "";
  if (!id) return null;
  const tags = Array.isArray(src.tags)
    ? src.tags.filter((t): t is string => typeof t === "string" && t.trim().length > 0)
    : undefined;
  return {
    id,
    displayName:
      typeof src.displayName === "string" && src.displayName.trim()
        ? src.displayName.trim()
        : id,
    type: typeof src.type === "string" && src.type.trim() ? src.type : "Unknown",
    organization:
      typeof src.organization === "string" ? src.organization : undefined,
    profile: typeof src.profile === "string" ? src.profile : undefined,
    tags,
  };
}

/** True for loopback / .local OpenAI-compatible or Ollama bases (no cloud key needed). */
export function isLocalLlmBaseUrl(baseUrl: string): boolean {
  const raw = baseUrl.trim().toLowerCase();
  if (!raw) return false;
  try {
    const withScheme = raw.includes("://") ? raw : `http://${raw}`;
    const url = new URL(withScheme);
    const host = url.hostname.replace(/^\[|\]$/g, "");
    return (
      host === "localhost" ||
      host === "127.0.0.1" ||
      host === "0.0.0.0" ||
      host === "::1" ||
      host.endsWith(".local")
    );
  } catch {
    return (
      raw.includes("localhost") ||
      raw.includes("127.0.0.1") ||
      raw.includes("[::1]")
    );
  }
}

export function apiAllowsEmptyKey(api: Pick<ApiEntry, "kind" | "baseUrl">): boolean {
  const meta = protocolMeta(api.kind);
  if (meta.apiKeyOptional) return true;
  return api.kind === "openai-chat" && isLocalLlmBaseUrl(api.baseUrl);
}

export function textModels(api: ApiEntry): CatalogModel[] {
  return api.catalog.filter((model) => {
    const type = (model.type || "").trim().toLowerCase();
    return type === "text" || type === "llm" || type === "chat" || type === "language";
  });
}

/** Total pinned chat LLMs across all APIs (shared budget for the right-side dropdown). */
export function globalPinnedLlmCount(settings: AppSettings): number {
  return chatCapableApis(settings).reduce((n, api) => n + api.pinnedLlmModelIds.length, 0);
}

/** Models that should appear in the chat dropdown for one API entry (pinned only). */
export function dropdownLlmModels(api: ApiEntry): CatalogModel[] {
  const texts = textModels(api);
  const pinned = new Set(api.pinnedLlmModelIds);
  return texts.filter((model) => pinned.has(model.id));
}

/** Pin model ids onto an API until the global dropdown budget is full. */
export function pinModelsIntoBudget(
  settings: AppSettings,
  api: ApiEntry,
  modelIds: string[],
): boolean {
  const pinned = new Set(api.pinnedLlmModelIds);
  let changed = false;
  for (const id of modelIds) {
    if (pinned.has(id)) continue;
    if (globalPinnedLlmCount(settings) >= LLM_DROPDOWN_LIMIT) break;
    // Count uses api.pinnedLlmModelIds; update as we go so the budget stays accurate.
    pinned.add(id);
    api.pinnedLlmModelIds = [...pinned];
    changed = true;
  }
  return changed;
}

export function apiById(settings: AppSettings, apiId: string | null | undefined): ApiEntry | null {
  if (!apiId) return null;
  return settings.apis.find((api) => api.id === apiId) ?? null;
}

export function chatCapableApis(settings: AppSettings): ApiEntry[] {
  return settings.apis.filter(
    (api) => api.enabled && protocolMeta(api.kind).capabilities.chat,
  );
}

export function findApiByKind(settings: AppSettings, kind: ProtocolKind): ApiEntry | null {
  return (
    settings.apis.find((api) => api.enabled && api.kind === kind && (api.apiKey || protocolMeta(kind).apiKeyOptional)) ??
    settings.apis.find((api) => api.kind === kind) ??
    null
  );
}

/** Migrate legacy `{ providers: { atlas, elevenlabs } }` settings into `apis[]`. */
export function migrateLegacySettings(raw: unknown): AppSettings {
  const base = defaultSettings();
  if (!raw || typeof raw !== "object") return base;
  const obj = raw as Record<string, unknown>;

  // Already new shape
  if (Array.isArray(obj.apis)) {
    return normalizeNewSettings(obj);
  }

  const providers = (obj.providers ?? {}) as Record<string, Record<string, unknown>>;
  const apis: ApiEntry[] = [];
  const idMap: Record<string, string> = {};

  for (const legacyId of ["atlas", "elevenlabs"] as const) {
    const src = providers[legacyId];
    if (!src && legacyId === "elevenlabs") continue;
    const kind: ProtocolKind = legacyId === "atlas" ? "atlas-cloud" : "elevenlabs";
    const meta = protocolMeta(kind);
    const entry = emptyApiEntry(
      kind,
      legacyId === "atlas" ? "Atlas Cloud" : "ElevenLabs",
    );
    if (src) {
      entry.apiKey = typeof src.apiKey === "string" ? src.apiKey : "";
      entry.enabled = Boolean(src.enabled);
      entry.catalog = Array.isArray(src.catalog) ? (src.catalog as CatalogModel[]) : [];
      entry.catalogFetchedAt =
        typeof src.catalogFetchedAt === "string" ? src.catalogFetchedAt : null;
      entry.pinnedLlmModelIds = Array.isArray(src.pinnedLlmModelIds)
        ? (src.pinnedLlmModelIds as string[]).slice(0, LLM_DROPDOWN_LIMIT)
        : [];
      entry.baseUrl = meta.defaultBaseUrl;
    }
    idMap[legacyId] = entry.id;
    apis.push(entry);
  }

  if (apis.length === 0) {
    apis.push(emptyApiEntry("atlas-cloud", "Atlas Cloud"));
  }

  const capabilityDefaults: AppSettings["capabilityDefaults"] = {};
  const rawCaps = (obj.capabilityDefaults ?? {}) as Record<
    string,
    { providerId?: string; apiId?: string; modelId?: string } | null
  >;
  for (const [slot, ref] of Object.entries(rawCaps)) {
    if (!ref?.modelId) {
      capabilityDefaults[slot as CapabilitySlotId] = null;
      continue;
    }
    const apiId =
      ref.apiId ||
      (ref.providerId ? idMap[ref.providerId] : undefined) ||
      apis[0]!.id;
    capabilityDefaults[slot as CapabilitySlotId] = { apiId, modelId: ref.modelId };
  }

  let selectedChatModel: ModelRef | null = null;
  const rawSelected = obj.selectedChatModel as
    | { providerId?: string; apiId?: string; modelId?: string }
    | null
    | undefined;
  if (rawSelected?.modelId) {
    selectedChatModel = {
      apiId:
        rawSelected.apiId ||
        (rawSelected.providerId ? idMap[rawSelected.providerId] : undefined) ||
        apis[0]!.id,
      modelId: rawSelected.modelId,
    };
  }

  return {
    apis,
    capabilityDefaults,
    adultContentEnabled: false,
    adultCapabilityDefaults: {},
    selectedChatModel,
  };
}

function normalizeNewSettings(obj: Record<string, unknown>): AppSettings {
  const rawApis = Array.isArray(obj.apis) ? obj.apis : [];
  const apis: ApiEntry[] = rawApis.map((item) => normalizeApiEntry(item));
  if (apis.length === 0) {
    apis.push(emptyApiEntry("atlas-cloud", "Atlas Cloud"));
  }

  const capabilityDefaults = normalizeCapabilityMap(obj.capabilityDefaults, apis);
  const adultCapabilityDefaults = normalizeCapabilityMap(
    obj.adultCapabilityDefaults,
    apis,
  );

  const rawSelected = obj.selectedChatModel as
    | { providerId?: string; apiId?: string; modelId?: string }
    | null
    | undefined;
  const selectedChatModel =
    rawSelected?.modelId
      ? {
          apiId: rawSelected.apiId || rawSelected.providerId || apis[0]!.id,
          modelId: rawSelected.modelId,
        }
      : null;

  return {
    apis,
    capabilityDefaults,
    adultContentEnabled: Boolean(obj.adultContentEnabled),
    adultCapabilityDefaults,
    selectedChatModel,
  };
}

function normalizeApiEntry(raw: unknown): ApiEntry {
  const src = (raw && typeof raw === "object" ? raw : {}) as Partial<ApiEntry>;
  const kind = (PROTOCOL_KINDS.some((k) => k.id === src.kind) ? src.kind : "openai-chat") as ProtocolKind;
  const meta = protocolMeta(kind);
  const entry: ApiEntry = {
    id: typeof src.id === "string" && src.id ? src.id : newApiId(),
    name: typeof src.name === "string" && src.name.trim() ? src.name.trim() : meta.label,
    kind,
    baseUrl:
      typeof src.baseUrl === "string" && src.baseUrl.trim()
        ? src.baseUrl.trim().replace(/\/+$/, "")
        : meta.defaultBaseUrl,
    apiKey: typeof src.apiKey === "string" ? src.apiKey : "",
    secretKey: typeof src.secretKey === "string" ? src.secretKey : "",
    enabled: src.enabled !== false,
    catalog: Array.isArray(src.catalog)
      ? src.catalog
          .map((item) => normalizeCatalogModel(item))
          .filter((item): item is CatalogModel => Boolean(item))
      : [],
    catalogFetchedAt:
      typeof src.catalogFetchedAt === "string" ? src.catalogFetchedAt : null,
    pinnedLlmModelIds: Array.isArray(src.pinnedLlmModelIds)
      ? src.pinnedLlmModelIds.slice(0, LLM_DROPDOWN_LIMIT)
      : [],
  };
  if (entry.catalog.length === 0) {
    entry.catalog = seedCatalogForKind(kind);
  }
  if (kind === "openai-chat" || kind === "gemini") {
    const haveImage = entry.catalog.some((model) => model.type === "Image");
    if (!haveImage) {
      entry.catalog = [
        ...entry.catalog,
        ...(kind === "gemini" ? seedGeminiImageCatalog() : seedOpenAiImageCatalog()),
      ];
    }
  }
  return entry;
}
