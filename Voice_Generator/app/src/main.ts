import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

type SourceMode = "recorded" | "built-in";
type BuiltinVoice = {
  catalog_id: string;
  provider: string;
  model_id: string;
  voice_id: string;
  voice_name: string;
  gender: string | null;
  language: string;
  language_label: string;
  age: string | null;
  example_url: string | null;
  voice_field: string;
};
type BuiltinVoiceCatalog = {
  language_filter: string;
  sample_text: string;
  voices: BuiltinVoice[];
};
type RecordingSource = { id: number; name: string };

const appElement = document.querySelector<HTMLDivElement>("#app");

if (!appElement) {
  throw new Error("Application root was not found.");
}

const CHARACTER_STORAGE_KEY = "voice-profile-studio.character-target";

const app = appElement;
let activeMode: SourceMode = "built-in";
let isRecording = false;
let recordingPath: string | null = null;
let recordingReady = false;
let recordingSaved = false;
let recordingDuration = 0;
let recordingStartedAt = 0;
let playbackUrl: string | null = null;
let playbackPath: string | null = null;
let meterAnimationFrame: number | null = null;
let levelPollInterval: number | null = null;
let recordingSpectrum: number[] = [];
let recordingSources: RecordingSource[] = [];
let recordingSourcesError: string | null = null;
let recordingSourceId: number | null = null;
let recordingError: string | null = null;
let saveMessage: string | null = null;
let characterName = "";
let characterDirectory = "";
let characterReferencePath = "";
let recordingScript = "Technology is not separate from the world around us. It is one more way the world speaks.";
let builtinCatalog: BuiltinVoiceCatalog | null = null;
let builtinLoading = false;
let builtinError: string | null = null;
let builtinProviderFilter = "all";
let builtinBusyId: string | null = null;
let builtinPreviewUrl: string | null = null;
let builtinPreviewCatalogId: string | null = null;
let builtinSaveMessage: string | null = null;
let builtinAutoplay = false;

function loadStoredCharacterTarget(): void {
  try {
    const raw = localStorage.getItem(CHARACTER_STORAGE_KEY);
    if (!raw) return;
    const stored = JSON.parse(raw) as {
      characterName?: string;
      characterDirectory?: string;
      characterReferencePath?: string;
    };
    characterName = stored.characterName || "";
    characterDirectory = stored.characterDirectory || "";
    characterReferencePath = stored.characterReferencePath || "";
  } catch {
    // Ignore corrupt localStorage and keep empty defaults.
  }
}

function persistCharacterTarget(): void {
  localStorage.setItem(
    CHARACTER_STORAGE_KEY,
    JSON.stringify({
      characterName,
      characterDirectory,
      characterReferencePath,
    }),
  );
}

function clearBuiltinPreview(options: { keepSelection?: boolean } = {}): void {
  const player = document.querySelector<HTMLAudioElement>("#builtin-player");
  if (player) {
    player.pause();
    player.removeAttribute("src");
    player.load();
  }
  if (builtinPreviewUrl) {
    URL.revokeObjectURL(builtinPreviewUrl);
  }
  builtinPreviewUrl = null;
  builtinAutoplay = false;
  if (!options.keepSelection) {
    builtinPreviewCatalogId = null;
  }
}

const sourceCards: Record<SourceMode, { title: string; description: string }> = {
  recorded: {
    title: "Record a voice",
    description: "Record and approve a voice clip using this computer's microphone.",
  },
  "built-in": {
    title: "Choose a built-in voice",
    description: "Browse English Atlas Cloud voices, play samples, and save one for this character.",
  },
};

function render(): void {
  app.innerHTML = `
    <div class="app-shell">
      <section class="workspace">
        <header class="topbar">
          <div>
            <p class="eyebrow">Voice Profile Studio</p>
            <h2>Give your character a voice</h2>
          </div>
        </header>

        <main class="content">
          <section class="panel character-panel">
            <div class="section-heading">
              <div>
                <p class="section-kicker">Step 1</p>
                <h3>Character target</h3>
              </div>
              <span class="badge">Local files</span>
            </div>
            <p class="muted">Voice profiles belong to one character. Choose an existing directory or create a new target.</p>
            <div class="form-grid">
              <label>
                Character name
                <input id="character-name" value="${escapeHtml(characterName)}" placeholder="e.g. Kris Thornvale" />
              </label>
              <label>
                Character directory
                <div class="directory-picker">
                  <input id="character-directory" value="${escapeHtml(characterDirectory)}" placeholder="/path/to/character/" />
                  <button class="secondary-button" id="choose-directory" type="button">Browse…</button>
                </div>
              </label>
              <label>
                Character reference sheet
                <div class="file-picker">
                  <input id="character-reference-path" value="${escapeHtml(characterReferencePath)}" placeholder="/path/to/character/reference.md" />
                  <button class="secondary-button" id="choose-reference" type="button">Browse…</button>
                </div>
              </label>
            </div>
          </section>

          <section class="panel">
            <div class="section-heading">
              <div>
                <p class="section-kicker">Step 2</p>
                <h3>How should this voice be created?</h3>
              </div>
            </div>
            <div class="source-grid">
              ${(Object.keys(sourceCards) as SourceMode[]).map((mode) => `
                <button class="source-card ${activeMode === mode ? "selected" : ""}" data-mode="${mode}" type="button">
                  <span class="source-card-copy">
                    <strong>${sourceCards[mode].title}</strong>
                    <small>${sourceCards[mode].description}</small>
                  </span>
                </button>
              `).join("")}
            </div>
          </section>

          <section class="panel mode-panel">
            ${renderModePanel()}
          </section>
        </main>
      </section>
    </div>
  `;

  bindEvents();
}

function renderModePanel(): string {
  if (activeMode === "recorded") {
    return `
      <div class="section-heading">
        <div>
          <p class="section-kicker">Recorded voice</p>
          <h3>Record a reference clip</h3>
        </div>
        <span class="badge">No Atlas call</span>
      </div>
      <p class="muted">Read the sample below in the voice you want to preserve. Your recording stays local until you approve and save it.</p>
      <div class="script-card">
        <span class="quote-mark">“</span>
        <textarea id="recording-script">${escapeHtml(recordingScript)}</textarea>
        <span class="script-label">Suggested sample script</span>
      </div>
      <div class="recording-meter ${isRecording ? "active" : ""}">
        <div class="meter-header"><span>Live frequency scale</span><span>${formatDuration(recordingDuration)}</span></div>
        <div id="recording-spectrum" class="recording-spectrum" aria-label="Recording frequency level">
          ${Array.from({ length: 48 }, () => '<span class="spectrum-bar"></span>').join("")}
        </div>
        <div class="frequency-scale"><span>0</span><span>1 kHz</span><span>2 kHz</span><span>4 kHz</span><span>8 kHz</span></div>
      </div>
      <label class="microphone-picker">
        Microphone
        <select id="recording-source" ${recordingSources.length ? "" : "disabled"}>
          ${recordingSources.length
            ? recordingSources.map((source) => `<option value="${source.id}" ${recordingSourceId === source.id ? "selected" : ""}>${escapeHtml(source.name)}</option>`).join("")
            : '<option>No PipeWire microphones found</option>'}
        </select>
      </label>
      ${recordingSourcesError ? `<div class="notice error">${recordingSourcesError}</div>` : ""}
      <div class="recording-controls">
        <button class="record-button ${isRecording ? "recording" : ""}" id="record-button"><span class="record-dot"></span>${isRecording ? "Stop recording" : "Start recording"}</button>
        <span id="recording-status" class="muted">${recordingError || (isRecording ? "Recording in progress…" : recordingSaved ? "Saved clip and metadata." : recordingReady ? "Recording ready to review." : "Native microphone recording is ready.")}</span>
      </div>
      ${recordingPath && recordingReady ? `
        <audio class="audio-player" id="recording-player" controls preload="metadata"${playbackUrl ? ` src="${playbackUrl}"` : ""}></audio>
        <div class="recording-actions">
          <button class="secondary-button" id="retake-button" type="button">Retake</button>
          <button class="primary-button" id="save-recording-button" type="button" ${recordingSaved ? "disabled" : ""}>${recordingSaved ? "Saved" : "Approve & save clip"}</button>
        </div>
        ${saveMessage ? `<div class="notice">${saveMessage}</div>` : ""}
      ` : ""}
      ${recordingError ? `<div class="notice error">${recordingError}</div>` : ""}
    `;
  }

  const providers = builtinCatalog
    ? Array.from(new Set(builtinCatalog.voices.map((voice) => voice.provider)))
    : [];
  const visibleVoices = (builtinCatalog?.voices || []).filter(
    (voice) => builtinProviderFilter === "all" || voice.provider === builtinProviderFilter,
  );

  return `
    <div class="section-heading">
      <div>
        <p class="section-kicker">Built-in Atlas voice</p>
        <h3>Browse English voices</h3>
      </div>
      <span class="badge">${builtinCatalog ? `${visibleVoices.length} voices` : builtinLoading ? "Loading…" : "Not loaded"}</span>
    </div>
    <p class="muted">English-only presets from xAI, ElevenLabs, Gemini, and MiniMax. Play each sample with the script below, then choose one to save as a character voice profile.</p>
    <div class="script-card">
      <span class="quote-mark">“</span>
      <textarea id="recording-script">${escapeHtml(recordingScript)}</textarea>
      <span class="script-label">Sample script for previews and saved clips</span>
    </div>
    ${builtinError ? `<div class="notice error">${builtinError}</div>` : ""}
    ${builtinSaveMessage ? `<div class="notice">${builtinSaveMessage}</div>` : ""}
    ${builtinCatalog ? `
      <div class="voice-toolbar">
        <label>
          Provider
          <select id="builtin-provider-filter">
            <option value="all" ${builtinProviderFilter === "all" ? "selected" : ""}>All providers</option>
            ${providers.map((provider) => `<option value="${escapeHtml(provider)}" ${builtinProviderFilter === provider ? "selected" : ""}>${escapeHtml(provider)}</option>`).join("")}
          </select>
        </label>
        <p class="muted">Seed Audio is omitted because its presets are Chinese-only.</p>
      </div>
      <div class="catalog-list">
        ${visibleVoices.map((voice) => `
          <article class="catalog-row voice-row ${builtinPreviewCatalogId === voice.catalog_id ? "selected" : ""}">
            <div class="placeholder-icon">◌</div>
            <div class="voice-copy">
              <strong>${escapeHtml(voice.voice_name)}</strong>
              <p>${escapeHtml(voice.provider)} · ${escapeHtml(voice.model_id)} · ${escapeHtml(voice.language_label)}${voice.gender ? ` · ${escapeHtml(voice.gender)}` : ""}${voice.age ? ` · ${escapeHtml(voice.age)}` : ""}</p>
              <p class="voice-id">voice: ${escapeHtml(voice.voice_id)}</p>
            </div>
            <div class="voice-actions">
              <button class="secondary-button" type="button" data-preview-voice="${escapeHtml(voice.catalog_id)}" ${builtinBusyId ? "disabled" : ""}>${builtinBusyId === voice.catalog_id ? "Working…" : "Play"}</button>
              <button class="primary-button" type="button" data-save-voice="${escapeHtml(voice.catalog_id)}" ${builtinBusyId ? "disabled" : ""}>Choose</button>
            </div>
          </article>
        `).join("")}
      </div>
      ${builtinPreviewUrl ? `<audio class="audio-player" id="builtin-player" controls${builtinAutoplay ? " autoplay" : ""} src="${builtinPreviewUrl}"></audio>` : ""}
    ` : `
      <div class="catalog-placeholder">
        <div class="placeholder-icon">◌</div>
        <div>
          <strong>English Atlas voice library</strong>
          <p>${builtinLoading ? "Loading the English built-in voice catalog…" : "The English voice library will appear here shortly."}</p>
        </div>
      </div>
    `}
  `;
}

function bindEvents(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-mode]").forEach((button) => {
    button.addEventListener("click", () => {
      const nextMode = button.dataset.mode as SourceMode;
      if (nextMode === activeMode) return;
      if (activeMode === "built-in" || nextMode === "built-in") {
        clearBuiltinPreview();
        builtinSaveMessage = null;
        builtinError = null;
      }
      activeMode = nextMode;
      render();
    });
  });

  document.querySelector<HTMLInputElement>("#character-name")?.addEventListener("input", (event) => {
    characterName = (event.target as HTMLInputElement).value;
    persistCharacterTarget();
  });
  document.querySelector<HTMLInputElement>("#character-directory")?.addEventListener("input", (event) => {
    characterDirectory = (event.target as HTMLInputElement).value;
    persistCharacterTarget();
  });
  document.querySelector<HTMLInputElement>("#character-reference-path")?.addEventListener("input", (event) => {
    characterReferencePath = (event.target as HTMLInputElement).value;
    persistCharacterTarget();
  });
  document.querySelector<HTMLTextAreaElement>("#recording-script")?.addEventListener("input", (event) => {
    recordingScript = (event.target as HTMLTextAreaElement).value;
  });
  document.querySelector<HTMLSelectElement>("#recording-source")?.addEventListener("change", (event) => {
    recordingSourceId = Number((event.target as HTMLSelectElement).value);
  });
  document.querySelector<HTMLButtonElement>("#record-button")?.addEventListener("click", toggleRecording);
  document.querySelector<HTMLButtonElement>("#choose-directory")?.addEventListener("click", chooseDirectory);
  document.querySelector<HTMLButtonElement>("#choose-reference")?.addEventListener("click", chooseCharacterReference);
  document.querySelector<HTMLButtonElement>("#save-recording-button")?.addEventListener("click", saveRecording);
  document.querySelector<HTMLButtonElement>("#retake-button")?.addEventListener("click", retakeRecording);
  document.querySelector<HTMLAudioElement>("#recording-player")?.addEventListener("error", () => {
    updateRecordingStatus("Playback could not open this WAV file. Retake the recording and try again.");
  });
  if (recordingPath && recordingReady && playbackPath !== recordingPath) {
    void prepareAudioPlayback(recordingPath);
  }
  if (isRecording) {
    startMeterAnimation();
  }

  document.querySelector<HTMLSelectElement>("#builtin-provider-filter")?.addEventListener("change", (event) => {
    builtinProviderFilter = (event.target as HTMLSelectElement).value;
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-preview-voice]").forEach((button) => {
    button.addEventListener("click", () => {
      void previewBuiltinVoice(button.dataset.previewVoice || "");
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-save-voice]").forEach((button) => {
    button.addEventListener("click", () => {
      void saveBuiltinVoice(button.dataset.saveVoice || "");
    });
  });
}

async function toggleRecording(): Promise<void> {
  recordingError = null;
  saveMessage = null;
  if (isRecording) {
    try {
      recordingPath = await invoke<string>("stop_recording");
      resetPlaybackUrl();
      isRecording = false;
      recordingReady = true;
      recordingSaved = false;
      recordingDuration = Math.max(0, Math.round((Date.now() - recordingStartedAt) / 1000));
      stopMeterAnimation();
      render();
    } catch (error) {
      isRecording = false;
      stopMeterAnimation();
      recordingError = `Recording could not be finalized: ${String(error)}`;
      render();
    }
    return;
  }

  try {
    const outputPath = `/tmp/voice-profile-studio-${Date.now()}.wav`;
    recordingPath = await invoke<string>("start_recording", {
      outputPath,
      sourceId: recordingSourceId,
    });
    isRecording = true;
    recordingReady = false;
    recordingSaved = false;
    recordingDuration = 0;
    recordingStartedAt = Date.now();
    render();
  } catch (error) {
    recordingError = `Microphone recording failed: ${String(error)}. Check PipeWire/ALSA access.`;
    render();
  }
}

async function chooseDirectory(): Promise<void> {
  const selected = await open({ directory: true, multiple: false, title: "Choose character directory" });
  if (typeof selected !== "string") {
    return;
  }
  characterDirectory = selected;
  persistCharacterTarget();
  const input = document.querySelector<HTMLInputElement>("#character-directory");
  if (input) input.value = selected;
}

async function chooseCharacterReference(): Promise<void> {
  const selected = await open({
    directory: false,
    multiple: false,
    title: "Choose character reference sheet",
    filters: [{ name: "Character reference", extensions: ["md", "markdown", "txt"] }],
  });
  if (typeof selected !== "string") {
    return;
  }
  characterReferencePath = selected;
  persistCharacterTarget();
  const input = document.querySelector<HTMLInputElement>("#character-reference-path");
  if (input) input.value = selected;
}

async function saveRecording(): Promise<void> {
  if (!recordingPath || !recordingReady) return;
  const selectedDirectory = characterDirectory.trim();
  const selectedName = characterName.trim() || "Character";
  const sampleText = recordingScript.trim();
  const selectedReferencePath = characterReferencePath.trim();
  if (!selectedDirectory) {
    recordingError = "Choose a character directory before saving.";
    render();
    return;
  }

  try {
    const saved = await invoke<{ audio_path: string; metadata_path: string }>("save_recording", {
      request: {
        temp_path: recordingPath,
        character_directory: selectedDirectory,
        character_name: selectedName,
        sample_text: sampleText,
        character_reference_path: selectedReferencePath || null,
      },
    });
    recordingPath = saved.audio_path;
    resetPlaybackUrl();
    recordingSaved = true;
    saveMessage = `Saved clip and text metadata: ${saved.metadata_path}`;
    render();
  } catch (error) {
    recordingError = `Could not save the recording: ${String(error)}`;
    render();
  }
}

function retakeRecording(): void {
  resetPlaybackUrl();
  recordingPath = null;
  recordingReady = false;
  recordingSaved = false;
  recordingDuration = 0;
  recordingError = null;
  saveMessage = null;
  render();
}

function resetPlaybackUrl(): void {
  if (playbackUrl) {
    URL.revokeObjectURL(playbackUrl);
  }
  playbackUrl = null;
  playbackPath = null;
}

function blobUrlFromAudioBytes(audioBytes: number[] | Uint8Array): string {
  const bytes = audioBytes instanceof Uint8Array ? audioBytes : new Uint8Array(audioBytes);
  return URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
}

async function prepareAudioPlayback(path: string): Promise<void> {
  const player = document.querySelector<HTMLAudioElement>("#recording-player");
  if (!player || playbackPath === path) return;
  try {
    const audioBytes = await invoke<number[]>("read_audio_file", { path });
    if (recordingPath !== path) return;
    resetPlaybackUrl();
    playbackUrl = blobUrlFromAudioBytes(audioBytes);
    playbackPath = path;
    player.src = playbackUrl;
    player.load();
  } catch (error) {
    updateRecordingStatus(`Playback could not be loaded: ${String(error)}`);
  }
}

function formatDuration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;",
  })[character] || character);
}

function updateMeter(spectrum: number[]): void {
  document.querySelectorAll<HTMLElement>(".spectrum-bar").forEach((bar, index) => {
    const activity = spectrum[index] || 0.02;
    bar.style.height = `${Math.max(4, activity * 112)}px`;
    bar.classList.toggle("active", activity > 0.04);
  });
}

function startMeterAnimation(): void {
  stopMeterAnimation();
  recordingSpectrum = [];
  levelPollInterval = window.setInterval(async () => {
    try {
      recordingSpectrum = await invoke<number[]>("recording_level");
    } catch {
      recordingSpectrum = [];
      updateRecordingStatus("Recorder stopped or microphone data is unavailable.");
    }
  }, 100);
  const animate = (): void => {
    recordingDuration = Math.max(0, Math.round((Date.now() - recordingStartedAt) / 1000));
    updateMeter(recordingSpectrum);
    const status = document.querySelector<HTMLSpanElement>("#recording-status");
    const signalDetected = recordingSpectrum.some((value) => value > 0.06);
    if (status) {
      status.textContent = signalDetected
        ? `Recording… audio detected · ${formatDuration(recordingDuration)}`
        : `Recording… waiting for microphone signal · ${formatDuration(recordingDuration)}`;
    }
    meterAnimationFrame = requestAnimationFrame(animate);
  };
  meterAnimationFrame = requestAnimationFrame(animate);
}

function stopMeterAnimation(): void {
  if (levelPollInterval !== null) {
    window.clearInterval(levelPollInterval);
    levelPollInterval = null;
  }
  if (meterAnimationFrame !== null) {
    cancelAnimationFrame(meterAnimationFrame);
    meterAnimationFrame = null;
  }
  recordingSpectrum = [];
}

async function loadRecordingSources(): Promise<void> {
  try {
    recordingSources = await invoke<RecordingSource[]>("list_recording_sources");
    if (recordingSources.length && recordingSourceId === null) {
      recordingSourceId = recordingSources[0].id;
    }
    render();
  } catch (error) {
    recordingSourcesError = `Microphone list unavailable: ${String(error)}`;
    render();
  }
}

function updateRecordingStatus(message: string): void {
  const status = document.querySelector<HTMLSpanElement>("#recording-status");
  if (status) status.textContent = message;
}

async function loadBuiltinVoices(): Promise<void> {
  builtinLoading = true;
  builtinError = null;
  render();
  try {
    builtinCatalog = await invoke<BuiltinVoiceCatalog>("list_builtin_voices");
  } catch (error) {
    builtinError = `Could not load the English voice library: ${String(error)}`;
  } finally {
    builtinLoading = false;
    render();
  }
}

async function previewBuiltinVoice(catalogId: string): Promise<void> {
  if (!catalogId) return;
  clearBuiltinPreview();
  builtinBusyId = catalogId;
  builtinPreviewCatalogId = catalogId;
  builtinError = null;
  builtinSaveMessage = null;
  render();
  try {
    const preview = await invoke<{
      audio_path: string;
      audio_bytes: number[];
      cached: boolean;
    }>("preview_builtin_voice", {
      request: {
        catalog_id: catalogId,
        sample_text: recordingScript.trim(),
      },
    });
    if (!preview.audio_bytes?.length) {
      throw new Error("Preview returned an empty audio sample.");
    }
    if (builtinBusyId !== catalogId) {
      return;
    }
    builtinPreviewUrl = blobUrlFromAudioBytes(preview.audio_bytes);
    builtinPreviewCatalogId = catalogId;
    builtinAutoplay = true;
    builtinSaveMessage = preview.cached
      ? "Playing cached sample for this script."
      : "Generated a fresh sample for this script.";
  } catch (error) {
    if (builtinBusyId === catalogId) {
      builtinError = `Preview failed: ${String(error)}`;
      clearBuiltinPreview();
    }
  } finally {
    if (builtinBusyId === catalogId) {
      builtinBusyId = null;
      render();
      builtinAutoplay = false;
    }
  }
}

async function saveBuiltinVoice(catalogId: string): Promise<void> {
  if (!catalogId) return;
  const selectedDirectory = characterDirectory.trim();
  const selectedName = characterName.trim() || "Character";
  const sampleText = recordingScript.trim();
  const selectedReferencePath = characterReferencePath.trim();
  if (!selectedDirectory) {
    builtinError = "Choose a character directory before saving.";
    render();
    return;
  }
  if (!sampleText) {
    builtinError = "Sample text is required before saving.";
    render();
    return;
  }

  clearBuiltinPreview();
  builtinBusyId = catalogId;
  builtinPreviewCatalogId = catalogId;
  builtinError = null;
  builtinSaveMessage = null;
  render();
  try {
    const saved = await invoke<{
      audio_path: string;
      audio_bytes: number[];
      metadata_path: string;
    }>("save_builtin_voice", {
      request: {
        catalog_id: catalogId,
        sample_text: sampleText,
        character_directory: selectedDirectory,
        character_name: selectedName,
        character_reference_path: selectedReferencePath || null,
      },
    });
    builtinPreviewCatalogId = catalogId;
    builtinSaveMessage = `Saved clip and voice metadata: ${saved.metadata_path}`;
    if (saved.audio_bytes?.length) {
      builtinPreviewUrl = blobUrlFromAudioBytes(saved.audio_bytes);
      builtinAutoplay = false;
    }
  } catch (error) {
    builtinError = `Could not save the built-in voice: ${String(error)}`;
  } finally {
    builtinBusyId = null;
    render();
  }
}

loadStoredCharacterTarget();
render();
void loadRecordingSources();
void loadBuiltinVoices();
