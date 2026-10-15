import { invoke, openDialog } from "./host";

export type VoiceHost = {
  root: HTMLElement;
  projectRoot: string | null | (() => string | null);
  selectedPath: string | null | (() => string | null);
  onStatus: (left: string, right?: string) => void;
  onSaved?: () => void;
};

function hostProjectRoot(): string | null {
  const value = host?.projectRoot;
  return typeof value === "function" ? value() : (value ?? null);
}

type SourceMode = "recorded" | "library" | "designed" | "existing";

type ExistingAudio = { name: string; path: string };

const AUDIO_FILE = /\.(wav|mp3|m4a|ogg|flac|aac)$/i;

type RecordingSource = { id: number; name: string };

type LibraryVoice = {
  voiceId: string;
  name: string;
  category?: string | null;
  description?: string | null;
  previewUrl?: string | null;
  gender?: string | null;
  age?: string | null;
  accent?: string | null;
};

type DesignPreview = {
  generatedVoiceId: string;
  audioBase64: string;
  mediaType: string;
  durationSecs?: number | null;
  language?: string | null;
};

type SavedVoiceProfile = {
  audioPath: string;
  metadataPath: string;
};

const DEFAULT_SAMPLE =
  "Technology is not separate from the world around us. It is one more way the world speaks. Can you hear the difference between a quiet answer and a sharp question? My name is ready for this story.";

const sourceCards: Record<SourceMode, { title: string; description: string }> = {
  recorded: {
    title: "Record a voice",
    description: "Capture a microphone clip and save it as this character’s voice profile.",
  },
  library: {
    title: "Choose from library",
    description: "Browse ElevenLabs voices, play the same sample, and pick one.",
  },
  designed: {
    title: "Design a voice",
    description: "Set age, gender, pace, and other traits, then compare generated candidates.",
  },
  existing: {
    title: "Use an existing file",
    description: "Register an audio file already in this character’s voice folder, or browse to one.",
  },
};

let host: VoiceHost | null = null;
let activeMode: SourceMode = "library";
let characterName = "";
let characterDirectory = "";
let characterReferencePath = "";
let sampleText = DEFAULT_SAMPLE;
let statusMessage: string | null = null;
let errorMessage: string | null = null;
let busy = false;

let isRecording = false;
let recordingPath: string | null = null;
let recordingReady = false;
let recordingSources: RecordingSource[] = [];
let recordingSourceId: number | null = null;
let recordingSpectrum: number[] = Array.from({ length: 48 }, () => 0);
let levelPoll: number | null = null;
let recordingObjectUrl: string | null = null;

let libraryVoices: LibraryVoice[] = [];
let libraryFilter = "default";
let librarySearch = "";
let libraryNextToken: string | null = null;
let libraryHasMore = false;
let selectedLibraryId: string | null = null;
let libraryPreviewUrl: string | null = null;
let libraryAutoplay = false;

let designGender = "female";
let designAge = "young adult";
let designPace = "conversational";
let designPitch = "medium";
let designTexture = "clear";
let designEnergy = "warm";
let designAccent = "General American";
let designNote = "";
let designPreviews: DesignPreview[] = [];
let selectedDesignId: string | null = null;
let designPreviewUrl: string | null = null;
let designAutoplay = false;

let existingAudio: ExistingAudio[] = [];
let selectedExistingPath: string | null = null;

export function bindVoiceHost(next: VoiceHost): void {
  host = next;
}

function setNotice(error: string | null, status: string | null = null): void {
  errorMessage = error;
  statusMessage = status;
}

function revokeUrl(url: string | null): void {
  if (url) URL.revokeObjectURL(url);
}

function pausePlayersInRoot(): void {
  host?.root.querySelectorAll("audio").forEach((player) => {
    player.pause();
    player.removeAttribute("src");
    player.load();
  });
}

function clearLibraryPreview(options: { keepSelection?: boolean } = {}): void {
  pausePlayersInRoot();
  revokeUrl(libraryPreviewUrl);
  libraryPreviewUrl = null;
  libraryAutoplay = false;
  if (!options.keepSelection) {
    selectedLibraryId = null;
  }
}

function clearDesignPreview(options: { keepSelection?: boolean } = {}): void {
  pausePlayersInRoot();
  revokeUrl(designPreviewUrl);
  designPreviewUrl = null;
  designAutoplay = false;
  if (!options.keepSelection) {
    selectedDesignId = null;
  }
}

function clearRecordingPlayback(): void {
  pausePlayersInRoot();
  revokeUrl(recordingObjectUrl);
  recordingObjectUrl = null;
}

function clearAllPlayback(): void {
  clearLibraryPreview();
  clearDesignPreview();
  clearRecordingPlayback();
}

function audioUrlFromBase64(base64: string, mediaType: string): string {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: mediaType || "audio/mpeg" }));
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

function inferCharacterTarget(projectRoot: string | null, selectedPath: string | null): void {
  if (!selectedPath) {
    if (projectRoot && !characterDirectory) {
      characterDirectory = `${projectRoot.replace(/\/$/, "")}/Characters`;
    }
    return;
  }
  const normalized = selectedPath.replace(/\/$/, "");
  const parts = normalized.split("/");
  const base = parts[parts.length - 1] || "";
  const isFile = base.includes(".");
  if (isFile && /\.(md|markdown|txt)$/i.test(base)) {
    characterReferencePath = normalized;
    characterDirectory = parts.slice(0, -1).join("/");
    characterName = base.replace(/\.(md|markdown|txt)$/i, "").replace(/[_-]/g, " ");
    return;
  }
  characterDirectory = normalized;
  characterName = base.replace(/[_-]/g, " ");
  characterReferencePath = "";
}

function buildDesignDescription(): string {
  const note = designNote.trim();
  const base = `A ${designEnergy} ${designGender} ${designAge} voice with a ${designPitch} pitch, ${designPace} pace, and ${designTexture} texture. ${designAccent} accent. Suitable for character dialogue and narration.`;
  return note ? `${base} ${note}` : base;
}

function ensureCharacterReady(): boolean {
  if (!characterName.trim()) {
    setNotice("Enter a character name before continuing.");
    renderVoicePanel();
    return false;
  }
  if (!characterDirectory.trim()) {
    setNotice("Choose a character directory before continuing.");
    renderVoicePanel();
    return false;
  }
  return true;
}

export function renderVoicePanel(): void {
  if (!host) return;
  const root = host.root;
  root.innerHTML = `
    <div class="voice-page">
      <header class="voice-header">
        <div>
          <h1>Character Voice</h1>
          <p class="settings-lede">Record, pick from the ElevenLabs library, design from traits, or register a file already on disk.</p>
        </div>
      </header>

      <section class="voice-section">
        <h2>Character target</h2>
        <div class="voice-form-grid">
          <label class="settings-field">
            <span>Character name</span>
            <input id="voice-character-name" value="${escapeAttr(characterName)}" placeholder="e.g. Mom" />
          </label>
          <label class="settings-field">
            <span>Character directory</span>
            <div class="voice-path-row">
              <input id="voice-character-directory" value="${escapeAttr(characterDirectory)}" placeholder="/path/to/Characters/Mom" />
              <button type="button" class="ghost-button" data-action="browse-directory">Browse…</button>
            </div>
          </label>
          <label class="settings-field">
            <span>Character sheet (optional)</span>
            <div class="voice-path-row">
              <input id="voice-character-reference" value="${escapeAttr(characterReferencePath)}" placeholder="/path/to/Mom.md" />
              <button type="button" class="ghost-button" data-action="browse-reference">Browse…</button>
            </div>
          </label>
        </div>
        ${existingAudioNotice()}
      </section>

      <section class="voice-section">
        <h2>How should this voice be created?</h2>
        <div class="voice-source-grid">
          ${(Object.keys(sourceCards) as SourceMode[])
            .map(
              (mode) => `
            <button type="button" class="voice-source-card ${activeMode === mode ? "selected" : ""}" data-mode="${mode}">
              <strong>${sourceCards[mode].title}</strong>
              <small>${sourceCards[mode].description}</small>
            </button>`,
            )
            .join("")}
        </div>
      </section>

      <section class="voice-section">
        <label class="settings-field">
          <span>Sample script (used for previews and saved clips)</span>
          <textarea id="voice-sample-text" rows="3">${escapeHtml(sampleText)}</textarea>
        </label>
      </section>

      <section class="voice-section mode-panel">
        ${renderModePanel()}
      </section>

      ${errorMessage ? `<div class="voice-notice error">${escapeHtml(errorMessage)}</div>` : ""}
      ${statusMessage ? `<div class="voice-notice">${escapeHtml(statusMessage)}</div>` : ""}
    </div>
  `;
  wireVoiceDom(root);
  // Autoplay is one-shot: survive this render only, never later refreshes/mode switches.
  libraryAutoplay = false;
  designAutoplay = false;
}

function renderModePanel(): string {
  if (activeMode === "recorded") {
    return `
      <h2>Record</h2>
      <p class="settings-help">Read the sample in the voice you want to keep. Approve only after you like the playback.</p>
      <div class="voice-meter">
        <div class="voice-meter-bars">
          ${recordingSpectrum.map((level) => `<span style="height:${Math.max(8, Math.round(level * 100))}%"></span>`).join("")}
        </div>
      </div>
      <label class="settings-field">
        <span>Microphone</span>
        <select id="voice-mic" ${recordingSources.length ? "" : "disabled"}>
          ${
            recordingSources.length
              ? recordingSources
                  .map(
                    (source) =>
                      `<option value="${source.id}" ${recordingSourceId === source.id ? "selected" : ""}>${escapeHtml(source.name)}</option>`,
                  )
                  .join("")
              : "<option>No PipeWire microphones found</option>"
          }
        </select>
      </label>
      <div class="voice-actions">
        <button type="button" class="primary-button" data-action="toggle-record" ${busy ? "disabled" : ""}>
          ${isRecording ? "Stop recording" : "Start recording"}
        </button>
        ${
          recordingReady
            ? `<button type="button" class="ghost-button" data-action="retake" ${busy ? "disabled" : ""}>Retake</button>
               <button type="button" class="primary-button" data-action="save-recording" ${busy ? "disabled" : ""}>Approve & save</button>`
            : ""
        }
      </div>
      ${recordingObjectUrl ? `<audio class="voice-player" controls src="${recordingObjectUrl}"></audio>` : ""}
    `;
  }

  if (activeMode === "library") {
    return `
      <h2>ElevenLabs library</h2>
      <div class="voice-toolbar">
        <label class="settings-field">
          <span>Search</span>
          <input id="voice-library-search" value="${escapeAttr(librarySearch)}" placeholder="Name, accent, age…" />
        </label>
        <label class="settings-field">
          <span>Type</span>
          <select id="voice-library-type">
            <option value="default" ${libraryFilter === "default" ? "selected" : ""}>Default library</option>
            <option value="community" ${libraryFilter === "community" ? "selected" : ""}>Community</option>
            <option value="personal" ${libraryFilter === "personal" ? "selected" : ""}>My voices</option>
            <option value="all" ${libraryFilter === "all" ? "selected" : ""}>All</option>
          </select>
        </label>
        <button type="button" class="ghost-button" data-action="load-library" ${busy ? "disabled" : ""}>
          ${busy && activeMode === "library" ? "Loading…" : "Load / refresh"}
        </button>
      </div>
      <div class="voice-catalog">
        ${
          libraryVoices.length
            ? libraryVoices
                .map(
                  (voice) => `
            <article class="voice-row ${selectedLibraryId === voice.voiceId ? "selected" : ""}">
              <div class="voice-copy">
                <strong>${escapeHtml(voice.name)}</strong>
                <p>${[voice.category, voice.gender, voice.age, voice.accent].filter(Boolean).map((part) => escapeHtml(String(part))).join(" · ") || "ElevenLabs voice"}</p>
                ${voice.description ? `<p class="voice-desc">${escapeHtml(voice.description)}</p>` : ""}
              </div>
              <div class="voice-row-actions">
                <button type="button" class="ghost-button" data-preview-library="${escapeAttr(voice.voiceId)}" ${busy ? "disabled" : ""}>Play</button>
                <button type="button" class="primary-button" data-save-library="${escapeAttr(voice.voiceId)}" ${busy ? "disabled" : ""}>Choose</button>
              </div>
            </article>`,
                )
                .join("")
            : `<p class="settings-help">Load the library to hear voices next to each other.</p>`
        }
      </div>
      ${
        libraryHasMore
          ? `<button type="button" class="ghost-button" data-action="load-more-library" ${busy ? "disabled" : ""}>Load more</button>`
          : ""
      }
      ${
        libraryPreviewUrl
          ? `<audio class="voice-player" controls${libraryAutoplay ? " autoplay" : ""} src="${libraryPreviewUrl}"></audio>`
          : ""
      }
    `;
  }

  if (activeMode === "existing") {
    const browsed =
      selectedExistingPath && !existingAudio.some((file) => file.path === selectedExistingPath)
        ? selectedExistingPath
        : null;
    const rows = [
      ...existingAudio.map((file) => ({ name: file.name, path: file.path })),
      ...(browsed
        ? [{ name: browsed.split("/").pop() || browsed, path: browsed }]
        : []),
    ];
    return `
      <h2>Existing file</h2>
      <p class="settings-help">A file already in the voice folder stays where it is. A file from somewhere else is copied in, then registered as this character’s voice.</p>
      <div class="voice-actions">
        <button type="button" class="ghost-button" data-action="refresh-existing" ${busy ? "disabled" : ""}>Refresh folder</button>
        <button type="button" class="ghost-button" data-action="browse-existing" ${busy ? "disabled" : ""}>Browse…</button>
      </div>
      <div class="voice-catalog">
        ${
          rows.length
            ? rows
                .map(
                  (file) => `
            <article class="voice-row ${selectedExistingPath === file.path ? "selected" : ""}">
              <div class="voice-copy">
                <strong>${escapeHtml(file.name)}</strong>
                <p>${escapeHtml(file.path)}</p>
              </div>
              <div class="voice-row-actions">
                <button type="button" class="primary-button" data-attach-existing="${escapeAttr(file.path)}" ${busy ? "disabled" : ""}>Use this file</button>
              </div>
            </article>`,
                )
                .join("")
            : `<p class="settings-help">No audio files in this character’s voice folder yet. Browse to the file you moved.</p>`
        }
      </div>
    `;
  }

  return `
    <h2>Design from characteristics</h2>
    <p class="settings-help">Dropdowns build an ElevenLabs voice description. Generate candidates, listen, then choose one.</p>
    <div class="voice-design-grid">
      ${renderSelect("Gender", "design-gender", designGender, ["female", "male", "neutral"])}
      ${renderSelect("Age", "design-age", designAge, ["young", "teen", "young adult", "middle-aged", "elderly"])}
      ${renderSelect("Pace", "design-pace", designPace, ["slow", "measured", "conversational", "quick"])}
      ${renderSelect("Pitch", "design-pitch", designPitch, ["low", "medium", "high"])}
      ${renderSelect("Texture", "design-texture", designTexture, ["smooth", "clear", "breathy", "gravelly", "nasal"])}
      ${renderSelect("Energy", "design-energy", designEnergy, ["calm", "warm", "lively", "intense"])}
      ${renderSelect("Accent", "design-accent", designAccent, ["General American", "British RP", "Irish", "Australian", "Southern US", "none specified"])}
      <label class="settings-field">
        <span>Extra note (optional)</span>
        <input id="design-note" value="${escapeAttr(designNote)}" placeholder="e.g. soft laugh under lines, slightly mischievous" />
      </label>
    </div>
    <p class="settings-meta">Prompt preview: ${escapeHtml(buildDesignDescription())}</p>
    <div class="voice-actions">
      <button type="button" class="primary-button" data-action="generate-design" ${busy ? "disabled" : ""}>
        ${busy && activeMode === "designed" ? "Generating…" : "Generate candidates"}
      </button>
    </div>
    <div class="voice-catalog">
      ${
        designPreviews.length
          ? designPreviews
              .map(
                (preview, index) => `
          <article class="voice-row ${selectedDesignId === preview.generatedVoiceId ? "selected" : ""}">
            <div class="voice-copy">
              <strong>Candidate ${index + 1}</strong>
              <p>${preview.durationSecs ? `${preview.durationSecs.toFixed(1)}s` : "preview"}${preview.language ? ` · ${escapeHtml(preview.language)}` : ""}</p>
            </div>
            <div class="voice-row-actions">
              <button type="button" class="ghost-button" data-preview-design="${escapeAttr(preview.generatedVoiceId)}" ${busy ? "disabled" : ""}>Play</button>
              <button type="button" class="primary-button" data-save-design="${escapeAttr(preview.generatedVoiceId)}" ${busy ? "disabled" : ""}>Choose</button>
            </div>
          </article>`,
              )
              .join("")
          : `<p class="settings-help">Candidates appear here after you generate.</p>`
      }
    </div>
    ${
      designPreviewUrl
        ? `<audio class="voice-player" controls${designAutoplay ? " autoplay" : ""} src="${designPreviewUrl}"></audio>`
        : ""
    }
  `;
}

function existingAudioNotice(): string {
  if (!existingAudio.length || activeMode === "existing") return "";
  const names = existingAudio.map((file) => file.name).join(", ");
  return `<p class="settings-help">Audio already in this voice folder: ${escapeHtml(names)}. Choose “Use an existing file” to register one.</p>`;
}

function renderSelect(label: string, id: string, value: string, options: string[]): string {
  return `
    <label class="settings-field">
      <span>${escapeHtml(label)}</span>
      <select id="${id}">
        ${options
          .map(
            (option) =>
              `<option value="${escapeAttr(option)}" ${option === value ? "selected" : ""}>${escapeHtml(option)}</option>`,
          )
          .join("")}
      </select>
    </label>
  `;
}

function wireVoiceDom(root: HTMLElement): void {
  root.querySelector<HTMLInputElement>("#voice-character-name")?.addEventListener("input", (event) => {
    characterName = (event.target as HTMLInputElement).value;
  });
  root.querySelector<HTMLInputElement>("#voice-character-directory")?.addEventListener("input", (event) => {
    characterDirectory = (event.target as HTMLInputElement).value;
  });
  root.querySelector<HTMLInputElement>("#voice-character-reference")?.addEventListener("input", (event) => {
    characterReferencePath = (event.target as HTMLInputElement).value;
  });
  root.querySelector<HTMLTextAreaElement>("#voice-sample-text")?.addEventListener("input", (event) => {
    sampleText = (event.target as HTMLTextAreaElement).value;
  });

  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-mode]")) {
    button.addEventListener("click", () => {
      const mode = button.dataset.mode as SourceMode;
      if (mode === activeMode) return;
      clearAllPlayback();
      activeMode = mode;
      setNotice(null);
      if (mode === "recorded") void loadRecordingSources();
      if (mode === "library" && libraryVoices.length === 0) void loadLibrary(false);
      if (mode === "existing") {
        void refreshExistingAudio();
        return;
      }
      renderVoicePanel();
    });
  }

  root.querySelector('[data-action="browse-directory"]')?.addEventListener("click", () => {
    void (async () => {
      const selected = await openDialog({ directory: true, multiple: false, defaultPath: characterDirectory || hostProjectRoot() || undefined });
      if (typeof selected === "string") {
        characterDirectory = selected;
        void refreshExistingAudio();
      }
    })();
  });
  root.querySelector('[data-action="browse-reference"]')?.addEventListener("click", () => {
    void (async () => {
      const selected = await openDialog({
        multiple: false,
        defaultPath: characterReferencePath || characterDirectory || hostProjectRoot() || undefined,
        filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }],
      });
      if (typeof selected === "string") {
        characterReferencePath = selected;
        renderVoicePanel();
      }
    })();
  });

  root.querySelector('[data-action="toggle-record"]')?.addEventListener("click", () => {
    void toggleRecording();
  });
  root.querySelector('[data-action="retake"]')?.addEventListener("click", () => {
    recordingReady = false;
    recordingPath = null;
    clearRecordingPlayback();
    setNotice(null, "Ready to record again.");
    renderVoicePanel();
  });
  root.querySelector('[data-action="save-recording"]')?.addEventListener("click", () => {
    void saveRecording();
  });
  root.querySelector('[data-action="refresh-existing"]')?.addEventListener("click", () => {
    void refreshExistingAudio();
  });
  root.querySelector('[data-action="browse-existing"]')?.addEventListener("click", () => {
    void browseExistingAudio();
  });
  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-attach-existing]")) {
    button.addEventListener("click", () => {
      const path = button.dataset.attachExisting;
      if (path) void attachExisting(path);
    });
  }
  root.querySelector<HTMLSelectElement>("#voice-mic")?.addEventListener("change", (event) => {
    recordingSourceId = Number((event.target as HTMLSelectElement).value);
  });

  root.querySelector('[data-action="load-library"]')?.addEventListener("click", () => {
    librarySearch = root.querySelector<HTMLInputElement>("#voice-library-search")?.value ?? librarySearch;
    libraryFilter = root.querySelector<HTMLSelectElement>("#voice-library-type")?.value ?? libraryFilter;
    void loadLibrary(false);
  });
  root.querySelector('[data-action="load-more-library"]')?.addEventListener("click", () => {
    void loadLibrary(true);
  });
  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-preview-library]")) {
    button.addEventListener("click", () => {
      const id = button.dataset.previewLibrary;
      if (id) void previewLibraryVoice(id);
    });
  }
  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-save-library]")) {
    button.addEventListener("click", () => {
      const id = button.dataset.saveLibrary;
      if (id) void saveLibraryVoice(id);
    });
  }

  const syncDesign = (): void => {
    designGender = root.querySelector<HTMLSelectElement>("#design-gender")?.value ?? designGender;
    designAge = root.querySelector<HTMLSelectElement>("#design-age")?.value ?? designAge;
    designPace = root.querySelector<HTMLSelectElement>("#design-pace")?.value ?? designPace;
    designPitch = root.querySelector<HTMLSelectElement>("#design-pitch")?.value ?? designPitch;
    designTexture = root.querySelector<HTMLSelectElement>("#design-texture")?.value ?? designTexture;
    designEnergy = root.querySelector<HTMLSelectElement>("#design-energy")?.value ?? designEnergy;
    designAccent = root.querySelector<HTMLSelectElement>("#design-accent")?.value ?? designAccent;
    designNote = root.querySelector<HTMLInputElement>("#design-note")?.value ?? designNote;
  };
  for (const id of [
    "design-gender",
    "design-age",
    "design-pace",
    "design-pitch",
    "design-texture",
    "design-energy",
    "design-accent",
    "design-note",
  ]) {
    root.querySelector(`#${id}`)?.addEventListener("change", () => {
      syncDesign();
      renderVoicePanel();
    });
    root.querySelector(`#${id}`)?.addEventListener("input", syncDesign);
  }
  root.querySelector('[data-action="generate-design"]')?.addEventListener("click", () => {
    syncDesign();
    void generateDesign();
  });
  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-preview-design]")) {
    button.addEventListener("click", () => {
      const id = button.dataset.previewDesign;
      if (id) previewDesign(id);
    });
  }
  for (const button of root.querySelectorAll<HTMLButtonElement>("[data-save-design]")) {
    button.addEventListener("click", () => {
      const id = button.dataset.saveDesign;
      if (id) void saveDesign(id);
    });
  }
}

async function loadRecordingSources(): Promise<void> {
  try {
    recordingSources = await invoke<RecordingSource[]>("voice_list_recording_sources");
    if (recordingSources.length && recordingSourceId == null) {
      recordingSourceId = recordingSources[0].id;
    }
  } catch (error) {
    recordingSources = [];
    setNotice(String(error));
  }
  renderVoicePanel();
}

async function toggleRecording(): Promise<void> {
  if (isRecording) {
    try {
      busy = true;
      const path = await invoke<string>("voice_stop_recording");
      isRecording = false;
      stopLevelPoll();
      recordingPath = path;
      recordingReady = true;
      revokeUrl(recordingObjectUrl);
      const payload = await invoke<ArrayBuffer | number[] | string>("read_media_bytes", {
        path,
      });
      const source =
        typeof payload === "string"
          ? Uint8Array.from(atob(payload), (c) => c.charCodeAt(0))
          : payload instanceof ArrayBuffer
            ? new Uint8Array(payload)
            : Uint8Array.from(payload as number[]);
      const copy = new Uint8Array(source.byteLength);
      copy.set(source);
      recordingObjectUrl = URL.createObjectURL(new Blob([copy], { type: "audio/wav" }));
      setNotice(null, "Recording ready — listen, then approve & save.");
    } catch (error) {
      setNotice(String(error));
      isRecording = false;
      stopLevelPoll();
    } finally {
      busy = false;
      renderVoicePanel();
    }
    return;
  }

  if (!ensureCharacterReady()) return;
  const tempDir = characterDirectory.replace(/\/$/, "");
  const outputPath = `${tempDir}/voice/.recording_temp.wav`;
  try {
    busy = true;
    recordingPath = await invoke<string>("voice_start_recording", {
      outputPath,
      sourceId: recordingSourceId,
    });
    isRecording = true;
    recordingReady = false;
    setNotice(null, "Recording…");
    startLevelPoll();
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    renderVoicePanel();
  }
}

function startLevelPoll(): void {
  stopLevelPoll();
  levelPoll = window.setInterval(() => {
    void (async () => {
      try {
        recordingSpectrum = await invoke<number[]>("voice_recording_level");
        const bars = host?.root.querySelectorAll<HTMLElement>(".voice-meter-bars span");
        bars?.forEach((bar, index) => {
          const level = recordingSpectrum[index] ?? 0;
          bar.style.height = `${Math.max(8, Math.round(level * 100))}%`;
        });
      } catch (error) {
        setNotice(String(error));
        isRecording = false;
        stopLevelPoll();
        renderVoicePanel();
      }
    })();
  }, 80);
}

function stopLevelPoll(): void {
  if (levelPoll != null) {
    window.clearInterval(levelPoll);
    levelPoll = null;
  }
}

async function saveRecording(): Promise<void> {
  if (!ensureCharacterReady() || !recordingPath) return;
  try {
    busy = true;
    renderVoicePanel();
    const saved = await invoke<SavedVoiceProfile>("voice_save_recording", {
      request: {
        tempPath: recordingPath,
        characterDirectory,
        characterName,
        sampleText,
        characterReferencePath: characterReferencePath.trim() || null,
      },
    });
    recordingReady = false;
    recordingPath = null;
    setNotice(null, `Saved ${saved.audioPath}`);
    host?.onStatus("Voice saved", saved.metadataPath);
    host?.onSaved?.();
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    renderVoicePanel();
  }
}

async function loadLibrary(more: boolean): Promise<void> {
  try {
    if (!more) {
      clearLibraryPreview();
    } else {
      // Keep selection, but never restart audio on "load more" re-render.
      libraryAutoplay = false;
    }
    busy = true;
    renderVoicePanel();
    const page = await invoke<{
      voices: LibraryVoice[];
      hasMore: boolean;
      nextPageToken?: string | null;
    }>("elevenlabs_list_voices", {
      search: librarySearch.trim() || null,
      voiceType: libraryFilter,
      pageSize: 40,
      nextPageToken: more ? libraryNextToken : null,
    });
    libraryVoices = more ? [...libraryVoices, ...page.voices] : page.voices;
    libraryHasMore = page.hasMore;
    libraryNextToken = page.nextPageToken ?? null;
    setNotice(null, `${libraryVoices.length} voice(s) loaded`);
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    libraryAutoplay = false;
    renderVoicePanel();
  }
}

async function authorizeVoiceSpend(operation: string, model: string, input: string, _characters: number): Promise<{ projectRoot: string; confirmation: string }> {
  const projectRoot = hostProjectRoot();
  if (!projectRoot) throw new Error("A saved project is required for paid voice requests.");
  type VoiceQuote = { policy: "approval_required" | "warn_only"; requestHash: string; ledgerRevision: string | null; disclosure: string };
  const request = { projectRoot, operation, model, input };
  const quote = await invoke<VoiceQuote>("elevenlabs_quote_voice_spend", request);
  const warning = `${quote.disclosure}\n\nThis ${operation} request has no verified USD maximum. The provider may charge more than the project budget. Continue?`;
  if (!window.confirm(warning)) throw new Error("Voice request cancelled; no provider request was sent.");
  if (quote.policy === "warn_only") return { projectRoot, confirmation: "" };
  if (quote.policy !== "approval_required" || !quote.ledgerRevision) throw new Error("Voice approval quote is incomplete.");
  const confirmation = await invoke<string>("elevenlabs_confirm_unbounded_voice_spend", {
    ...request, requestHash: quote.requestHash, ledgerRevision: quote.ledgerRevision,
  });
  return { projectRoot, confirmation };
}

async function previewLibraryVoice(voiceId: string): Promise<void> {
  const voice = libraryVoices.find((item) => item.voiceId === voiceId);
  if (!voice) return;
  clearLibraryPreview();
  selectedLibraryId = voiceId;
  try {
    busy = true;
    renderVoicePanel();
    let payload: { audioBase64: string; mediaType: string };
    if (voice.previewUrl) {
      payload = await invoke("elevenlabs_download_preview_url", { url: voice.previewUrl });
    } else {
      const text = sampleText;
      payload = await invoke("elevenlabs_tts_preview", {
        voiceId,
        text,
        ...await authorizeVoiceSpend("elevenlabs_tts_preview", "eleven_multilingual_v2", JSON.stringify({ text: text.trim(), voice_id: voiceId }), [...text].length),
      });
    }
    if (selectedLibraryId !== voiceId) {
      return;
    }
    libraryPreviewUrl = audioUrlFromBase64(payload.audioBase64, payload.mediaType);
    libraryAutoplay = true;
    setNotice(null, `Playing ${voice.name}`);
  } catch (error) {
    if (selectedLibraryId === voiceId) {
      setNotice(String(error));
      clearLibraryPreview();
    }
  } finally {
    if (selectedLibraryId === voiceId) {
      busy = false;
      renderVoicePanel();
      libraryAutoplay = false;
    }
  }
}

async function saveLibraryVoice(voiceId: string): Promise<void> {
  if (!ensureCharacterReady()) return;
  const voice = libraryVoices.find((item) => item.voiceId === voiceId);
  if (!voice) return;
  try {
    clearLibraryPreview({ keepSelection: true });
    selectedLibraryId = voiceId;
    busy = true;
    renderVoicePanel();
    const text = sampleText;
    const payload = await invoke<{ audioBase64: string; mediaType: string }>(
      "elevenlabs_tts_preview",
      { voiceId, text, ...await authorizeVoiceSpend("elevenlabs_tts_preview", "eleven_multilingual_v2", JSON.stringify({ text: text.trim(), voice_id: voiceId }), [...text].length) },
    );
    const saved = await invoke<SavedVoiceProfile>("voice_save_library_voice", {
      request: {
        characterDirectory,
        characterName,
        characterReferencePath: characterReferencePath.trim() || null,
        sampleText,
        voiceId: voice.voiceId,
        voiceName: voice.name,
        voiceDescription: voice.description ?? `ElevenLabs library voice: ${voice.name}`,
        gender: voice.gender ?? null,
        age: voice.age ?? null,
        accent: voice.accent ?? null,
        audioBase64: payload.audioBase64,
        mediaType: payload.mediaType,
      },
    });
    libraryPreviewUrl = audioUrlFromBase64(payload.audioBase64, payload.mediaType);
    libraryAutoplay = false;
    setNotice(null, `Saved ${saved.audioPath}`);
    host?.onStatus("Voice saved", saved.metadataPath);
    host?.onSaved?.();
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    libraryAutoplay = false;
    renderVoicePanel();
  }
}

async function generateDesign(): Promise<void> {
  if (!ensureCharacterReady()) return;
  if (sampleText.trim().length < 100) {
    setNotice("Sample script must be at least 100 characters for voice design.");
    renderVoicePanel();
    return;
  }
  try {
    clearDesignPreview();
    busy = true;
    renderVoicePanel();
    const description = buildDesignDescription();
    const effectiveText = [...sampleText.trim()].slice(0, 1000).join("");
    const result = await invoke<{
      previews: DesignPreview[];
      text: string;
      voiceDescription: string;
    }>("elevenlabs_design_voice", {
      voiceDescription: description,
      text: sampleText,
      ...await authorizeVoiceSpend("elevenlabs_design_voice", "eleven_multilingual_ttv_v2", JSON.stringify({ text: effectiveText, voice_description: description.trim() }), [...effectiveText].length),
    });
    designPreviews = result.previews;
    setNotice(null, `${designPreviews.length} candidate(s) ready — listen, then choose.`);
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    designAutoplay = false;
    renderVoicePanel();
  }
}

function previewDesign(generatedVoiceId: string): void {
  const preview = designPreviews.find((item) => item.generatedVoiceId === generatedVoiceId);
  if (!preview) return;
  clearDesignPreview();
  selectedDesignId = generatedVoiceId;
  designPreviewUrl = audioUrlFromBase64(preview.audioBase64, preview.mediaType);
  designAutoplay = true;
  setNotice(null, "Playing design candidate");
  renderVoicePanel();
  designAutoplay = false;
}

async function saveDesign(generatedVoiceId: string): Promise<void> {
  if (!ensureCharacterReady()) return;
  const preview = designPreviews.find((item) => item.generatedVoiceId === generatedVoiceId);
  if (!preview) return;
  try {
    clearDesignPreview({ keepSelection: true });
    selectedDesignId = generatedVoiceId;
    busy = true;
    renderVoicePanel();
    const description = buildDesignDescription();
    const { projectRoot, confirmation } = await authorizeVoiceSpend("elevenlabs_create_designed_voice", "text-to-voice/create", JSON.stringify({ generated_voice_id: preview.generatedVoiceId, voice_description: description.trim(), voice_name: characterName.trim() }), 0);
    const saved = await invoke<SavedVoiceProfile>("voice_save_designed_voice", {
      request: {
        characterDirectory,
        characterName,
        characterReferencePath: characterReferencePath.trim() || null,
        sampleText,
        voiceDescription: description,
        generatedVoiceId: preview.generatedVoiceId,
        audioBase64: preview.audioBase64,
        mediaType: preview.mediaType,
        designFields: {
          gender: designGender,
          age: designAge,
          pace: designPace,
          pitch: designPitch,
          texture: designTexture,
          energy: designEnergy,
          accent: designAccent,
          note: designNote,
        },
        projectRoot,
        confirmation,
      },
    });
    designPreviewUrl = audioUrlFromBase64(preview.audioBase64, preview.mediaType);
    designAutoplay = false;
    setNotice(null, `Saved ${saved.audioPath}`);
    host?.onStatus("Voice saved", saved.metadataPath);
    host?.onSaved?.();
  } catch (error) {
    setNotice(String(error));
  } finally {
    busy = false;
    designAutoplay = false;
    renderVoicePanel();
  }
}

async function refreshExistingAudio(): Promise<void> {
  const directory = characterDirectory.trim().replace(/\/$/, "");
  if (!directory) {
    existingAudio = [];
    renderVoicePanel();
    return;
  }
  try {
    const entries = await invoke<{ name: string; path: string; isDir: boolean }[]>("list_dir", {
      path: `${directory}/voice`,
    });
    existingAudio = entries
      .filter((entry) => !entry.isDir && AUDIO_FILE.test(entry.name) && !entry.name.startsWith("."))
      .sort((a, b) => a.name.localeCompare(b.name));
  } catch {
    existingAudio = [];
  }
  if (!selectedExistingPath && existingAudio.length === 1) {
    selectedExistingPath = existingAudio[0].path;
  }
  renderVoicePanel();
}

async function browseExistingAudio(): Promise<void> {
  const selected = await openDialog({
    multiple: false,
    defaultPath: characterDirectory ? `${characterDirectory.replace(/\/$/, "")}/voice` : hostProjectRoot() || undefined,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "m4a", "ogg", "flac", "aac"] }],
  });
  if (typeof selected !== "string") return;
  selectedExistingPath = selected;
  renderVoicePanel();
}

async function attachExisting(path: string): Promise<void> {
  if (!ensureCharacterReady()) return;
  try {
    busy = true;
    selectedExistingPath = path;
    renderVoicePanel();
    const saved = await invoke<SavedVoiceProfile>("voice_attach_existing", {
      request: {
        audioPath: path,
        characterDirectory,
        characterName,
        sampleText,
        characterReferencePath: characterReferencePath.trim() || null,
      },
    });
    setNotice(null, `Registered ${saved.audioPath}`);
    host?.onStatus("Voice saved", saved.metadataPath);
    host?.onSaved?.();
    busy = false;
    await refreshExistingAudio();
  } catch (error) {
    setNotice(String(error));
    busy = false;
    renderVoicePanel();
  }
}

export function openVoicePanelContext(projectRoot: string | null, selectedPath: string | null): void {
  clearAllPlayback();
  inferCharacterTarget(projectRoot, selectedPath);
  setNotice(null);
  selectedExistingPath = null;
  if (activeMode === "recorded") void loadRecordingSources();
  if (activeMode === "library" && libraryVoices.length === 0) void loadLibrary(false);
  void refreshExistingAudio();
}
