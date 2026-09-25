import { invoke } from "./host";
import {
  CAPABILITY_SLOTS,
  LLM_DROPDOWN_LIMIT,
  PROTOCOL_KINDS,
  apiAllowsEmptyKey,
  apiById,
  chatCapableApis,
  defaultSettings,
  dropdownLlmModels,
  emptyApiEntry,
  globalPinnedLlmCount,
  isLocalLlmBaseUrl,
  isSpicyModel,
  migrateLegacySettings,
  pinModelsIntoBudget,
  protocolMeta,
  seedGeminiImageCatalog,
  seedOpenAiImageCatalog,
  textModels,
  type ApiEntry,
  type AppSettings,
  type CapabilitySlotId,
  type CatalogModel,
  type ModelRef,
  type ProtocolKind,
} from "./settingsTypes";

export type SettingsHost = {
  root: HTMLElement;
  onStatus: (left: string, right?: string) => void;
  onSettingsChanged: (settings: AppSettings) => void;
  onDraftChanged?: () => void;
};

let draft: AppSettings = defaultSettings();
let savedSnapshot = "";
let host: SettingsHost | null = null;
let busy = false;

function cloneSettings(value: AppSettings): AppSettings {
  return JSON.parse(JSON.stringify(value)) as AppSettings;
}

function snapshotOf(value: AppSettings): string {
  return JSON.stringify(value);
}

export function getSettings(): AppSettings {
  return cloneSettings(draft);
}

export function settingsAreDirty(): boolean {
  return snapshotOf(draft) !== savedSnapshot;
}

export async function loadAppSettings(): Promise<AppSettings> {
  const loaded = await invoke<unknown>("load_settings");
  draft = migrateLegacySettings(loaded);
  savedSnapshot = snapshotOf(draft);
  await refreshLocalLlmCatalogs({ autoAddOllama: true, persist: true });
  return getSettings();
}

export async function persistSettings(): Promise<void> {
  if (draft.apis.length === 0) {
    throw new Error("Add at least one API in Settings.");
  }
  await invoke("save_settings", { settings: draft });
  savedSnapshot = snapshotOf(draft);
  host?.onSettingsChanged(getSettings());
}

function modelsForSlot(
  slotId: CapabilitySlotId,
  options?: { excludeSpicy?: boolean },
): Array<CatalogModel & { apiId: string }> {
  const slot = CAPABILITY_SLOTS.find((item) => item.id === slotId);
  if (!slot) return [];
  const models: Array<CatalogModel & { apiId: string }> = [];
  for (const api of draft.apis) {
    const caps = protocolMeta(api.kind).capabilities;
    // A saved reference WAV needs a model that accepts reference audio.
    // Direct ElevenLabs TTS requires a registered voice ID instead, so it
    // belongs in the profile slot and is not a valid voice-clone backend.
    if (slot.id === "audio.tts_clone" && api.kind === "elevenlabs") continue;
    if (slot.mediaType === "Video" && !caps.video) continue;
    if (slot.mediaType === "Image" && !caps.image) continue;
    if (slot.mediaType === "Audio" && !caps.audio) continue;
    if (!api.apiKey && !apiAllowsEmptyKey(api) && api.catalog.length === 0) {
      continue;
    }
    for (const model of api.catalog) {
      if (model.type !== slot.mediaType) continue;
      if (options?.excludeSpicy && isSpicyModel(model)) continue;
      models.push({ ...model, apiId: api.id });
    }
  }
  return models;
}

function modelOptionValue(apiId: string, modelId: string): string {
  return `${apiId}::${modelId}`;
}

function parseModelOption(value: string): ModelRef | null {
  if (!value) return null;
  const [apiId, ...rest] = value.split("::");
  const modelId = rest.join("::");
  if (!apiId || !modelId) return null;
  return { apiId, modelId };
}

export function bindSettingsHost(next: SettingsHost): void {
  host = next;
}

function notifyDraftChanged(): void {
  host?.onDraftChanged?.();
}

function renderApiCard(api: ApiEntry, index: number): string {
  const meta = protocolMeta(api.kind);
  const canRemove = draft.apis.length > 1;
  const catalogAction = meta.canFetchCatalog
    ? `<button type="button" class="ghost-button" data-action="fetch-catalog" data-api-id="${escapeAttr(api.id)}" ${busy ? "disabled" : ""}>
          ${busy ? "Fetching…" : "Fetch model library"}
        </button>
        <span class="settings-meta">
          ${
            api.catalogFetchedAt
              ? `${api.catalog.length} models · fetched ${new Date(api.catalogFetchedAt).toLocaleString()}`
              : api.catalog.length > 0
                ? `${api.catalog.length} models (seeded / cached)`
                : "No catalog loaded yet"
          }
        </span>`
    : meta.capabilities.voiceStudio
      ? `<span class="settings-meta">Used by View → Character Voice (library + design).</span>`
      : `<span class="settings-meta">${api.catalog.length} model(s) available (built-in list for this protocol).</span>`;

  const keyLabel = api.kind === "kling" ? "Access Key" : "API key";

  return `
    <div class="provider-card" data-api-id="${escapeAttr(api.id)}">
      <div class="provider-card-head">
        <strong>API ${index + 1}</strong>
        <div class="provider-card-head-actions">
          <label class="inline-check">
            <input type="checkbox" data-field="enabled" ${api.enabled ? "checked" : ""} />
            Enabled
          </label>
          <button type="button" class="ghost-button danger-text" data-action="remove-api" data-api-id="${escapeAttr(api.id)}" ${canRemove ? "" : "disabled"} title="${canRemove ? "Remove this API" : "At least one API is required"}">
            Remove
          </button>
        </div>
      </div>
      <p class="settings-help">${escapeHtml(meta.help)}</p>
      <label class="settings-field">
        <span>Name</span>
        <input type="text" data-field="name" value="${escapeAttr(api.name)}" autocomplete="off" spellcheck="false" />
      </label>
      <label class="settings-field">
        <span>Protocol</span>
        <select data-field="kind">
          ${PROTOCOL_KINDS.map(
            (kind) => `
            <option value="${kind.id}" ${kind.id === api.kind ? "selected" : ""}>
              ${escapeHtml(kind.label)}
            </option>`,
          ).join("")}
        </select>
      </label>
      <label class="settings-field">
        <span>Base URL</span>
        <input type="text" data-field="baseUrl" value="${escapeAttr(api.baseUrl)}" autocomplete="off" spellcheck="false" />
      </label>
      <label class="settings-field">
        <span>${escapeHtml(keyLabel)}${meta.apiKeyOptional ? " (optional)" : ""}</span>
        <input type="password" data-field="apiKey" value="${escapeAttr(api.apiKey)}" autocomplete="off" spellcheck="false" />
      </label>
      ${
        meta.needsSecretKey
          ? `<label class="settings-field">
              <span>Secret Key</span>
              <input type="password" data-field="secretKey" value="${escapeAttr(api.secretKey)}" autocomplete="off" spellcheck="false" />
            </label>`
          : ""
      }
      <div class="provider-actions">
        ${catalogAction}
      </div>
    </div>
  `;
}

export function renderSettingsPanel(): void {
  if (!host) return;
  const root = host.root;
  const chatApis = chatCapableApis(draft);
  const pinningTargets = chatApis.filter((api) => textModels(api).length > 0);
  const pinnedTotal = globalPinnedLlmCount(draft);
  const anyCatalog = draft.apis.some((api) => api.catalog.length > 0);

  root.innerHTML = `
    <div class="settings-page">
      <header class="settings-header">
        <div>
          <h1>Settings</h1>
          <p class="settings-lede">Named APIs, generation defaults, and which LLMs appear in chat.</p>
        </div>
        <div class="settings-header-actions">
          <button type="button" class="ghost-button" data-action="reload">Reload</button>
          <button type="button" class="primary-button" data-action="save">Save</button>
        </div>
      </header>

      <section class="settings-section">
        <h2>APIs</h2>
        <p class="settings-help">
          Add as many named APIs as you need. Each entry picks a protocol (how the desk talks to it).
          Keys stay on this machine in the app config directory. At least one API is required.
        </p>
        ${draft.apis.map((api, index) => renderApiCard(api, index)).join("")}
        <div class="api-add-row">
          <button type="button" class="ghost-button api-add-button" data-action="add-api" title="Add another API">+</button>
          <span class="settings-meta">Add another API</span>
        </div>
      </section>

      <section class="settings-section">
        <h2>Generation defaults</h2>
        <p class="settings-help">
          Specialists use these model IDs when a handoff does not name a model (Settings-first — not marketplace discovery).
          Override in chat or a handoff still wins. Slots only list models from APIs whose protocol supports that media type.
          Atlas “spicy” models are hidden here — set those under Adult content.
        </p>
        ${renderCapabilityGroups(anyCatalog, {
          defaultsKey: "capabilityDefaults",
          excludeSpicy: true,
        })}
      </section>

      <section class="settings-section">
        <h2>Adult content</h2>
        <p class="settings-help">
          Optional parallel generation defaults for adult / NSFW workflows. Lists all models (including Atlas
          <strong>spicy</strong> ones hidden from main Generation defaults). Enable this section to configure
          slots, then in Director chat say <strong>adult</strong> or <strong>nsfw</strong> (e.g. “generate this
          scene using the adult models”) so that turn uses these defaults. Unset adult slots fall back to main
          Generation defaults.
        </p>
        <label class="settings-field settings-toggle">
          <span>Enable adult content defaults</span>
          <input type="checkbox" data-field="adultContentEnabled" ${
            draft.adultContentEnabled ? "checked" : ""
          } />
        </label>
        ${
          draft.adultContentEnabled
            ? renderCapabilityGroups(anyCatalog, {
                defaultsKey: "adultCapabilityDefaults",
                slotAttr: "adult-slot",
              })
            : `<p class="settings-meta">Turn on to choose adult model defaults. Main Generation defaults stay unchanged.</p>`
        }
      </section>

      <section class="settings-section">
        <h2>LLMs (chat dropdown)</h2>
        <p class="settings-help">
          Pin up to ${LLM_DROPDOWN_LIMIT} text models across all chat APIs (cloud and local).
          Only pinned models appear in the chat model dropdown on the right.
        </p>
        <p class="settings-meta">Pinned: ${pinnedTotal} / ${LLM_DROPDOWN_LIMIT}</p>
        ${
          pinningTargets.length > 0
            ? pinningTargets
                .map((api) => {
                  const text = textModels(api);
                  const pinned = new Set(api.pinnedLlmModelIds);
                  return `
                    <h3>${escapeHtml(api.name)}</h3>
                    <div class="llm-pin-list" data-api-id="${escapeAttr(api.id)}">
                      ${text
                        .map(
                          (model) => `
                        <label class="llm-pin-item">
                          <input
                            type="checkbox"
                            data-pin-model="${escapeAttr(model.id)}"
                            data-api-id="${escapeAttr(api.id)}"
                            ${pinned.has(model.id) ? "checked" : ""}
                          />
                          <span>
                            <strong>${escapeHtml(model.displayName)}</strong>
                            <small>${escapeHtml(model.id)}</small>
                          </span>
                        </label>`,
                        )
                        .join("")}
                    </div>
                    <p class="settings-meta">${api.pinnedLlmModelIds.length} pinned from this API</p>`;
                })
                .join("")
            : `<p class="settings-meta">No text models yet — add a chat API and fetch its model library (Ollama is picked up automatically when running).</p>`
        }
      </section>
    </div>
  `;

  wireSettingsDom(root);
}

function renderCapabilityGroups(
  anyCatalog: boolean,
  options: {
    defaultsKey: "capabilityDefaults" | "adultCapabilityDefaults";
    excludeSpicy?: boolean;
    slotAttr?: string;
  },
): string {
  const groups: Array<"video" | "image" | "audio"> = ["video", "image", "audio"];
  const slotAttr = options.slotAttr ?? "slot";
  const defaults = draft[options.defaultsKey];
  return groups
    .map((group) => {
      const slots = CAPABILITY_SLOTS.filter((slot) => slot.group === group);
      return `
        <div class="capability-group">
          <h3>${group}</h3>
          <div class="capability-grid">
            ${slots
              .map((slot) => {
                const current = defaults[slot.id];
                const modelOptions = modelsForSlot(slot.id, {
                  excludeSpicy: options.excludeSpicy,
                });
                const currentValue = current
                  ? modelOptionValue(current.apiId, current.modelId)
                  : "";
                const fallbackNote =
                  options.defaultsKey === "adultCapabilityDefaults" && !current
                    ? `<em class="slot-note">Falls back to main Generation defaults</em>`
                    : "";
                return `
                  <label class="settings-field">
                    <span>
                      ${escapeHtml(slot.label)}
                      ${slot.note ? `<em class="slot-note">${escapeHtml(slot.note)}</em>` : ""}
                      ${fallbackNote}
                    </span>
                    <select data-${slotAttr}="${slot.id}" ${
                      slot.enabled && anyCatalog ? "" : "disabled"
                    }>
                      <option value="">Not set${
                        options.defaultsKey === "adultCapabilityDefaults"
                          ? " (use main default)"
                          : ""
                      }</option>
                      ${modelOptions
                        .map((model) => {
                          const value = modelOptionValue(model.apiId, model.id);
                          const api = apiById(draft, model.apiId);
                          const apiName = api?.name ?? model.apiId;
                          return `
                        <option value="${escapeAttr(value)}" ${
                          value === currentValue ? "selected" : ""
                        }>
                          ${escapeHtml(apiName)} · ${escapeHtml(model.displayName)} (${escapeHtml(model.id)})
                        </option>`;
                        })
                        .join("")}
                    </select>
                  </label>`;
              })
              .join("")}
          </div>
        </div>`;
    })
    .join("");
}

function wireSettingsDom(root: HTMLElement): void {
  root.querySelector<HTMLButtonElement>('[data-action="save"]')?.addEventListener("click", () => {
    void (async () => {
      try {
        await persistSettings();
        host?.onStatus("Settings saved");
        renderSettingsPanel();
      } catch (error) {
        host?.onStatus(`Could not save settings: ${error}`);
      }
    })();
  });

  root.querySelector<HTMLButtonElement>('[data-action="reload"]')?.addEventListener("click", () => {
    void (async () => {
      await loadAppSettings();
      renderSettingsPanel();
      host?.onSettingsChanged(getSettings());
      host?.onStatus("Settings reloaded");
    })();
  });

  root.querySelector<HTMLButtonElement>('[data-action="add-api"]')?.addEventListener("click", () => {
    draft.apis.push(emptyApiEntry("openai-chat", `API ${draft.apis.length + 1}`));
    notifyDraftChanged();
    renderSettingsPanel();
  });

  for (const button of root.querySelectorAll<HTMLButtonElement>('[data-action="remove-api"]')) {
    button.addEventListener("click", () => {
      const apiId = button.dataset.apiId;
      if (!apiId || draft.apis.length <= 1) return;
      draft.apis = draft.apis.filter((api) => api.id !== apiId);
      // Drop refs that pointed at the removed API.
      for (const slot of CAPABILITY_SLOTS) {
        const ref = draft.capabilityDefaults[slot.id];
        if (ref?.apiId === apiId) draft.capabilityDefaults[slot.id] = null;
        const adultRef = draft.adultCapabilityDefaults[slot.id];
        if (adultRef?.apiId === apiId) draft.adultCapabilityDefaults[slot.id] = null;
      }
      if (draft.selectedChatModel?.apiId === apiId) {
        draft.selectedChatModel = null;
      }
      notifyDraftChanged();
      renderSettingsPanel();
    });
  }

  for (const button of root.querySelectorAll<HTMLButtonElement>('[data-action="fetch-catalog"]')) {
    button.addEventListener("click", () => {
      const apiId = button.dataset.apiId;
      if (!apiId) return;
      void fetchApiCatalog(apiId);
    });
  }

  for (const card of root.querySelectorAll<HTMLElement>(".provider-card[data-api-id]")) {
    const apiId = card.dataset.apiId;
    const api = draft.apis.find((item) => item.id === apiId);
    if (!api) continue;

    const name = card.querySelector<HTMLInputElement>('[data-field="name"]');
    name?.addEventListener("input", () => {
      api.name = name.value;
      notifyDraftChanged();
    });

    const kindSelect = card.querySelector<HTMLSelectElement>('[data-field="kind"]');
    kindSelect?.addEventListener("change", () => {
      const previousMeta = protocolMeta(api.kind);
      const nextKind = kindSelect.value as ProtocolKind;
      if (nextKind === api.kind) return;
      const meta = protocolMeta(nextKind);
      const baseWasDefault =
        !api.baseUrl.trim() || api.baseUrl.replace(/\/+$/, "") === previousMeta.defaultBaseUrl;
      api.kind = nextKind;
      if (baseWasDefault) {
        api.baseUrl = meta.defaultBaseUrl;
      }
      // Catalogs are protocol-specific; don't keep Atlas ids on Anthropic, etc.
      api.catalog = emptyApiEntry(nextKind).catalog;
      api.catalogFetchedAt = null;
      api.pinnedLlmModelIds = [];
      notifyDraftChanged();
      renderSettingsPanel();
      if (nextKind === "ollama" || (nextKind === "openai-chat" && isLocalLlmBaseUrl(api.baseUrl))) {
        void fetchApiCatalog(api.id);
      }
    });

    const baseUrl = card.querySelector<HTMLInputElement>('[data-field="baseUrl"]');
    baseUrl?.addEventListener("input", () => {
      api.baseUrl = baseUrl.value.trim();
      notifyDraftChanged();
    });

    const apiKey = card.querySelector<HTMLInputElement>('[data-field="apiKey"]');
    apiKey?.addEventListener("input", () => {
      api.apiKey = apiKey.value;
      notifyDraftChanged();
    });

    const secretKey = card.querySelector<HTMLInputElement>('[data-field="secretKey"]');
    secretKey?.addEventListener("input", () => {
      api.secretKey = secretKey.value;
      notifyDraftChanged();
    });

    const enabled = card.querySelector<HTMLInputElement>('[data-field="enabled"]');
    enabled?.addEventListener("change", () => {
      api.enabled = enabled.checked;
      notifyDraftChanged();
    });
  }

  root
    .querySelector<HTMLInputElement>('[data-field="adultContentEnabled"]')
    ?.addEventListener("change", (event) => {
      const input = event.currentTarget as HTMLInputElement;
      draft.adultContentEnabled = input.checked;
      notifyDraftChanged();
      renderSettingsPanel();
    });

  for (const select of root.querySelectorAll<HTMLSelectElement>("[data-slot]")) {
    select.addEventListener("change", () => {
      const slotId = select.dataset.slot as CapabilitySlotId;
      draft.capabilityDefaults[slotId] = parseModelOption(select.value);
      notifyDraftChanged();
    });
  }

  for (const select of root.querySelectorAll<HTMLSelectElement>("[data-adult-slot]")) {
    select.addEventListener("change", () => {
      const slotId = select.dataset.adultSlot as CapabilitySlotId;
      draft.adultCapabilityDefaults[slotId] = parseModelOption(select.value);
      notifyDraftChanged();
    });
  }

  for (const checkbox of root.querySelectorAll<HTMLInputElement>("[data-pin-model]")) {
    checkbox.addEventListener("change", () => {
      const modelId = checkbox.dataset.pinModel;
      const apiId = checkbox.dataset.apiId;
      const api = draft.apis.find((item) => item.id === apiId);
      if (!modelId || !api) return;
      const pinned = new Set(api.pinnedLlmModelIds);
      if (checkbox.checked) {
        if (pinned.has(modelId)) return;
        if (globalPinnedLlmCount(draft) >= LLM_DROPDOWN_LIMIT) {
          checkbox.checked = false;
          host?.onStatus(`Pin at most ${LLM_DROPDOWN_LIMIT} LLMs total for the chat dropdown`);
          return;
        }
        pinned.add(modelId);
      } else {
        pinned.delete(modelId);
      }
      api.pinnedLlmModelIds = [...pinned];
      notifyDraftChanged();
      renderSettingsPanel();
      host?.onDraftChanged?.();
    });
  }
}

async function applyFetchedCatalog(api: ApiEntry, catalog: CatalogModel[]): Promise<void> {
  if (api.kind === "openai-chat" || api.kind === "gemini") {
    const have = new Set(catalog.map((model) => `${model.type}::${model.id}`));
    const seeded =
      api.kind === "gemini" ? seedGeminiImageCatalog() : seedOpenAiImageCatalog();
    for (const model of seeded) {
      const key = `${model.type}::${model.id}`;
      if (!have.has(key)) catalog.push(model);
    }
  }
  api.catalog = catalog;
  api.catalogFetchedAt = new Date().toISOString();
  api.enabled = true;
  const texts = textModels(api);
  const textIds = new Set(texts.map((model) => model.id));
  api.pinnedLlmModelIds = api.pinnedLlmModelIds.filter((id) => textIds.has(id));
  // Local / small catalogs: auto-pin new models into the shared dropdown budget.
  if (api.kind === "ollama" || isLocalLlmBaseUrl(api.baseUrl) || texts.length <= LLM_DROPDOWN_LIMIT) {
    pinModelsIntoBudget(
      draft,
      api,
      texts.map((model) => model.id),
    );
  }
}

async function fetchApiCatalog(apiId: string, options?: { quiet?: boolean }): Promise<boolean> {
  if (busy) return false;
  const api = draft.apis.find((item) => item.id === apiId);
  if (!api) return false;
  const meta = protocolMeta(api.kind);
  if (!meta.canFetchCatalog) {
    if (!options?.quiet) host?.onStatus("This protocol uses a built-in model list");
    return false;
  }
  if (!api.apiKey.trim() && !apiAllowsEmptyKey(api)) {
    if (!options?.quiet) host?.onStatus(`Enter an API key for ${api.name} first`);
    return false;
  }
  busy = true;
  if (host) renderSettingsPanel();
  try {
    const catalog = await invoke<CatalogModel[]>("fetch_api_catalog", {
      kind: api.kind,
      apiKey: api.apiKey || (api.kind === "ollama" || isLocalLlmBaseUrl(api.baseUrl) ? "local" : ""),
      baseUrl: api.baseUrl,
    });
    await applyFetchedCatalog(api, catalog);
    if (!options?.quiet) {
      host?.onStatus(`Fetched ${catalog.length} models for ${api.name}`);
    }
    notifyDraftChanged();
    return true;
  } catch (error) {
    if (!options?.quiet) host?.onStatus(`Catalog fetch failed: ${error}`);
    return false;
  } finally {
    busy = false;
    if (host) renderSettingsPanel();
  }
}

/** Pull local Ollama / LM Studio catalogs so chat can list pulled models without a manual fetch. */
export async function refreshLocalLlmCatalogs(options?: {
  autoAddOllama?: boolean;
  persist?: boolean;
}): Promise<void> {
  let changed = false;

  const ollamaApis = draft.apis.filter((api) => api.kind === "ollama" && api.enabled !== false);
  for (const api of ollamaApis) {
    if (textModels(api).length > 0 && api.catalogFetchedAt) {
      if (
        pinModelsIntoBudget(
          draft,
          api,
          textModels(api).map((model) => model.id),
        )
      ) {
        changed = true;
      }
      continue;
    }
    if (await fetchApiCatalog(api.id, { quiet: true })) changed = true;
  }

  const localOpenAi = draft.apis.filter(
    (api) => api.kind === "openai-chat" && api.enabled !== false && isLocalLlmBaseUrl(api.baseUrl),
  );
  for (const api of localOpenAi) {
    if (textModels(api).length > 0 && api.catalogFetchedAt) {
      if (
        pinModelsIntoBudget(
          draft,
          api,
          textModels(api).map((model) => model.id),
        )
      ) {
        changed = true;
      }
      continue;
    }
    if (await fetchApiCatalog(api.id, { quiet: true })) changed = true;
  }

  if (options?.autoAddOllama && !draft.apis.some((api) => api.kind === "ollama")) {
    try {
      const catalog = await invoke<CatalogModel[]>("fetch_api_catalog", {
        kind: "ollama",
        apiKey: "",
        baseUrl: protocolMeta("ollama").defaultBaseUrl,
      });
      if (catalog.length > 0) {
        const entry = emptyApiEntry("ollama", "Ollama (local)");
        entry.catalog = catalog;
        entry.catalogFetchedAt = new Date().toISOString();
        draft.apis.push(entry);
        pinModelsIntoBudget(
          draft,
          entry,
          textModels(entry).map((model) => model.id),
        );
        changed = true;
        host?.onStatus(
          `Found ${catalog.length} local Ollama model(s) — added “${entry.name}” to Settings`,
        );
      }
    } catch {
      // Ollama not running; leave Settings alone.
    }
  }

  if (changed && options?.persist) {
    try {
      await persistSettings();
    } catch {
      // Draft still has the catalog for the UI; user can Save manually.
      notifyDraftChanged();
    }
  } else if (changed) {
    notifyDraftChanged();
  }
}

export function populateChatModelSelect(select: HTMLSelectElement): void {
  const settings = draft;
  const previous = select.value;
  select.innerHTML = "";

  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = "No chat model selected";
  select.append(placeholder);

  let optionCount = 0;
  for (const api of chatCapableApis(settings)) {
    if (!api.apiKey && !apiAllowsEmptyKey(api)) continue;
    const models = dropdownLlmModels(api);
    if (models.length === 0) continue;

    const group = document.createElement("optgroup");
    group.label = api.name;
    for (const model of models) {
      const option = document.createElement("option");
      option.value = modelOptionValue(api.id, model.id);
      option.textContent = model.displayName;
      group.append(option);
      optionCount += 1;
    }
    select.append(group);
  }

  const preferred = settings.selectedChatModel
    ? modelOptionValue(settings.selectedChatModel.apiId, settings.selectedChatModel.modelId)
    : "";

  if (preferred && select.querySelector(`option[value="${cssEscape(preferred)}"]`)) {
    select.value = preferred;
  } else if (previous && select.querySelector(`option[value="${cssEscape(previous)}"]`)) {
    select.value = previous;
  } else {
    select.value = "";
  }

  select.disabled = optionCount === 0;
  if (optionCount === 0) {
    placeholder.textContent = "Configure a chat API in Settings";
  }
}

export function setSelectedChatModelFromSelect(value: string): void {
  draft.selectedChatModel = parseModelOption(value);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function escapeAttr(value: string): string {
  return escapeHtml(value).replace(/`/g, "&#96;");
}

function cssEscape(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
