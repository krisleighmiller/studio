import { ask, invoke, openDialog, toMediaUrlAsync } from "./host";
import {
  LOOK_ROLES,
  MOUTH_POLICIES,
  SCENE_STRATEGIES,
  SHOT_ROLES,
  WORKFLOW_STAGES,
  type AssetRecord,
  type CharacterRecord,
  type CharacterProfile,
  type CutSketch,
  type CutRecord,
  type LookSlot,
  type MixCounts,
  type MouthPolicy,
  type ProjectReport,
  type ProjectState,
  type SceneIntent,
  type SceneRecord,
  type SceneReport,
  type ShotRole,
  type SpeechMode,
  type StageId,
  type StageStatus,
  type WorkflowImportResult,
} from "./workflowTypes";

export type WorkflowSelection = {
  sceneId: number | null;
  cut: number | null;
  path: string | null;
  excerpt: string | null;
  stage: StageId | null;
};

type DeskHooks = {
  root: HTMLElement;
  projectRoot: () => string | null;
  onOpenPath: (path: string) => void;
  onAddToChat: (path: string) => void;
  onOpenVoice: (characterDirectory: string) => void;
  onComposeChat: (text: string, attachment?: string | null) => void;
  onSelectionChange: (selection: WorkflowSelection) => void;
  chatModel: () => { apiId: string; modelId: string } | null;
  onStatus: (left: string, right?: string) => void;
  onPendingPlan?: (text: string) => void;
};

type WorkflowViewport = {
  sceneId: number | null;
  stage: StageId;
  mainTop: number;
  mainLeft: number;
  scenesTop: number;
  inspectorTop: number;
  jobsTop: number;
};

let hooks: DeskHooks | null = null;
let state: ProjectState | null = null;
let report: ProjectReport | null = null;
let loadError: string | null = null;
let selectedSceneId: number | null = null;
let selectedCut: number | null = null;
let selectedStage: StageId = "production_notes";
let jobsOpen = false;
let busy: string | null = null;
let actionNote: string | null = null;
let intentDraft: SceneIntent | null = null;
let intentDraftScene: number | null = null;
const startFrameEditBriefs = new Map<string, string>();
const characterDrafts = new Map<
  string,
  { name: string; profile: CharacterProfile }
>();
let newCharacterOpen = false;

export function bindWorkflowDesk(next: DeskHooks): void {
  hooks = next;
}

export function workflowSelection(): WorkflowSelection {
  const scene = selectedScene();
  const cut = selectedCutRecord();
  return {
    sceneId: selectedSceneId,
    cut: selectedCut,
    path: resolveProjectPath(cut?.production_note_path ?? scene?.source_path ?? null),
    excerpt: selectionExcerpt(scene, cut, sceneReport(selectedSceneId)),
    stage: selectedStage,
  };
}

export async function refreshWorkflowDesk(): Promise<void> {
  const root = hooks?.projectRoot();
  if (!root || !state) return;
  const result = await invoke<WorkflowImportResult>("workflow_status", {
    projectRoot: root,
    refresh: false,
  });
  const keepNote = actionNote;
  const previousIntentMissing = selectedScene()?.stages.scene_intent === "missing";
  applyHostResult(result);
  if (!result.note) actionNote = keepNote;
  const scene = selectedScene();
  if (
    previousIntentMissing &&
    scene?.intent &&
    !scene.intent_approved
  ) {
    selectedStage = "scene_intent";
  }
  renderWorkflowDesk();
  emitSelection();
}

export function workflowStatusLine(): string {
  if (loadError) return loadError;
  const first = report?.summary?.split("\n").find((line) => line.trim());
  return first ?? "";
}

export function clearWorkflowDesk(): void {
  state = null;
  report = null;
  loadError = null;
  selectedSceneId = null;
  selectedCut = null;
  selectedStage = "scene_intent";
  busy = null;
  actionNote = null;
  intentDraft = null;
  intentDraftScene = null;
  characterDrafts.clear();
  newCharacterOpen = false;
  if (hooks) hooks.root.replaceChildren();
}

export async function loadWorkflowDesk(projectRoot: string): Promise<void> {
  loadError = null;
  try {
    const result = await invoke<WorkflowImportResult>("workflow_import", {
      projectRoot,
      persist: true,
    });
    state = result.state;
    report = result.report;
    const first = result.state.scenes[0];
    selectedSceneId = first?.id ?? null;
    selectedCut = first?.cuts[0]?.id.cut ?? null;
    const scene = result.state.scenes.find((item) => item.id === selectedSceneId);
    selectedStage = defaultStage(scene);
    renderWorkflowDesk();
    emitSelection();
  } catch (error) {
    loadError = error instanceof Error ? error.message : String(error);
    state = null;
    report = null;
    renderWorkflowDesk();
  }
}

function applyHostResult(result: WorkflowImportResult): void {
  state = result.state;
  report = result.report;
  actionNote = result.note ?? null;
  intentDraft = null;
  intentDraftScene = null;
  if (selectedSceneId != null && !result.state.scenes.some((scene) => scene.id === selectedSceneId)) {
    selectedSceneId = result.state.scenes[0]?.id ?? null;
  }
  if (selectedCut != null) {
    const scene = result.state.scenes.find((item) => item.id === selectedSceneId);
    if (!scene?.cuts.some((cut) => cut.id.cut === selectedCut)) {
      selectedCut = scene?.cuts[0]?.id.cut ?? null;
    }
  }
  renderWorkflowDesk();
  emitSelection();
  hooks?.onStatus("Production", result.note || workflowStatusLine());
}

function requireChat(): { apiId: string; modelId: string } | null {
  const model = hooks?.chatModel() ?? null;
  if (!model?.modelId) {
    actionNote = "Pick a chat model in the dropdown first.";
    renderWorkflowDesk();
    hooks?.onStatus("Production", actionNote);
    return null;
  }
  return { apiId: model.apiId, modelId: model.modelId };
}

async function runAction(
  label: string,
  cmd: string,
  args: Record<string, unknown>,
): Promise<boolean> {
  const root = hooks?.projectRoot();
  if (!root) {
    actionNote = "Open a project first.";
    renderWorkflowDesk();
    return false;
  }
  busy = label;
  actionNote = null;
  renderWorkflowDesk();
  try {
    const result = await invoke<WorkflowImportResult>(cmd, { projectRoot: root, ...args });
    busy = null;
    applyHostResult(result);
    if (result.pendingAction) hooks?.onPendingPlan?.(result.pendingAction);
    return true;
  } catch (error) {
    busy = null;
    actionNote = error instanceof Error ? error.message : String(error);
    renderWorkflowDesk();
    hooks?.onStatus("Production", actionNote);
    return false;
  }
}

export function renderWorkflowDesk(): void {
  const root = hooks?.root;
  if (!root) return;
  const viewport = captureWorkflowViewport(root);
  root.replaceChildren();
  if (loadError) {
    root.append(block("workflow-error", `Could not import workflow state.\n${loadError}`));
    return;
  }
  if (!state || !report) {
    root.append(
      block(
        "empty-state center",
        "Open a project folder to see scenes, cuts, and missing preproduction artifacts.",
      ),
    );
    return;
  }

  const desk = document.createElement("div");
  desk.className = "workflow-desk";
  desk.dataset.sceneId = selectedSceneId?.toString() ?? "";
  desk.dataset.stage = selectedStage;
  if (busy) desk.classList.add("busy");
  const head = document.createElement("div");
  if (actionNote) head.append(block(noteClass(actionNote), actionNote));
  if (busy) head.append(block("workflow-banner", busy));
  head.append(renderStageRail());
  desk.append(head, renderBody(), renderJobs());
  root.append(desk);
  restoreWorkflowViewport(root, viewport);
}

function captureWorkflowViewport(root: HTMLElement): WorkflowViewport | null {
  const desk = root.querySelector<HTMLElement>(".workflow-desk");
  const main = root.querySelector<HTMLElement>(".workflow-main");
  if (!desk || !main) return null;
  const renderedSceneId = Number.parseInt(desk.dataset.sceneId ?? "", 10);
  return {
    sceneId: Number.isFinite(renderedSceneId) ? renderedSceneId : null,
    stage: (desk.dataset.stage as StageId | undefined) ?? selectedStage,
    mainTop: main.scrollTop,
    mainLeft: main.scrollLeft,
    scenesTop:
      root.querySelector<HTMLElement>(".workflow-scenes")?.scrollTop ?? 0,
    inspectorTop:
      root.querySelector<HTMLElement>(".workflow-inspector")?.scrollTop ?? 0,
    jobsTop:
      root.querySelector<HTMLElement>(".workflow-jobs-body")?.scrollTop ?? 0,
  };
}

function restoreWorkflowViewport(
  root: HTMLElement,
  viewport: WorkflowViewport | null,
): void {
  if (
    !viewport ||
    viewport.sceneId !== selectedSceneId ||
    viewport.stage !== selectedStage
  ) {
    return;
  }
  const main = root.querySelector<HTMLElement>(".workflow-main");
  if (main) {
    main.scrollTop = viewport.mainTop;
    main.scrollLeft = viewport.mainLeft;
  }
  const scenes = root.querySelector<HTMLElement>(".workflow-scenes");
  if (scenes) scenes.scrollTop = viewport.scenesTop;
  const inspector = root.querySelector<HTMLElement>(".workflow-inspector");
  if (inspector) inspector.scrollTop = viewport.inspectorTop;
  const jobs = root.querySelector<HTMLElement>(".workflow-jobs-body");
  if (jobs) jobs.scrollTop = viewport.jobsTop;
}

function noteClass(note: string): string {
  const lower = note.toLowerCase();
  if (lower.includes("reject")) return "workflow-banner reject";
  if (lower.includes("warning") || lower.includes("warn")) return "workflow-banner warn";
  return "workflow-banner";
}

function defaultStage(scene: SceneRecord | undefined): StageId {
  if (!scene) return "script";
  const intentMissing =
    scene.stages.scene_intent === "missing" || !scene.intent;
  if (intentMissing) return "scene_intent";
  if (!scene.intent_approved) return "scene_intent";
  const lookMissing =
    scene.stages.scene_look === "missing" ||
    !scene.look_board?.slots?.length ||
    scene.look_board.slots.every((slot) => !slot.approved_asset_id);
  if (lookMissing) return "scene_look";
  if (scene.stages.cut_sketches === "missing") return "cut_sketches";
  if (scene.stages.production_notes === "present") return "production_notes";
  if (scene.stages.script === "present") return "script";
  return "script";
}

function selectedScene(): SceneRecord | undefined {
  return state?.scenes.find((scene) => scene.id === selectedSceneId);
}

function selectedCutRecord(): CutRecord | undefined {
  return selectedScene()?.cuts.find((cut) => cut.id.cut === selectedCut);
}

function sceneReport(id: number | null): SceneReport | undefined {
  if (id == null) return undefined;
  return report?.scenes.find((scene) => scene.id === id);
}

function emitSelection(): void {
  hooks?.onSelectionChange(workflowSelection());
}

function resolveProjectPath(raw: string | null | undefined): string | null {
  if (!raw) return null;
  if (raw.startsWith("/") || /^[A-Za-z]:[\\/]/.test(raw)) return raw;
  const root = hooks?.projectRoot();
  if (!root) return raw;
  return `${root.replace(/\/+$/, "")}/${raw.replace(/^\/+/, "")}`;
}

function renderStageRail(): HTMLElement {
  const scene = selectedScene();
  const nav = document.createElement("nav");
  nav.className = "workflow-stage-rail";
  nav.setAttribute("aria-label", "Production stages");
  for (const stage of WORKFLOW_STAGES) {
    const status = scene?.stages[stage.id] ?? "missing";
    const button = document.createElement("button");
    button.type = "button";
    button.className = `workflow-stage${selectedStage === stage.id ? " active" : ""}`;
    button.dataset.status = status;
    button.innerHTML = `<span class="workflow-stage-dot" aria-hidden="true"></span><span class="workflow-stage-label"></span><span class="workflow-stage-status"></span>`;
    button.querySelector(".workflow-stage-label")!.textContent = stage.label;
    button.querySelector(".workflow-stage-status")!.textContent = statusLabel(status);
    button.addEventListener("click", () => {
      selectedStage = stage.id;
      renderWorkflowDesk();
    });
    nav.append(button);
  }
  return nav;
}

function renderBody(): HTMLElement {
  const body = document.createElement("div");
  body.className = "workflow-body";
  body.append(renderSceneList(), renderMain(), renderInspector());
  return body;
}

function renderSceneList(): HTMLElement {
  const aside = document.createElement("aside");
  aside.className = "workflow-scenes";
  aside.setAttribute("aria-label", "Scenes");
  const heading = document.createElement("h2");
  heading.textContent = "Scenes";
  aside.append(heading);
  if (report) {
    const meta = document.createElement("p");
    meta.className = "workflow-scenes-meta";
    meta.textContent = `${report.script_count} script · ${report.character_count} characters · ${report.scenes.length} scenes`;
    aside.append(meta);
  }
  for (const scene of state?.scenes ?? []) {
    const sceneStatus = sceneReport(scene.id);
    const button = document.createElement("button");
    button.type = "button";
    button.className = `workflow-scene${scene.id === selectedSceneId ? " active" : ""}`;
    const mix = sceneStatus ? mixLabel(sceneStatus.mix) : `${scene.cuts.length} cuts`;
    button.innerHTML = `<span class="workflow-scene-title"></span><span class="workflow-scene-mix"></span>`;
    button.querySelector(".workflow-scene-title")!.textContent = scene.title;
    button.querySelector(".workflow-scene-mix")!.textContent = mix;
    if (sceneStatus?.effectively_performance_heavy) {
      button.classList.add("heavy");
    }
    button.addEventListener("click", () => {
      selectedSceneId = scene.id;
      selectedCut = scene.cuts[0]?.id.cut ?? null;
      selectedStage = defaultStage(scene);
      renderWorkflowDesk();
      emitSelection();
    });
    aside.append(button);
  }
  return aside;
}

function renderMain(): HTMLElement {
  const main = document.createElement("section");
  main.className = "workflow-main";
  main.setAttribute("aria-label", "Stage");
  const scene = selectedScene();
  const sceneStatus = sceneReport(selectedSceneId);
  const title = document.createElement("header");
  title.className = "workflow-main-header";
  const heading = document.createElement("h1");
  heading.textContent = WORKFLOW_STAGES.find((stage) => stage.id === selectedStage)?.label ?? selectedStage;
  const sub = document.createElement("p");
  sub.textContent = scene
    ? sceneStatus
      ? `${scene.title} · ${mixLabel(sceneStatus.mix)} · ${sceneStatus.approved_start_frames} start frames`
      : `${scene.title} · ${scene.cuts.length} cuts`
    : "No scene selected";
  title.append(heading, sub);
  main.append(title);

  if (sceneStatus?.effectively_performance_heavy && selectedStage === "production_notes") {
    main.append(
      block(
        "workflow-banner warn",
        "This mix is effectively performance-heavy for a cinematic-balanced request.",
      ),
    );
  }

  main.append(renderStageContent(scene, sceneStatus));
  return main;
}

function renderStageContent(scene: SceneRecord | undefined, sceneStatus: SceneReport | undefined): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-stage-content";
  if (!scene) {
    wrap.append(block("empty-state", "Select a scene."));
    return wrap;
  }
  switch (selectedStage) {
    case "script":
      wrap.append(renderScriptStage(scene));
      break;
    case "characters":
      wrap.append(renderCharactersStage());
      break;
    case "scene_intent":
      wrap.append(renderIntentStage(scene));
      break;
    case "scene_look":
      wrap.append(renderLookStage(scene));
      break;
    case "cut_sketches":
      wrap.append(renderSketchStage(scene, sceneStatus));
      break;
    case "reference_images":
      wrap.append(renderReferenceImagesStage(scene, sceneStatus));
      break;
    case "production_notes":
      wrap.append(renderReadiness(scene, sceneStatus));
      if (!scene.cuts.length) {
        wrap.append(missingPanel("No production notes for this scene.", "Run scene plan or import a project that has Scene_Prompts."));
      } else {
        wrap.append(renderCutCards(scene, true));
      }
      break;
    case "render":
      wrap.append(missingPanel("No renders recorded.", "Provider jobs will appear here after a cut is sent."));
      break;
    case "take_review":
      wrap.append(missingPanel("No takes to review.", "Selected / alternate / rejected statuses land here after generation."));
      break;
    case "assemble":
      wrap.append(missingPanel("Assemble is not planned yet.", "Host-owned assemble from selected takes comes later."));
      break;
  }
  return wrap;
}

function emptyIntent(): SceneIntent {
  return {
    narrative_purpose: "",
    emotional_purpose: "",
    character_focus: [],
    visual_contrast: "",
    important_props: [],
    continuity_constraints: [],
    sound_and_transition: [],
    audience_must_know: "",
  };
}

function intentWorkingCopy(scene: SceneRecord): SceneIntent {
  if (intentDraftScene !== scene.id || !intentDraft) {
    intentDraftScene = scene.id;
    intentDraft = { ...emptyIntent(), ...scene.intent };
  }
  return intentDraft;
}

function renderIntentStage(scene: SceneRecord): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-intent";
  const copy = intentWorkingCopy(scene);
  const approved = Boolean(scene.intent_approved);
  wrap.append(
    block(
      approved ? "workflow-banner" : "workflow-missing",
      approved
        ? "Intent is approved. You can still edit and save a new draft."
        : scene.intent
          ? "Intent is a draft. Approve it before relying on it for sketches."
          : "No scene intent yet. Draft from the script, then edit and approve.",
    ),
  );
  const fields: { key: keyof SceneIntent; label: string; list?: boolean }[] = [
    { key: "narrative_purpose", label: "Narrative purpose" },
    { key: "emotional_purpose", label: "Emotional purpose" },
    { key: "character_focus", label: "Character focus", list: true },
    { key: "visual_contrast", label: "Visual contrast" },
    { key: "important_props", label: "Important props", list: true },
    { key: "continuity_constraints", label: "Continuity constraints", list: true },
    { key: "sound_and_transition", label: "Sound and transition", list: true },
    { key: "audience_must_know", label: "Audience must know" },
  ];
  for (const field of fields) {
    const label = document.createElement("label");
    label.className = "workflow-field";
    const title = document.createElement("span");
    title.textContent = field.label;
    const input = document.createElement("textarea");
    input.rows = field.list ? 2 : 3;
    const value = copy[field.key];
    input.value = Array.isArray(value) ? value.join(", ") : (value ?? "");
    input.addEventListener("input", () => {
      if (!intentDraft) return;
      if (field.list) {
        (intentDraft[field.key] as string[]) = input.value
          .split(/[,;\n]/)
          .map((item) => item.trim())
          .filter(Boolean);
      } else {
        (intentDraft[field.key] as string) = input.value;
      }
    });
    label.append(title, input);
    wrap.append(label);
  }
  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Draft from script", () => {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting scene intent…", "workflow_draft_intent", {
        sceneId: scene.id,
        ...chat,
      });
    }),
    actionButton("Save draft", () => {
      void runAction("Saving scene intent…", "workflow_save_intent", {
        sceneId: scene.id,
        intent: intentWorkingCopy(scene),
      });
    }),
    actionButton("Approve intent", () => {
      void runAction("Approving scene intent…", "workflow_approve_intent", {
        sceneId: scene.id,
      });
    }),
  );
  wrap.append(actions);
  return wrap;
}

function renderLookStage(scene: SceneRecord): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-look";
  const board = scene.look_board;
  const slots = board?.slots ?? [];
  const approved = slots.filter((slot) => slot.approved_asset_id).length;
  wrap.append(
    block(
      approved ? "workflow-banner" : "workflow-missing",
      approved
        ? `${approved} approved scene look still(s). Character model-sheet closeups stay under Characters/.`
        : "Scene look is location, wardrobe, prop, mood, and lighting stills — not talking-head identity refs. Draft prompts, attach or generate, then approve.",
    ),
  );
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-sketch-toolbar";
  toolbar.append(
    actionButton("Draft look board", () => {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting scene look…", "workflow_draft_look", {
        sceneId: scene.id,
        ...chat,
      });
    }),
  );
  wrap.append(toolbar);
  if (board?.notes?.length) {
    wrap.append(block("workflow-banner", board.notes.join(" · ")));
  }
  if (!slots.length) {
    wrap.append(
      missingPanel(
        "No look slots yet.",
        "Draft from scene intent, or wait until intent is approved if you want the model to use it.",
      ),
    );
    return wrap;
  }
  for (const slot of slots) {
    wrap.append(renderLookSlot(scene, slot));
  }
  queueLookThumbs(wrap);
  return wrap;
}

function renderLookSlot(scene: SceneRecord, slot: LookSlot): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-look-slot";
  const head = document.createElement("header");
  const roleLabel = document.createElement("label");
  roleLabel.className = "workflow-field inline";
  const roleTitle = document.createElement("span");
  roleTitle.textContent = "Role";
  const roleSelect = document.createElement("select");
  for (const option of LOOK_ROLES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    roleSelect.append(item);
  }
  roleSelect.value = LOOK_ROLES.some((item) => item.id === slot.role)
    ? slot.role
    : "location_reference";
  roleLabel.append(roleTitle, roleSelect);
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = slot.approved_asset_id
    ? "Approved"
    : (slot.asset_ids?.length ?? 0) > 0
      ? "Draft still"
      : "Prompt only";
  head.append(roleLabel, status);
  card.append(head);

  const titleLabel = document.createElement("label");
  titleLabel.className = "workflow-field";
  const titleSpan = document.createElement("span");
  titleSpan.textContent = "Title";
  const titleInput = document.createElement("input");
  titleInput.type = "text";
  titleInput.value = slot.title ?? "";
  titleLabel.append(titleSpan, titleInput);

  const promptLabel = document.createElement("label");
  promptLabel.className = "workflow-field";
  const promptSpan = document.createElement("span");
  promptSpan.textContent = "Prompt";
  const promptInput = document.createElement("textarea");
  promptInput.rows = 3;
  promptInput.value = slot.prompt ?? "";
  promptLabel.append(promptSpan, promptInput);

  const negLabel = document.createElement("label");
  negLabel.className = "workflow-field";
  const negSpan = document.createElement("span");
  negSpan.textContent = "Negative";
  const negInput = document.createElement("textarea");
  negInput.rows = 2;
  negInput.value = slot.negative_prompt ?? "";
  negLabel.append(negSpan, negInput);
  card.append(titleLabel, promptLabel, negLabel);
  const promptActions = document.createElement("div");
  promptActions.className = "workflow-inspector-actions";
  promptActions.append(
    actionButton("Save prompt", () => {
      void runAction("Saving reference prompt…", "workflow_update_look_slot", {
        sceneId: scene.id,
        slotId: slot.id,
        role: roleSelect.value,
        title: titleInput.value,
        prompt: promptInput.value,
        negativePrompt: negInput.value,
      });
    }),
  );
  card.append(promptActions);

  const identityHint = lookIdentityHint(scene, slot);
  if (identityHint) {
    const hint = document.createElement("p");
    hint.className = "workflow-look-identity";
    hint.textContent = identityHint;
    card.append(hint);
  }

  const versions = document.createElement("div");
  versions.className = "workflow-look-versions";
  const ids = slot.asset_ids ?? [];
  if (!ids.length) {
    versions.append(block("empty-state", "No stills yet. Attach an image or generate one."));
  } else {
    for (const assetId of ids) {
      const asset = findAsset(assetId);
      if (!asset) continue;
      const fig = document.createElement("figure");
      fig.className = `workflow-look-thumb${
        slot.approved_asset_id === assetId ? " approved" : ""
      }${asset?.status === "rejected" ? " rejected" : ""}`;
      const img = document.createElement("img");
      img.alt = slot.title || slot.role;
      img.dataset.lookPath = resolveProjectPath(asset?.path ?? null) ?? "";
      const cap = document.createElement("figcaption");
      cap.textContent = `${asset?.status ?? "draft"}${
        slot.approved_asset_id === assetId ? " · current" : ""
      }`;
      const versionActions = document.createElement("div");
      versionActions.className = "workflow-look-thumb-actions";
      versionActions.append(
        actionButton("Use", () => {
          void runAction("Approving look still…", "workflow_approve_look", {
            sceneId: scene.id,
            slotId: slot.id,
            assetId,
          });
        }),
        actionButton("Reject", () => {
          void runAction("Rejecting look still…", "workflow_reject_look", {
            sceneId: scene.id,
            slotId: slot.id,
            assetId,
          });
        }),
        actionButton("Delete", () => {
          void deleteLookImage(scene, slot, asset);
        }),
      );
      fig.append(img, cap, versionActions);
      img.addEventListener("click", () => {
        if (asset.path) hooks?.onOpenPath(resolveProjectPath(asset.path) ?? asset.path);
      });
      versions.append(fig);
    }
  }
  card.append(versions);

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Attach image", () => {
      void attachLookImage(scene.id, slot.id);
    }),
    actionButton("Generate still", () => {
      void runAction("Preparing look generate…", "workflow_generate_look", {
        sceneId: scene.id,
        slotId: slot.id,
        role: roleSelect.value,
        title: titleInput.value,
        prompt: promptInput.value,
        negativePrompt: negInput.value,
      });
    }),
  );
  card.append(actions);
  return card;
}

const LOOK_PEOPLE_ROLES = new Set([
  "interior_reference",
  "group_reference",
  "wardrobe_reference",
  "location_reference",
  "composition_reference",
]);

function lookIdentityHint(scene: SceneRecord, slot: LookSlot): string | null {
  const characters = state?.characters ?? [];
  if (!characters.length) return null;
  const hay = [
    slot.title ?? "",
    slot.prompt ?? "",
    LOOK_PEOPLE_ROLES.has(slot.role) ? scene.title : "",
    LOOK_PEOPLE_ROLES.has(slot.role) ? (scene.intent?.character_focus ?? []).join(" ") : "",
  ]
    .join("\n")
    .toLowerCase();
  const hits = characters
    .map((character) => character.name)
    .filter((name) => {
      const lower = name.toLowerCase();
      if (hay.includes(lower)) return true;
      const first = lower.split(/\s+/).find((part) => part.length >= 3);
      if (!first) return false;
      const unique =
        characters.filter((other) => other.name.toLowerCase().split(/\s+/)[0] === first).length ===
        1;
      return unique && new RegExp(`\\b${first.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`).test(hay);
    });
  if (!hits.length) return null;
  const wardrobe = scene.look_board?.slots?.find(
    (item) => item.role === "wardrobe_reference" && item.approved_asset_id,
  );
  if (wardrobe) {
    return `Generate will attach model sheets for identity and “${wardrobe.title || "Wardrobe"}” for clothing continuity: ${hits.join(", ")}`;
  }
  if (slot.role === "wardrobe_reference") {
    return `Generate will attach model sheets for identity. Approve the chosen wardrobe still to lock these clothes for later people images: ${hits.join(", ")}`;
  }
  return `Generate will attach model sheets for identity, but no wardrobe is approved; clothing may reset: ${hits.join(", ")}`;
}

async function deleteLookImage(
  scene: SceneRecord,
  slot: LookSlot,
  asset: AssetRecord,
): Promise<void> {
  const confirmed = await ask(
    `Delete ${asset.path}?\n\nThis removes the image file and its thumbnail. Provider job history is retained.`,
    { title: "Delete scene look image", kind: "warning" },
  );
  if (!confirmed) return;
  await runAction("Deleting look still…", "workflow_delete_look", {
    sceneId: scene.id,
    slotId: slot.id,
    assetId: asset.id,
  });
}

async function attachLookImage(sceneId: number, slotId: string): Promise<void> {
  const selected = await openDialog({
    title: "Attach scene look still",
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  const path = Array.isArray(selected) ? selected[0] : selected;
  if (!path) return;
  await runAction("Attaching look still…", "workflow_attach_look", {
    sceneId,
    slotId,
    sourcePath: path,
  });
}

function findAsset(id: string): AssetRecord | undefined {
  return state?.assets?.find((asset) => asset.id === id);
}

function queueLookThumbs(root: HTMLElement): void {
  root.querySelectorAll<HTMLImageElement>("img[data-look-path]").forEach((img) => {
    const path = img.dataset.lookPath;
    if (!path) return;
    void toMediaUrlAsync(path)
      .then((url) => {
        img.src = url;
      })
      .catch(() => {
        img.alt = "Could not load still";
      });
  });
}

const CUT_IMAGE_ROLES = new Set([
  "cut_start_frame",
  "performance_source",
  "reaction_plate",
  "insert_plate",
  "continuity_frame",
  "last_frame",
  "edit_source",
]);

function renderReferenceImagesStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-start-frames";
  const approved = sceneStatus?.approved_start_frames ?? 0;
  wrap.append(
    block(
      approved ? "workflow-banner" : "workflow-missing",
      approved
        ? `${approved} of ${scene.cuts.length} cuts have an approved start frame. Approved cut images become video sources; model sheets remain identity references.`
        : "No approved cut start frames. Draft or edit a still prompt, generate or attach versions, then choose Use on the production-ready image.",
    ),
  );
  if (!scene.cuts.length) {
    wrap.append(
      missingPanel(
        "No cuts are available.",
        "Draft cut sketches before generating cut-specific start frames.",
      ),
    );
    return wrap;
  }
  for (const cut of scene.cuts) {
    wrap.append(renderStartFrameCard(scene, cut));
  }
  queueLookThumbs(wrap);
  return wrap;
}

function renderStartFrameCard(scene: SceneRecord, cut: CutRecord): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-start-frame-card";
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = cut.approved_start_frame ? "Approved source" : "Needs approval";
  header.append(heading, status);
  card.append(header);

  const promptLabel = document.createElement("label");
  promptLabel.className = "workflow-field";
  const promptTitle = document.createElement("span");
  promptTitle.textContent = "Start-frame prompt";
  const prompt = document.createElement("textarea");
  prompt.rows = 4;
  prompt.value = cut.start_frame_prompt ?? cut.sketch?.visual_prompt ?? cut.prompt ?? "";
  promptLabel.append(promptTitle, prompt);
  const negativeLabel = document.createElement("label");
  negativeLabel.className = "workflow-field";
  const negativeTitle = document.createElement("span");
  negativeTitle.textContent = "Negative prompt";
  const negative = document.createElement("textarea");
  negative.rows = 2;
  negative.value =
    cut.start_frame_negative_prompt ??
    cut.sketch?.negative_prompt ??
    cut.negative_prompt ??
    "";
  negativeLabel.append(negativeTitle, negative);
  card.append(promptLabel, negativeLabel);

  const promptActions = document.createElement("div");
  promptActions.className = "workflow-inspector-actions";
  promptActions.append(
    actionButton("Draft image prompt", () => {
      const chat = requireChat();
      if (!chat) return;
      void runAction(
        "Drafting start-frame prompt…",
        "workflow_draft_start_frame_prompt",
        {
          sceneId: scene.id,
          cut: cut.id.cut,
          ...chat,
        },
      );
    }),
    actionButton("Save prompt", () => {
      void runAction("Saving start-frame prompt…", "workflow_save_start_frame_prompt", {
        sceneId: scene.id,
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    }),
    actionButton("Generate start frame", () => {
      void runAction("Preparing start-frame generate…", "workflow_generate_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
      });
    }),
    actionButton("Attach still", () => {
      void attachStartFrame(scene.id, cut.id.cut);
    }),
  );
  card.append(promptActions);

  const assets = cutImageAssets(scene.id, cut.id.cut);
  const versions = document.createElement("div");
  versions.className = "workflow-start-frame-versions";
  if (!assets.length) {
    versions.append(
      block(
        "empty-state",
        "No versions yet. Generated and attached images are saved under this cut’s start_frames folder.",
      ),
    );
  } else {
    for (const asset of assets) {
      versions.append(renderStartFrameVersion(scene, cut, asset));
    }
  }
  card.append(versions);

  const editKey = `${scene.id}.${cut.id.cut}`;
  const editLabel = document.createElement("label");
  editLabel.className = "workflow-field workflow-start-frame-edit";
  const editTitle = document.createElement("span");
  editTitle.textContent = "Edit brief";
  const editBrief = document.createElement("textarea");
  editBrief.rows = 2;
  editBrief.placeholder =
    "Describe only the change; the selected source version and approved references are preserved.";
  editBrief.value = startFrameEditBriefs.get(editKey) ?? "";
  editBrief.addEventListener("input", () => {
    startFrameEditBriefs.set(editKey, editBrief.value);
  });
  editLabel.append(editTitle, editBrief);
  card.append(editLabel);
  return card;
}

function renderStartFrameVersion(
  scene: SceneRecord,
  cut: CutRecord,
  asset: AssetRecord,
): HTMLElement {
  const figure = document.createElement("figure");
  figure.className = `workflow-start-frame-version${
    cut.approved_start_frame === asset.path ? " approved" : ""
  }${asset.status === "rejected" ? " rejected" : ""}`;
  const image = document.createElement("img");
  image.alt = `Scene ${scene.id} Cut ${cut.id.cut} start-frame version`;
  image.dataset.lookPath = resolveProjectPath(asset.path) ?? "";
  image.addEventListener("click", () => {
    hooks?.onOpenPath(resolveProjectPath(asset.path) ?? asset.path);
  });
  const caption = document.createElement("figcaption");
  caption.textContent = `${asset.path.split("/").pop() ?? asset.id} · ${
    cut.approved_start_frame === asset.path ? "approved source" : (asset.status ?? "draft")
  }`;
  const actions = document.createElement("div");
  actions.className = "workflow-start-frame-version-actions";
  actions.append(
    actionButton("Use", () => {
      void runAction("Approving cut start frame…", "workflow_approve_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
        assetId: asset.id,
      });
    }),
    actionButton("Reject", () => {
      void runAction("Rejecting cut start frame…", "workflow_reject_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
        assetId: asset.id,
      });
    }),
    actionButton("Edit", () => {
      const brief = startFrameEditBriefs.get(`${scene.id}.${cut.id.cut}`)?.trim() ?? "";
      if (!brief) {
        actionNote = `Enter an edit brief under Cut ${cut.id.cut} first.`;
        renderWorkflowDesk();
        return;
      }
      void runAction("Preparing start-frame edit…", "workflow_edit_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
        assetId: asset.id,
        brief,
      });
    }),
    actionButton("Delete", () => {
      void deleteStartFrame(scene, cut, asset);
    }),
  );
  figure.append(image, caption, actions);
  return figure;
}

function cutImageAssets(sceneId: number, cutNumber: number): AssetRecord[] {
  return (state?.assets ?? []).filter(
    (asset) =>
      CUT_IMAGE_ROLES.has(asset.role) &&
      asset.cut_id?.source_scene === sceneId &&
      asset.cut_id?.cut === cutNumber,
  );
}

async function attachStartFrame(sceneId: number, cutNumber: number): Promise<void> {
  const selected = await openDialog({
    title: `Attach start frame · Scene ${sceneId} Cut ${cutNumber}`,
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  const path = Array.isArray(selected) ? selected[0] : selected;
  if (!path) return;
  await runAction("Attaching cut start frame…", "workflow_attach_start_frame", {
    sceneId,
    cut: cutNumber,
    sourcePath: path,
  });
}

async function deleteStartFrame(
  scene: SceneRecord,
  cut: CutRecord,
  asset: AssetRecord,
): Promise<void> {
  const confirmed = await ask(
    `Delete ${asset.path}?\n\nThis removes this exact cut-image version and its registry entry. Provider job history is retained.`,
    { title: "Delete cut image", kind: "warning" },
  );
  if (!confirmed) return;
  await runAction("Deleting cut image…", "workflow_delete_start_frame", {
    sceneId: scene.id,
    cut: cut.id.cut,
    assetId: asset.id,
  });
}

function renderSketchStage(scene: SceneRecord, sceneStatus: SceneReport | undefined): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-sketches";
  const verdict = sceneStatus?.mix_verdict;
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-sketch-toolbar";
  const strategyLabel = document.createElement("label");
  strategyLabel.className = "workflow-field inline";
  const strategyTitle = document.createElement("span");
  strategyTitle.textContent = "Strategy";
  const strategySelect = document.createElement("select");
  for (const option of SCENE_STRATEGIES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    strategySelect.append(item);
  }
  strategySelect.value = scene.strategy ?? sceneStatus?.strategy ?? "cinematic_balanced";
  strategySelect.addEventListener("change", () => {
    void runAction("Setting strategy…", "workflow_set_strategy", {
      sceneId: scene.id,
      strategy: strategySelect.value,
      override: Boolean(scene.strategy_override),
    });
  });
  strategyLabel.append(strategyTitle, strategySelect);
  const overrideLabel = document.createElement("label");
  overrideLabel.className = "workflow-check";
  const override = document.createElement("input");
  override.type = "checkbox";
  override.checked = Boolean(scene.strategy_override);
  override.addEventListener("change", () => {
    void runAction("Updating mix override…", "workflow_set_strategy", {
      sceneId: scene.id,
      strategy: strategySelect.value,
      override: override.checked,
    });
  });
  overrideLabel.append(override, document.createTextNode(" Override mix rejection"));
  toolbar.append(strategyLabel, overrideLabel);
  wrap.append(toolbar);

  if (verdict && (verdict.mix.performance + verdict.mix.other_dialogue + verdict.mix.silent) > 0) {
    const status = verdict.status;
    wrap.append(
      block(
        status === "reject"
          ? "workflow-banner reject"
          : status === "warn"
            ? "workflow-banner warn"
            : "workflow-banner",
        `${verdict.strategy.replace(/_/g, " ")} · ${verdict.mix.performance} performance / ${verdict.mix.other_dialogue} other dialogue / ${verdict.mix.silent} silent${
          verdict.messages.length ? `\n${verdict.messages.join("\n")}` : ""
        }`,
      ),
    );
  } else if (sceneStatus?.can_backfill_sketches) {
    wrap.append(
      missingPanel(
        "Cut sketches are missing.",
        "Backfill from production notes, then change shot roles so a cinematic-balanced mix is not all lip-sync. Or draft sketches from scene intent.",
      ),
    );
  } else if (!scene.intent) {
    wrap.append(
      missingPanel(
        "Scene intent is missing.",
        "Draft and approve scene intent first. Sketches need that planning layer.",
      ),
    );
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  if (sceneStatus?.can_backfill_sketches || scene.cuts.some((cut) => cut.production_note_path)) {
    actions.append(
      actionButton("Backfill from notes", () => {
        void runAction("Backfilling cut sketches…", "workflow_backfill_sketches", {
          sceneId: scene.id,
          replace: true,
        });
      }),
    );
  }
  actions.append(
    actionButton("Draft sketches", () => {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting cut sketches…", "workflow_draft_sketches", {
        sceneId: scene.id,
        strategy: strategySelect.value,
        ...chat,
      });
    }),
  );
  wrap.append(actions);
  if (scene.cuts.length) wrap.append(renderSketchCards(scene));
  return wrap;
}

function renderSketchCards(scene: SceneRecord): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-sketch-cards";
  for (const cut of scene.cuts) {
    list.append(renderSketchCard(scene, cut));
  }
  return list;
}

function renderSketchCard(scene: SceneRecord, cut: CutRecord): HTMLElement {
  const sketch = editableSketch(cut);
  const card = document.createElement("article");
  card.className = `workflow-sketch-card${cut.id.cut === selectedCut ? " active" : ""}`;
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${sketch.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = cut.sketch ? speechLabel(cut.speech_mode) : "Imported fallback";
  header.append(heading, status);
  card.append(header);

  const fields = document.createElement("div");
  fields.className = "workflow-sketch-field-grid";
  const title = sketchTextField(fields, "Title", sketch.title ?? "");
  const role = sketchSelectField(
    fields,
    "Shot role",
    sketch.shot_role ?? "",
    SHOT_ROLES,
  );
  const subject = sketchTextField(fields, "Subject", sketch.subject ?? "");
  const speaker = sketchTextField(fields, "Speaker", sketch.speaker ?? "");
  const mouth = sketchSelectField(
    fields,
    "Mouth policy",
    sketch.mouth_policy ?? "",
    MOUTH_POLICIES,
  );
  const pace = sketchSelectField(fields, "Pace", sketch.pace ?? "", [
    { id: "tight", label: "Tight" },
    { id: "natural", label: "Natural" },
    { id: "held", label: "Held" },
  ]);
  const onCamera = sketchTextField(
    fields,
    "On camera",
    (sketch.on_camera ?? []).join(", "),
  );
  card.append(fields);

  const prompt = sketchTextarea(
    card,
    "Visual prompt",
    sketch.visual_prompt ?? "",
    5,
  );
  const negative = sketchTextarea(
    card,
    "Negative prompt",
    sketch.negative_prompt ?? "",
    2,
  );

  const refs = document.createElement("fieldset");
  refs.className = "workflow-sketch-references";
  const refsTitle = document.createElement("legend");
  refsTitle.textContent = "Scene look references";
  refs.append(refsTitle);
  const checkedRefs = new Set(sketch.reference_roles ?? []);
  const refChecks = LOOK_ROLES.map((option) => {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.value = option.id;
    input.checked = checkedRefs.has(option.id);
    label.append(input, document.createTextNode(option.label));
    refs.append(label);
    return input;
  });
  card.append(refs);

  const transition = sketchTextarea(
    card,
    "Transition idea",
    sketch.transition_idea ?? "",
    2,
  );
  const dialogue = sketchTextarea(
    card,
    "Dialogue coverage",
    formatSketchDialogue(sketch),
    3,
  );
  dialogue.placeholder = "Speaker | heard_as | dialogue text";
  const dialogueHint = document.createElement("p");
  dialogueHint.className = "workflow-field-hint";
  dialogueHint.textContent =
    "One line per beat: Speaker | on_camera, voice_over, lip_sync, or off_camera | dialogue text";
  card.append(dialogueHint);

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Save sketch", () => {
      selectedCut = cut.id.cut;
      void runAction("Saving cut sketch…", "workflow_save_sketch", {
        sceneId: scene.id,
        cut: cut.id.cut,
        sketch: {
          title: nullable(title.value),
          shot_role: nullable(role.value),
          subject: nullable(subject.value),
          speaker: nullable(speaker.value),
          mouth_policy: nullable(mouth.value),
          pace: nullable(pace.value),
          visual_prompt: nullable(prompt.value),
          negative_prompt: nullable(negative.value),
          reference_roles: refChecks
            .filter((input) => input.checked)
            .map((input) => input.value),
          transition_idea: nullable(transition.value),
          dialogue: parseSketchDialogue(dialogue.value),
          on_camera: splitList(onCamera.value),
        },
      });
    }),
  );
  card.append(actions);
  return card;
}

function editableSketch(cut: CutRecord): CutSketch {
  if (cut.sketch) return cut.sketch;
  const hasDialogue = Boolean(cut.dialogue?.length);
  const lipsync = cut.speech_mode === "source_video_lipsync";
  const voiceOver = cut.speech_mode === "voice_over";
  return {
    title: cut.title,
    shot_role: lipsync ? "performance" : voiceOver ? "reaction" : hasDialogue ? "atmosphere" : "insert",
    subject: cut.subject,
    speaker: cut.dialogue?.[0]?.speaker ?? null,
    mouth_policy: lipsync
      ? "speaking_visible"
      : voiceOver
        ? "off_camera"
        : hasDialogue
          ? "closed_or_not_visible"
          : "no_people",
    pace: null,
    visual_prompt: cut.prompt,
    negative_prompt: cut.negative_prompt,
    reference_roles: [],
    transition_idea: null,
    dialogue: (cut.dialogue ?? []).map((line) => ({
      ...line,
      heard_as: lipsync ? "lip_sync" : hasDialogue ? "voice_over" : null,
    })),
    on_camera: [],
  };
}

function sketchTextField(
  parent: HTMLElement,
  title: string,
  value: string,
): HTMLInputElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  label.append(heading, input);
  parent.append(label);
  return input;
}

function sketchSelectField(
  parent: HTMLElement,
  title: string,
  value: string,
  options: { id: string; label: string }[],
): HTMLSelectElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const select = document.createElement("select");
  const blank = document.createElement("option");
  blank.value = "";
  blank.textContent = "—";
  select.append(blank);
  for (const option of options) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    select.append(item);
  }
  select.value = value;
  label.append(heading, select);
  parent.append(label);
  return select;
}

function sketchTextarea(
  parent: HTMLElement,
  title: string,
  value: string,
  rows: number,
): HTMLTextAreaElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const textarea = document.createElement("textarea");
  textarea.rows = rows;
  textarea.value = value;
  label.append(heading, textarea);
  parent.append(label);
  return textarea;
}

function nullable(value: string): string | null {
  const trimmed = value.trim();
  return trimmed || null;
}

function splitList(value: string): string[] {
  return value
    .split(/[,;\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function formatSketchDialogue(sketch: CutSketch): string {
  return (sketch.dialogue ?? [])
    .map(
      (line) =>
        `${line.speaker ?? ""} | ${line.heard_as ?? ""} | ${line.text ?? ""}`,
    )
    .join("\n");
}

function parseSketchDialogue(
  value: string,
): { speaker: string; heard_as?: string | null; text: string }[] {
  return value
    .split("\n")
    .map((line) => {
      const parts = line.split("|");
      if (parts.length >= 3) {
        const speaker = parts.shift()?.trim() ?? "";
        const heardAs = parts.shift()?.trim() ?? "";
        return {
          speaker,
          heard_as: heardAs || null,
          text: parts.join("|").trim(),
        };
      }
      if (parts.length === 2) {
        return {
          speaker: parts[0].trim(),
          heard_as: null,
          text: parts[1].trim(),
        };
      }
      return { speaker: "", heard_as: null, text: line.trim() };
    })
    .filter((line) => line.text);
}

function renderScriptStage(scene: SceneRecord): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-script";
  const bits: [string, string][] = [];
  if (scene.location) bits.push(["Location", scene.location]);
  for (const line of scene.action ?? []) bits.push(["Action", line]);
  for (const line of scene.sound ?? []) bits.push(["Sound", line]);
  for (const line of scene.dialogue ?? []) bits.push([line.speaker, line.text]);
  for (const line of scene.transitions ?? []) bits.push(["Transition", line]);
  if (bits.length === 0 && scene.source_excerpt) {
    const pre = document.createElement("pre");
    pre.className = "workflow-excerpt";
    pre.textContent = scene.source_excerpt;
    card.append(pre);
    return card;
  }
  if (bits.length === 0) {
    card.append(block("empty-state", "No script beats found for this scene."));
    return card;
  }
  for (const [label, text] of bits) {
    const row = document.createElement("p");
    row.innerHTML = `<strong></strong> <span></span>`;
    row.querySelector("strong")!.textContent = `${label}:`;
    row.querySelector("span")!.textContent = text;
    card.append(row);
  }
  return card;
}

function renderCharactersStage(): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-characters";
  const characters: CharacterRecord[] = state?.characters ?? [];
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-inspector-actions";
  toolbar.append(
    actionButton(newCharacterOpen ? "Cancel new character" : "New character", () => {
      newCharacterOpen = !newCharacterOpen;
      if (!newCharacterOpen) characterDrafts.delete("__new__");
      renderWorkflowDesk();
    }),
  );
  list.append(toolbar);
  if (newCharacterOpen) list.append(renderCharacterEditor(null));
  if (!characters.length && !newCharacterOpen) {
    list.append(
      missingPanel(
        "No characters discovered.",
        "Create a profile here. Identity model sheets and voice assets will remain in that character’s folder.",
      ),
    );
  }
  for (const character of characters) {
    list.append(renderCharacterEditor(character));
  }
  return list;
}

function renderCharacterEditor(character: CharacterRecord | null): HTMLElement {
  const key = character?.id ?? "__new__";
  const draft = characterDraft(key, character);
  const card = document.createElement("article");
  card.className = "workflow-character-card workflow-character-editor";
  const head = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = character?.name ?? "New character";
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = character
    ? `${character.identity_ready ? "identity ready" : "no identity"} · ${
        character.voice_ready ? "voice ready" : "no voice"
      }`
    : "Profile draft";
  head.append(heading, status);
  card.append(head);

  const fields = document.createElement("div");
  fields.className = "workflow-character-fields";
  const name = sketchTextField(fields, "Character name", draft.name);
  name.readOnly = Boolean(character);
  name.addEventListener("input", () => {
    draft.name = name.value;
  });
  const description = sketchTextarea(
    fields,
    "Description",
    draft.profile.description ?? "",
    3,
  );
  description.addEventListener("input", () => {
    draft.profile.description = nullable(description.value);
  });
  characterListField(
    fields,
    "Appearance",
    draft.profile.appearance ?? [],
    (values) => {
      draft.profile.appearance = values;
    },
  );
  characterListField(
    fields,
    "Default wardrobe",
    draft.profile.wardrobe ?? [],
    (values) => {
      draft.profile.wardrobe = values;
    },
  );
  characterListField(
    fields,
    "Personality",
    draft.profile.personality ?? [],
    (values) => {
      draft.profile.personality = values;
    },
  );
  characterListField(
    fields,
    "Continuity notes",
    draft.profile.continuity_notes ?? [],
    (values) => {
      draft.profile.continuity_notes = values;
    },
  );
  const identityPrompt = sketchTextarea(
    fields,
    "Identity prompt",
    draft.profile.identity_prompt ?? "",
    3,
  );
  identityPrompt.addEventListener("input", () => {
    draft.profile.identity_prompt = nullable(identityPrompt.value);
  });
  card.append(fields);

  if (character) {
    card.append(renderCharacterModelSheet(character));
  }

  if (character?.notes?.length) {
    const notes = document.createElement("p");
    notes.className = "workflow-field-hint";
    notes.textContent = character.notes.join(" · ");
    card.append(notes);
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton(character ? "Save profile" : "Create character", () => {
      const request = {
        characterId: character?.id,
        name: draft.name,
        profile: draft.profile,
      };
      void runAction(
        character ? "Saving character profile…" : "Creating character…",
        "workflow_save_character",
        request,
      ).then((saved) => {
        if (!saved) return;
        characterDrafts.delete(key);
        if (!character) newCharacterOpen = false;
        renderWorkflowDesk();
      });
    }),
  );
  if (character) {
    const directory = resolveProjectPath(character.dir) ?? character.dir;
    actions.append(
      actionButton(character.voice_ready ? "Edit voice" : "Create voice", () => {
        hooks?.onOpenVoice(directory);
      }),
      actionButton(
        character.identity_ready ? "Update identity" : "Build identity",
        () => {
          const profilePath = resolveProjectPath(character.profile_path);
          const identityBrief = draft.profile.identity_prompt?.trim();
          const prompt = `Build or update the model sheet for ${character.name} in ${character.dir}/model_sheet.${
            identityBrief ? ` Identity brief: ${identityBrief}` : ""
          }`;
          hooks?.onComposeChat(prompt, profilePath);
        },
      ),
    );
    if (character.profile_path) {
      actions.append(
        actionButton("Open profile", () => {
          const path = resolveProjectPath(character.profile_path);
          if (path) hooks?.onOpenPath(path);
        }),
      );
    }
    if (character.model_sheet_manifest_path) {
      actions.append(
        actionButton("Open model sheet manifest", () => {
          const path = resolveProjectPath(character.model_sheet_manifest_path);
          if (path) hooks?.onOpenPath(path);
        }),
      );
    }
  }
  card.append(actions);
  return card;
}

function characterDraft(
  key: string,
  character: CharacterRecord | null,
): { name: string; profile: CharacterProfile } {
  const existing = characterDrafts.get(key);
  if (existing) return existing;
  const profile = character?.profile ?? {};
  const draft = {
    name: character?.name ?? "",
    profile: {
      description: profile.description ?? null,
      appearance: [...(profile.appearance ?? [])],
      wardrobe: [...(profile.wardrobe ?? [])],
      personality: [...(profile.personality ?? [])],
      continuity_notes: [...(profile.continuity_notes ?? [])],
      identity_prompt: profile.identity_prompt ?? null,
    },
  };
  characterDrafts.set(key, draft);
  return draft;
}

function characterListField(
  parent: HTMLElement,
  title: string,
  values: string[],
  onInput: (values: string[]) => void,
): HTMLTextAreaElement {
  const input = sketchTextarea(parent, title, values.join("\n"), 2);
  input.placeholder = "One item per line";
  input.addEventListener("input", () => {
    onInput(splitList(input.value));
  });
  return input;
}

const CHARACTER_IMAGE_ROLES = new Set([
  "identity_reference",
  "body_reference",
  "expression_reference",
]);

function renderCharacterModelSheet(character: CharacterRecord): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-character-model-sheet";
  const heading = document.createElement("h4");
  const assets = (character.asset_ids ?? [])
    .map(findAsset)
    .filter(
      (asset): asset is AssetRecord =>
        Boolean(asset && CHARACTER_IMAGE_ROLES.has(asset.role)),
    );
  heading.textContent = assets.length
    ? `Model sheet · ${assets.length} saved still${assets.length === 1 ? "" : "s"}`
    : "Model sheet";
  section.append(heading);
  if (!assets.length) {
    section.append(
      block(
        "empty-state",
        "No saved model-sheet stills. Use Build identity to create the pack.",
      ),
    );
    return section;
  }
  const gallery = document.createElement("div");
  gallery.className = "workflow-character-model-sheet-gallery";
  for (const asset of assets) {
    const figure = document.createElement("figure");
    const image = document.createElement("img");
    image.alt = `${character.name} ${asset.notes ?? asset.role}`;
    image.dataset.lookPath = resolveProjectPath(asset.path) ?? asset.path;
    image.addEventListener("click", () => {
      hooks?.onOpenPath(resolveProjectPath(asset.path) ?? asset.path);
    });
    const caption = document.createElement("figcaption");
    caption.textContent = asset.notes?.trim() || asset.role.replace(/_/g, " ");
    figure.append(image, caption);
    gallery.append(figure);
  }
  section.append(gallery);
  return section;
}

function renderCutCards(scene: SceneRecord, showMode: boolean): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-cuts";
  list.setAttribute("aria-label", "Cuts");
  for (const cut of scene.cuts) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `workflow-cut${cut.id.cut === selectedCut ? " active" : ""}`;
    const mode = cut.sketch?.shot_role
      ? `${roleLabel(cut.sketch.shot_role)} · ${mouthLabel(cut.sketch.mouth_policy)}`
      : showMode
        ? speechLabel(cut.speech_mode)
        : "sketch pending";
    const start = cut.approved_start_frame ? "start frame" : "identity ref only";
    button.innerHTML = `<span class="workflow-cut-num"></span><span class="workflow-cut-title"></span><span class="workflow-cut-mode"></span><span class="workflow-cut-ref"></span>`;
    button.querySelector(".workflow-cut-num")!.textContent = `Cut ${cut.id.cut}`;
    button.querySelector(".workflow-cut-title")!.textContent = cut.title?.trim() || `Scene ${cut.id.source_scene} Cut ${cut.id.cut}`;
    button.querySelector(".workflow-cut-mode")!.textContent = mode;
    button.querySelector(".workflow-cut-ref")!.textContent = start;
    if (cut.sketch?.shot_role === "performance" || cut.speech_mode === "source_video_lipsync") {
      button.classList.add("performance");
    }
    button.addEventListener("click", () => {
      selectedCut = cut.id.cut;
      renderWorkflowDesk();
      emitSelection();
    });
    list.append(button);
  }
  return list;
}

function renderInspector(): HTMLElement {
  const aside = document.createElement("aside");
  aside.className = "workflow-inspector";
  aside.setAttribute("aria-label", "Inspector");
  const heading = document.createElement("h2");
  heading.textContent = "Inspector";
  aside.append(heading);
  const scene = selectedScene();
  const sceneStatus = sceneReport(selectedSceneId);
  const cut = selectedCutRecord();
  if (!scene) {
    aside.append(block("empty-state", "Nothing selected."));
    return aside;
  }
  aside.append(renderReadiness(scene, sceneStatus));
  if (sceneStatus) {
    const summary = document.createElement("pre");
    summary.className = "workflow-excerpt";
    summary.textContent = sceneStatus.summary;
    aside.append(summary);
  }
  if (cut) {
    const dl = document.createElement("dl");
    dl.className = "workflow-facts";
    addFact(dl, "Cut", `${cut.id.source_scene}.${cut.id.cut}`);
    addFact(dl, "Title", cut.title ?? "—");
    addFact(dl, "Mode", speechLabel(cut.speech_mode));
    addFact(dl, "Shot role", roleLabel(cut.sketch?.shot_role));
    addFact(dl, "Mouth", mouthLabel(cut.sketch?.mouth_policy));
    addFact(dl, "Subject", cut.subject ?? cut.sketch?.subject ?? "—");
    addFact(dl, "Start frame", cut.approved_start_frame ?? "none (not a model-sheet closeup)");
    addFact(dl, "Note", cut.production_note_path ?? "—");
    if (cut.dialogue?.length) {
      addFact(
        dl,
        "Dialogue",
        cut.dialogue.map((line) => `${line.speaker}: ${line.text}`).join(" "),
      );
    }
    aside.append(dl);
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    const notePath = resolveProjectPath(cut.production_note_path);
    if (notePath) {
      actions.append(
        actionButton("Open note", () => hooks?.onOpenPath(notePath)),
        actionButton("Add note to chat", () => hooks?.onAddToChat(notePath)),
      );
    }
    const scriptPath = resolveProjectPath(scene.source_path);
    if (scriptPath) {
      actions.append(actionButton("Open script", () => hooks?.onOpenPath(scriptPath)));
    }
    aside.append(actions);
  }
  return aside;
}

function roleLabel(role: ShotRole | null | undefined): string {
  return SHOT_ROLES.find((item) => item.id === role)?.label ?? "—";
}

function mouthLabel(policy: MouthPolicy | null | undefined): string {
  return MOUTH_POLICIES.find((item) => item.id === policy)?.label ?? "—";
}

function renderJobs(): HTMLElement {
  const drawer = document.createElement("section");
  drawer.className = `workflow-jobs${jobsOpen ? " open" : ""}`;
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "workflow-jobs-toggle";
  const count = state?.jobs?.length ?? 0;
  toggle.textContent = `Jobs (${count})`;
  toggle.addEventListener("click", () => {
    jobsOpen = !jobsOpen;
    renderWorkflowDesk();
  });
  drawer.append(toggle);
  if (jobsOpen) {
    const body = document.createElement("div");
    body.className = "workflow-jobs-body";
    if (!count) {
      body.append(
        block(
          "empty-state",
          "No provider jobs recorded yet. Scene look generates and later renders land here.",
        ),
      );
    } else {
      for (const job of state?.jobs ?? []) {
        const row = document.createElement("p");
        row.textContent = `${job.kind} · ${job.status ?? "unknown"} · ${
          job.output_path ?? job.model ?? job.provider ?? job.id
        }`;
        body.append(row);
      }
    }
    drawer.append(body);
  }
  return drawer;
}

function renderReadiness(
  scene: SceneRecord | undefined,
  sceneStatus: SceneReport | undefined,
): HTMLElement {
  const list = document.createElement("ul");
  list.className = "workflow-readiness";
  const rows: [boolean, string][] = [
    [scene?.stages.script === "present", "Script beats"],
    [
      (state?.characters.length ?? 0) > 0,
      `${state?.characters.length ?? 0} characters (${report?.identity_ready ?? 0} identity-ready)`,
    ],
    [Boolean(sceneStatus?.scene_intent), "Scene intent"],
    [Boolean(sceneStatus?.scene_look), "Scene look board"],
    [Boolean(sceneStatus?.cut_sketches), "Cut sketches"],
    [
      (sceneStatus?.approved_start_frames ?? 0) > 0,
      `${sceneStatus?.approved_start_frames ?? 0} approved start frames`,
    ],
    [Boolean(sceneStatus?.production_notes), "Production notes"],
  ];
  for (const [ready, label] of rows) {
    const item = document.createElement("li");
    item.className = ready ? "ready" : "missing";
    item.textContent = `${ready ? "Present" : "Missing"} · ${label}`;
    list.append(item);
  }
  return list;
}

function missingPanel(title: string, detail: string): HTMLElement {
  const article = document.createElement("article");
  article.className = "workflow-missing";
  const h = document.createElement("h3");
  h.textContent = title;
  const p = document.createElement("p");
  p.textContent = detail;
  article.append(h, p);
  return article;
}

function block(className: string, text: string): HTMLElement {
  const el = document.createElement("p");
  el.className = className;
  el.textContent = text;
  return el;
}

function actionButton(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost-button";
  button.textContent = label;
  button.disabled = Boolean(busy);
  button.addEventListener("click", onClick);
  return button;
}

function addFact(dl: HTMLDListElement, label: string, value: string): void {
  const dt = document.createElement("dt");
  dt.textContent = label;
  const dd = document.createElement("dd");
  dd.textContent = value;
  dl.append(dt, dd);
}

function statusLabel(status: StageStatus): string {
  switch (status) {
    case "present":
      return "ready";
    case "draft":
      return "draft";
    case "approved":
      return "approved";
    default:
      return "missing";
  }
}

function speechLabel(mode: SpeechMode | null | undefined): string {
  switch (mode) {
    case "source_video_lipsync":
      return "lip-sync";
    case "voice_over":
      return "voice-over";
    case "none":
      return "silent";
    default:
      return "unspecified";
  }
}

function mixLabel(mix: MixCounts): string {
  return `${mix.lipsync} LS / ${mix.voice_over} VO / ${mix.silent} silent`;
}

function selectionExcerpt(
  scene: SceneRecord | undefined,
  cut: CutRecord | undefined,
  sceneStatus: SceneReport | undefined,
): string | null {
  if (!scene) return null;
  const lines = [
    `Workflow selection: ${scene.title}`,
    `Workflow stage: ${selectedStage}`,
    selectedStage === "scene_intent" || selectedStage === "cut_sketches"
      ? "Scene intent and cut sketches are not production notes. Existing prod.md coverage does not complete this stage."
      : "",
    sceneStatus?.summary ?? "",
  ];
  if (cut) {
    lines.push(
      `Selected cut ${cut.id.cut}: ${cut.title ?? cut.sketch?.title ?? ""} (${speechLabel(cut.speech_mode)}${
        cut.sketch?.shot_role ? `, ${cut.sketch.shot_role}` : ""
      })`,
    );
    if (cut.sketch?.visual_prompt) lines.push(cut.sketch.visual_prompt.slice(0, 1200));
    else if (cut.prompt) lines.push(cut.prompt.slice(0, 1200));
  }
  const text = lines.filter(Boolean).join("\n\n").trim();
  return text || null;
}
