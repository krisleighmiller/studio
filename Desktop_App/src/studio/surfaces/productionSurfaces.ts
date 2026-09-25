import { LOOK_ROLES, MOUTH_POLICIES, REQUIREMENT_KINDS, REQUIREMENT_STATUSES, SCENE_STRATEGIES, SHOT_ROLES, VISUAL_SOURCE_STRATEGIES, type AnimaticShot, type AssetRecord, type CharacterProfile, type CharacterRecord, type CutRecord, type CutSketch, type RequirementStatus, type SceneIntent, type SceneRecord, type SceneReport, type TakeRecord, type VisualSourceStrategy } from "../../workflowTypes";
import { roleLabel, speechLabel } from "../labels";
import type { SurfaceHost } from "./host";
import { buildAssembleModel, buildCharactersModel, buildIntentModel, buildNotesModel, buildProfileModel, buildReferenceModel, buildRenderQueueModel, buildSketchModel, buildTakeReviewModel, type SurfacePresentation } from "./surfaceModels";

let host: SurfaceHost;
let intentDraft: SceneIntent | null = null;
let intentDraftScene: number | null = null;
const startFrameEditBriefs = new Map<string, string>();
const characterDrafts = new Map<string, { name: string; profile: CharacterProfile }>();
let newCharacterOpen = false;
let profileOpen = false;
let roughPlayback: { sceneId: number; index: number; timer: number; audio: HTMLAudioElement | null } | null = null;

export function bindSurfaceHost(next: SurfaceHost): void {
  host = next;
}

export function resetSurfaceSession(): void {
  intentDraft = null;
  intentDraftScene = null;
  startFrameEditBriefs.clear();
  characterDrafts.clear();
  newCharacterOpen = false;
  profileOpen = false;
  stopRoughCut();
}

export function clearIntentDraft(): void {
  intentDraft = null;
  intentDraftScene = null;
}

export function setProfileOpen(open: boolean): void {
  profileOpen = open;
}

function block(className: string, text: string): HTMLElement {
  const el = document.createElement("p");
  el.className = className;
  el.textContent = text;
  return el;
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

function actionButton(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost-button";
  button.textContent = label;
  button.disabled = Boolean(host.busy);
  button.addEventListener("click", onClick);
  return button;
}

function findAsset(id: string): AssetRecord | undefined {
  return host.state?.assets?.find((asset) => asset.id === id);
}

function queueLookThumbs(root: HTMLElement): void {
  root.querySelectorAll<HTMLImageElement>("img[data-look-path]").forEach((img) => {
    const path = img.dataset.lookPath;
    if (!path) return;
    void host.mediaUrl(path).then((url) => {
      img.src = url;
    }).catch(() => {
      img.alt = "Could not load still";
    });
  });
}

function queueTakeVideos(root: HTMLElement): void {
  root.querySelectorAll<HTMLVideoElement>("video[data-take-path]").forEach((video) => {
    const path = video.dataset.takePath;
    if (!path) return;
    void host.mediaUrl(path).then((url) => {
      video.src = url;
      video.load();
    }).catch(() => {
      video.replaceWith(block("workflow-field-hint", "Could not load this take."));
    });
  });
}

function presentSurface(view: HTMLElement, model: SurfacePresentation, syncBanner = false): HTMLElement {
  view.dataset.status = model.status;
  view.dataset.tone = model.tone;
  if (model.loading) view.setAttribute("aria-busy", "true");
  else view.removeAttribute("aria-busy");
  if (model.blocked.length) view.dataset.blocked = model.blocked.join(", ");
  else delete view.dataset.blocked;
  if (syncBanner && model.banner) {
    const existing = view.querySelector<HTMLElement>(":scope > .workflow-banner, :scope > .workflow-missing");
    if (existing) existing.textContent = model.banner;
  }
  for (const item of model.actions) {
    if (!item.reason) continue;
    for (const button of view.querySelectorAll<HTMLButtonElement>("button")) {
      if (button.textContent === item.label && !item.enabled) button.title = item.reason;
    }
  }
  return view;
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

function paintIntentStage(scene: SceneRecord): HTMLElement {
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
      const chat = host.requireChat();
      if (!chat) return;
      void host.run("Drafting scene intent…", "workflow_draft_intent", {
        sceneId: scene.id,
        ...chat,
      });
    }),
    actionButton("Save draft", () => {
      void host.run("Saving scene intent…", "workflow_save_intent", {
        sceneId: scene.id,
        intent: intentWorkingCopy(scene),
      });
    }),
    actionButton("Approve intent", () => {
      void host.run("Approving scene intent…", "workflow_approve_intent", {
        sceneId: scene.id,
      });
    }),
  );
  wrap.append(actions);
  return wrap;
}


function paintReferenceImagesStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-start-frames";
  const planned = sceneStatus?.planned_visual_sources ?? 0;
  const ready = sceneStatus?.render_ready_sources ?? 0;
  wrap.append(
    block(
      planned ? "workflow-banner" : "workflow-missing",
      planned
        ? `${planned} of ${scene.cuts.length} cuts have a planned visual-source strategy; ${ready} are render ready. Continuity-linked cuts remain correctly planned while they wait for a selected predecessor take.`
        : "No visual-source strategies are approved. Choose how each cut obtains its opening image; not every cut needs a preproduction start frame.",
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

function renderStartFrameCard(
  scene: SceneRecord,
  cut: CutRecord,
  embedded = false,
): HTMLElement {
  const card = document.createElement(embedded ? "section" : "article");
  card.className = `workflow-start-frame-card${embedded ? " embedded" : ""}`;
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = embedded
    ? "Visual source and reference image"
    : `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  const currentStrategy = effectiveVisualSourceStrategy(cut);
  status.textContent = visualSourceStatus(cut);
  header.append(heading, status);
  card.append(header);

  const sourceControls = document.createElement("section");
  sourceControls.className = "workflow-source-controls";
  const strategyLabel = document.createElement("label");
  strategyLabel.className = "workflow-field";
  const strategyTitle = document.createElement("span");
  strategyTitle.textContent = "Visual-source strategy";
  const strategySelect = document.createElement("select");
  for (const option of VISUAL_SOURCE_STRATEGIES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    strategySelect.append(item);
  }
  strategySelect.value = currentStrategy ?? "preproduction_start_frame";
  strategyLabel.append(strategyTitle, strategySelect);

  const predecessorLabel = document.createElement("label");
  predecessorLabel.className = "workflow-field";
  const predecessorTitle = document.createElement("span");
  predecessorTitle.textContent = "Predecessor cut";
  const predecessorSelect = document.createElement("select");
  const earlierCuts = scene.cuts.filter((candidate) => candidate.id.cut < cut.id.cut);
  for (const candidate of earlierCuts) {
    const item = document.createElement("option");
    item.value = String(candidate.id.cut);
    item.textContent = `Cut ${candidate.id.cut} · ${candidate.title?.trim() || "Untitled"}`;
    predecessorSelect.append(item);
  }
  predecessorSelect.value = String(
    cut.visual_source?.predecessor?.cut ??
      earlierCuts[earlierCuts.length - 1]?.id.cut ??
      "",
  );
  predecessorLabel.append(predecessorTitle, predecessorSelect);

  const sharedLabel = document.createElement("label");
  sharedLabel.className = "workflow-field";
  const sharedTitle = document.createElement("span");
  sharedTitle.textContent = "Approved shared reference";
  const sharedSelect = document.createElement("select");
  const emptyShared = document.createElement("option");
  emptyShared.value = "";
  emptyShared.textContent = "Choose an approved scene reference";
  sharedSelect.append(emptyShared);
  for (const asset of approvedSceneLookAssets(scene)) {
    const item = document.createElement("option");
    item.value = asset.id;
    item.textContent = asset.path.split("/").pop() ?? asset.id;
    sharedSelect.append(item);
  }
  sharedSelect.value = cut.visual_source?.asset_id ?? "";
  sharedLabel.append(sharedTitle, sharedSelect);

  const sourceHint = document.createElement("p");
  sourceHint.className = "workflow-field-hint";
  const boundAsset = cut.visual_source?.asset_id
    ? findAsset(cut.visual_source.asset_id)
    : undefined;
  sourceHint.textContent = visualSourceHint(cut, boundAsset);

  const sourceActions = document.createElement("div");
  sourceActions.className = "workflow-inspector-actions";
  const saveSource = actionButton("Save source strategy", () => {
    const strategy = strategySelect.value as VisualSourceStrategy;
    const args: Record<string, unknown> = {
      sceneId: scene.id,
      cut: cut.id.cut,
      strategy,
    };
    if (strategy === "previous_selected_take_exit_frame") {
      args.predecessorCut = Number(predecessorSelect.value);
    }
    if (strategy === "shared_reference" && sharedSelect.value) {
      args.assetId = sharedSelect.value;
    }
    void host.run("Saving visual-source strategy…", "workflow_set_visual_source", args);
  });
  sourceActions.append(saveSource);
  const frameTimeLabel = document.createElement("label");
  frameTimeLabel.className = "workflow-field";
  const frameTimeTitle = document.createElement("span");
  frameTimeTitle.textContent = "Frame time (seconds)";
  const frameTime = document.createElement("input");
  frameTime.type = "number";
  frameTime.min = "0";
  frameTime.step = "0.1";
  frameTime.value =
    cut.visual_source?.frame_time_seconds != null
      ? String(cut.visual_source.frame_time_seconds)
      : "";
  frameTimeLabel.append(frameTimeTitle, frameTime);
  const preview = document.createElement("video");
  preview.controls = true;
  preview.muted = true;
  preview.preload = "metadata";
  preview.className = "workflow-take-video";
  let frameTimeEdited = frameTime.value !== "";
  frameTime.addEventListener("input", () => {
    frameTimeEdited = true;
  });
  preview.addEventListener("loadedmetadata", () => {
    if (frameTimeEdited || !Number.isFinite(preview.duration)) return;
    const nearEnd = Math.max(0, preview.duration - 0.1);
    frameTime.value = nearEnd.toFixed(3);
  });
  const bindContinuity = actionButton("Bind selected frame", () => {
    void bindContinuityFrame(scene, cut, predecessorSelect.value, frameTime.value);
  });
  sourceActions.append(bindContinuity);
  sourceControls.append(
    strategyLabel,
    predecessorLabel,
    frameTimeLabel,
    preview,
    sharedLabel,
    sourceHint,
    sourceActions,
  );
  card.append(sourceControls);

  const toggleSourceFields = (): void => {
    const strategy = strategySelect.value as VisualSourceStrategy;
    const continuity = strategy === "previous_selected_take_exit_frame";
    predecessorLabel.hidden = !continuity;
    frameTimeLabel.hidden = !continuity;
    preview.hidden = !continuity;
    sharedLabel.hidden = strategy !== "shared_reference";
    bindContinuity.hidden = !continuity;
    const predecessor = earlierCuts.find(
      (candidate) => candidate.id.cut === Number(predecessorSelect.value),
    );
    const take = predecessor?.takes?.find(
      (candidate) =>
        candidate.id === predecessor?.selected_take ||
        candidate.path === predecessor?.selected_take,
    );
    bindContinuity.disabled = Boolean(host.busy) || !take;
    if (take) {
      preview.dataset.takePath = host.resolvePath(take.path) ?? take.path;
    } else {
      delete preview.dataset.takePath;
      preview.removeAttribute("src");
    }
    queueTakeVideos(sourceControls);
  };
  strategySelect.addEventListener("change", toggleSourceFields);
  predecessorSelect.addEventListener("change", toggleSourceFields);
  toggleSourceFields();

  const directSource = document.createElement("section");
  directSource.className = "workflow-direct-start-frame";
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
  directSource.append(promptLabel, negativeLabel);

  const promptActions = document.createElement("div");
  promptActions.className = "workflow-inspector-actions";
  promptActions.append(
    actionButton("Draft image prompt", () => {
      const chat = host.requireChat();
      if (!chat) return;
      void host.run(
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
      void host.run("Saving start-frame prompt…", "workflow_save_start_frame_prompt", {
        sceneId: scene.id,
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    }),
    actionButton("Generate start frame", () => {
      void host.run("Preparing start-frame generate…", "workflow_generate_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
      });
    }),
    actionButton("Attach still", () => {
      void attachStartFrame(scene.id, cut.id.cut);
    }),
  );
  directSource.append(promptActions);

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
  directSource.append(versions);

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
  directSource.append(editLabel);
  directSource.hidden = strategySelect.value !== "preproduction_start_frame";
  strategySelect.addEventListener("change", () => {
    directSource.hidden = strategySelect.value !== "preproduction_start_frame";
  });
  card.append(directSource);
  return card;
}

export function effectiveVisualSourceStrategy(cut: CutRecord): VisualSourceStrategy | null {
  return cut.visual_source?.strategy ??
    (cut.approved_start_frame ? "preproduction_start_frame" : null);
}

export function visualSourceReady(cut: CutRecord): boolean {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (strategy === "no_start_frame") return true;
  if (strategy === "preproduction_start_frame") return Boolean(cut.approved_start_frame);
  return Boolean(cut.visual_source?.asset_id);
}

function visualSourceStatus(cut: CutRecord): string {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (!strategy) return "Source strategy needed";
  if (visualSourceReady(cut)) return "Render source ready";
  if (strategy === "previous_selected_take_exit_frame") return "Planned · awaiting take";
  if (strategy === "shared_reference") return "Planned · choose reference";
  if (strategy === "deferred_manual") return "Planned · awaiting manual source";
  return "Planned · start frame needed";
}

function renderBlocker(cut: CutRecord): string | null {
  if (!cut.production_note_path) return "production note missing";
  if (!cut.prompt?.trim()) return "production video prompt missing";
  if (!effectiveVisualSourceStrategy(cut)) return "visual-source strategy missing";
  if (!visualSourceReady(cut)) {
    const strategy = effectiveVisualSourceStrategy(cut);
    if (strategy === "preproduction_start_frame") return "approved start frame missing";
    if (strategy === "previous_selected_take_exit_frame") {
      return "predecessor take or exit frame missing";
    }
    if (strategy === "shared_reference") return "approved shared reference missing";
    return "concrete render source missing";
  }
  return null;
}

function visualSourceHint(cut: CutRecord, boundAsset: AssetRecord | undefined): string {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (!strategy) {
    return "Choose and save a strategy. This approves how the cut will obtain its opening image.";
  }
  if (boundAsset) {
    return `Bound source: ${boundAsset.path}`;
  }
  if (strategy === "previous_selected_take_exit_frame") {
    const predecessor = cut.visual_source?.predecessor?.cut;
    return predecessor
      ? `Depends on Cut ${predecessor}. Production notes may proceed, but rendering waits for that cut’s selected take and exit frame.`
      : "Choose an earlier cut. Production notes may preserve the dependency before its frame exists.";
  }
  if (strategy === "shared_reference") {
    return "Choose an approved scene-look still to use as the shared source.";
  }
  if (strategy === "no_start_frame") {
    return "This cut is render ready without a starting image.";
  }
  if (strategy === "deferred_manual") {
    return "The source is intentionally deferred. Rendering remains blocked until a concrete asset is attached.";
  }
  return cut.approved_start_frame
    ? `Approved start frame: ${cut.approved_start_frame}`
    : "Generate or attach a still, then choose Use on the approved version.";
}

function approvedSceneLookAssets(scene: SceneRecord): AssetRecord[] {
  const ids = new Set(
    (scene.look_board?.slots ?? [])
      .map((slot) => slot.approved_asset_id)
      .filter((id): id is string => Boolean(id)),
  );
  return (host.state?.assets ?? []).filter(
    (asset) =>
      ids.has(asset.id) &&
      asset.scene_id === scene.id &&
      asset.status === "approved",
  );
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
  image.dataset.lookPath = host.resolvePath(asset.path) ?? "";
  image.addEventListener("click", () => {
    host.hooks?.onOpenPath(host.resolvePath(asset.path) ?? asset.path);
  });
  const caption = document.createElement("figcaption");
  caption.textContent = `${asset.path.split("/").pop() ?? asset.id} · ${
    cut.approved_start_frame === asset.path ? "approved source" : (asset.status ?? "draft")
  }`;
  const actions = document.createElement("div");
  actions.className = "workflow-start-frame-version-actions";
  actions.append(
    actionButton("Use", () => {
      void host.run("Approving cut start frame…", "workflow_approve_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
        assetId: asset.id,
      });
    }),
    actionButton("Reject", () => {
      void host.run("Rejecting cut start frame…", "workflow_reject_start_frame", {
        sceneId: scene.id,
        cut: cut.id.cut,
        assetId: asset.id,
      });
    }),
    actionButton("Edit", () => {
      const brief = startFrameEditBriefs.get(`${scene.id}.${cut.id.cut}`)?.trim() ?? "";
      if (!brief) {
        host.setError(`Enter an edit brief under Cut ${cut.id.cut} first.`);
        host.rerender();
        return;
      }
      void host.run("Preparing start-frame edit…", "workflow_edit_start_frame", {
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
  return (host.state?.assets ?? []).filter(
    (asset) =>
      CUT_IMAGE_ROLES.has(asset.role) &&
      asset.cut_id?.source_scene === sceneId &&
      asset.cut_id?.cut === cutNumber,
  );
}

async function attachStartFrame(sceneId: number, cutNumber: number): Promise<void> {
  const selected = await host.pickFile({
    title: `Attach start frame · Scene ${sceneId} Cut ${cutNumber}`,
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  const path = Array.isArray(selected) ? selected[0] : selected;
  if (!path) return;
  await host.run("Attaching cut start frame…", "workflow_attach_start_frame", {
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
  const confirmed = await host.ask(
    `Delete ${asset.path}?\n\nThis removes this exact cut-image version and its registry entry. Provider job history is retained.`,
    { title: "Delete cut image", kind: "warning" },
  );
  if (!confirmed) return;
  await host.run("Deleting cut image…", "workflow_delete_start_frame", {
    sceneId: scene.id,
    cut: cut.id.cut,
    assetId: asset.id,
  });
}

function paintSketchStage(scene: SceneRecord, sceneStatus: SceneReport | undefined): HTMLElement {
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
    void host.run("Setting strategy…", "workflow_set_strategy", {
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
    void host.run("Updating mix override…", "workflow_set_strategy", {
      sceneId: scene.id,
      strategy: strategySelect.value,
      override: override.checked,
    });
  });
  overrideLabel.append(override, document.createTextNode(" Override mix rejection"));
  const alternate = actionButton("Add alternate angle", () => {
    void host.run("Adding alternate angle…", "workflow_add_alternate_shot", {
      sceneId: scene.id,
    });
  });
  alternate.disabled = Boolean(host.busy);
  alternate.title = "Allocates a new shot id, an editable sketch, and animatic timing. The source strategy and production note stay empty.";
  toolbar.append(strategyLabel, overrideLabel, alternate);
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
        void host.run("Backfilling cut sketches…", "workflow_backfill_sketches", {
          sceneId: scene.id,
          replace: true,
        });
      }),
    );
  }
  actions.append(
    actionButton("Draft sketches", () => {
      const chat = host.requireChat();
      if (!chat) return;
      void host.run("Drafting cut sketches…", "workflow_draft_sketches", {
        sceneId: scene.id,
        strategy: strategySelect.value,
        ...chat,
      });
    }),
  );
  wrap.append(actions);
  if (scene.cuts.length) {
    wrap.append(renderAnimatic(scene));
    wrap.append(renderSketchCards(scene));
  }
  return wrap;
}

function cutForShot(scene: SceneRecord, shot: AnimaticShot): CutRecord | undefined {
  return scene.cuts.find(
    (cut) => cut.id.cut === shot.cut_id.cut && cut.id.source_scene === shot.cut_id.source_scene,
  );
}

export function stopRoughCut(): void {
  if (!roughPlayback) return;
  window.clearTimeout(roughPlayback.timer);
  roughPlayback.audio?.pause();
  roughPlayback = null;
}

function paintRoughStage(scene: SceneRecord, shot: AnimaticShot | null): void {
  const stage = document.querySelector<HTMLElement>(".workflow-rough-cut-stage");
  if (!stage || Number(stage.dataset.sceneId) !== scene.id) return;
  stage.replaceChildren();
  if (!shot) {
    stage.hidden = true;
    return;
  }
  stage.hidden = false;
  const cut = cutForShot(scene, shot);
  const title = document.createElement("h3");
  const index = (scene.animatic?.shots ?? []).findIndex((item) => item.id === shot.id);
  title.textContent = `${index + 1}. Cut ${shot.cut_id.cut} · ${cut?.sketch?.title || cut?.title || "Untitled"} · ${shot.duration_seconds.toFixed(2)}s`;
  const dialogue = document.createElement("p");
  const lines = cut?.sketch?.dialogue?.length ? cut.sketch.dialogue : [];
  const spoken = lines.length
    ? lines.map((line) => `${line.speaker}: ${line.text}`).join(" ")
    : (cut?.dialogue ?? []).map((line) => `${line.speaker}: ${line.text}`).join(" ");
  dialogue.textContent = spoken || "No dialogue on this shot.";
  stage.append(title, dialogue);
  const imagePath = host.resolvePath(cut?.approved_start_frame);
  if (imagePath) {
    const image = document.createElement("img");
    image.alt = title.textContent ?? "";
    stage.append(image);
    void host.mediaUrl(imagePath).then((url) => {
      image.src = url;
    });
  }
}

async function showRoughShot(scene: SceneRecord, index: number): Promise<void> {
  if (!roughPlayback || roughPlayback.sceneId !== scene.id) return;
  const shots = scene.animatic?.shots ?? [];
  const shot = shots[index];
  if (!shot) {
    stopRoughCut();
    paintRoughStage(scene, null);
    host.rerender();
    return;
  }
  roughPlayback.index = index;
  roughPlayback.audio?.pause();
  roughPlayback.audio = null;
  paintRoughStage(scene, shot);
  if (shot.scratch_audio_path) {
    const abs = host.resolvePath(shot.scratch_audio_path);
    if (abs) {
      const url = await host.mediaUrl(abs);
      if (!roughPlayback || roughPlayback.sceneId !== scene.id || roughPlayback.index !== index) return;
      const audio = new Audio(url);
      roughPlayback.audio = audio;
      void audio.play().catch(() => undefined);
    }
  }
  const ms = Math.max(0.4, shot.duration_seconds) * 1000;
  roughPlayback.timer = window.setTimeout(() => {
    const current = host.state?.scenes.find((item) => item.id === scene.id) ?? scene;
    void showRoughShot(current, index + 1);
  }, ms);
}

function playRoughCut(scene: SceneRecord): void {
  const shots = scene.animatic?.shots ?? [];
  if (!shots.length) return;
  stopRoughCut();
  roughPlayback = { sceneId: scene.id, index: 0, timer: 0, audio: null };
  host.rerender();
  void showRoughShot(scene, 0);
}

function renderAnimatic(scene: SceneRecord): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-animatic";
  section.setAttribute("aria-label", "Animatic");
  const shots = scene.animatic?.shots ?? [];
  const total = shots.reduce((sum, shot) => sum + (shot.duration_seconds || 0), 0);
  const heading = document.createElement("h2");
  heading.textContent = scene.animatic?.approved
    ? `Rough cut · approved · ${total.toFixed(1)}s`
    : `Rough cut · ${total.toFixed(1)}s`;
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Timing and order sit on the cut sketches. Dialogue without a scratch file plays as temporary on-screen text. A timing change marks later shot packages for review and keeps existing renders.";
  section.append(heading, note);

  const stage = document.createElement("div");
  stage.className = "workflow-rough-cut-stage";
  stage.dataset.sceneId = String(scene.id);
  stage.hidden = roughPlayback?.sceneId !== scene.id;
  section.append(stage);

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  controls.append(
    actionButton(roughPlayback?.sceneId === scene.id ? "Stop rough cut" : "Play rough cut", () => {
      if (roughPlayback?.sceneId === scene.id) {
        stopRoughCut();
        host.rerender();
        return;
      }
      playRoughCut(scene);
    }),
    actionButton(scene.animatic?.approved ? "Rough cut approved" : "Approve rough cut", () => {
      void host.run("Approving rough cut…", "workflow_approve_animatic", { sceneId: scene.id });
    }),
  );
  const approveButton = controls.querySelectorAll("button")[1] as HTMLButtonElement | undefined;
  if (approveButton && scene.animatic?.approved) approveButton.disabled = true;
  section.append(controls);

  const warnings = scene.animatic?.warnings ?? [];
  if (warnings.length) {
    section.append(
      block(
        "workflow-banner warn",
        warnings.map((warning) => warning.message).join("\n"),
      ),
    );
  }

  const list = document.createElement("ol");
  list.className = "workflow-animatic-list";
  shots.forEach((shot, index) => {
    const cut = cutForShot(scene, shot);
    const item = document.createElement("li");
    item.className = "workflow-animatic-row";
    const label = document.createElement("span");
    label.className = "workflow-animatic-label";
    label.textContent = `Cut ${shot.cut_id.cut} · ${cut?.sketch?.title || cut?.title || "Untitled"}`;
    const duration = document.createElement("input");
    duration.type = "number";
    duration.min = "0.4";
    duration.max = "120";
    duration.step = "0.1";
    duration.value = String(shot.duration_seconds);
    duration.setAttribute("aria-label", `Duration for cut ${shot.cut_id.cut}`);
    duration.addEventListener("change", () => {
      const seconds = Number(duration.value);
      if (!Number.isFinite(seconds)) return;
      void host.run("Saving animatic timing…", "workflow_set_animatic_duration", {
        sceneId: scene.id,
        shotId: shot.id,
        durationSeconds: seconds,
      });
    });
    const scratch = document.createElement("span");
    scratch.className = "workflow-animatic-scratch";
    const hasDialogue = Boolean(cut?.dialogue?.length || cut?.sketch?.dialogue?.length);
    if (shot.scratch_audio_path) {
      scratch.textContent = shot.scratch_audio_path.split("/").pop() ?? "Scratch audio";
    } else if (hasDialogue) {
      scratch.textContent = "Temporary dialogue";
    } else {
      scratch.textContent = "No scratch audio";
    }
    const rowActions = document.createElement("div");
    rowActions.className = "workflow-animatic-actions";
    rowActions.append(
      actionButton("Up", () => moveAnimaticShot(scene, index, -1)),
      actionButton("Down", () => moveAnimaticShot(scene, index, 1)),
      actionButton(shot.scratch_audio_path ? "Replace scratch" : "Attach scratch", () => {
        void attachScratchAudio(scene.id, shot.id);
      }),
    );
    if (shot.scratch_audio_path) {
      rowActions.append(
        actionButton("Remove scratch", () => {
          void host.run("Removing scratch audio…", "workflow_clear_scratch_audio", {
            sceneId: scene.id,
            shotId: shot.id,
          });
        }),
      );
    }
    item.append(label, duration, scratch, rowActions);
    list.append(item);
  });
  section.append(list);
  if (roughPlayback?.sceneId === scene.id) {
    const current = shots[roughPlayback.index];
    if (current) queueMicrotask(() => paintRoughStage(scene, current));
  }
  return section;
}

function moveAnimaticShot(scene: SceneRecord, index: number, delta: number): void {
  const shots = [...(scene.animatic?.shots ?? [])];
  const next = index + delta;
  if (next < 0 || next >= shots.length) return;
  const [moved] = shots.splice(index, 1);
  shots.splice(next, 0, moved);
  void host.run("Reordering animatic…", "workflow_reorder_animatic", {
    sceneId: scene.id,
    shotIds: shots.map((shot) => shot.id),
  });
}

async function attachScratchAudio(sceneId: number, shotId: string): Promise<void> {
  const selected = await host.pickFile({
    title: "Attach scratch audio",
    multiple: false,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "m4a", "aac", "ogg", "flac"] }],
  });
  const path = Array.isArray(selected) ? selected[0] : selected;
  if (!path) return;
  await host.run("Attaching scratch audio…", "workflow_attach_scratch_audio", {
    sceneId,
    shotId,
    sourcePath: path,
  });
}

function renderSketchCards(scene: SceneRecord): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-sketch-cards";
  for (const cut of scene.cuts) {
    list.append(renderSketchCard(scene, cut));
  }
  queueLookThumbs(list);
  return list;
}

function dependencyNote(state: CutRecord["dependency_state"]): string | null {
  switch (state) {
    case "needs_review":
      return "An approved upstream artifact changed. This shot needs review. Existing files were kept.";
    case "stale":
      return "An approved upstream artifact changed. This shot is stale. Existing files were kept.";
    case "invalid":
      return "An approved upstream artifact changed. This shot is invalid until it is rebuilt. Existing files were kept.";
    case "keep":
      return "An approved upstream artifact changed. This shot is safe to keep.";
    default:
      return null;
  }
}

function moveCut(scene: SceneRecord, cut: CutRecord, delta: number): void {
  const cuts = [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut);
  const index = cuts.findIndex((item) => item.stable_id && item.stable_id === cut.stable_id);
  const next = index + delta;
  if (index < 0 || next < 0 || next >= cuts.length) return;
  const ids = cuts.map((item) => item.stable_id ?? "").filter((id) => id.length > 0);
  if (ids.length !== cuts.length) return;
  const [moved] = ids.splice(index, 1);
  ids.splice(next, 0, moved);
  void host.run("Reordering cuts…", "workflow_reorder_cuts", {
    sceneId: scene.id,
    shotIds: ids,
  });
}

function renderCutOrder(scene: SceneRecord, cut: CutRecord): HTMLElement {
  const cuts = [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut);
  const index = cuts.findIndex((item) => item.id.cut === cut.id.cut);
  const row = document.createElement("div");
  row.className = "workflow-sketch-toolbar";
  const earlier = actionButton("Move earlier", () => moveCut(scene, cut, -1));
  earlier.disabled = index <= 0 || !cut.stable_id;
  const later = actionButton("Move later", () => moveCut(scene, cut, 1));
  later.disabled = index < 0 || index >= cuts.length - 1 || !cut.stable_id;
  row.append(earlier, later);
  return row;
}

function renderSketchCard(scene: SceneRecord, cut: CutRecord): HTMLElement {
  const sketch = editableSketch(cut);
  const card = document.createElement("article");
  card.className = `workflow-sketch-card${cut.id.cut === host.selectedCut ? " active" : ""}`;
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${sketch.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = cut.sketch ? speechLabel(cut.speech_mode) : "Imported fallback";
  header.append(heading, status);
  card.append(header);
  card.append(renderCutOrder(scene, cut));

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
  const mouth = host.recipeRequires("mouth_policy")
    ? sketchSelectField(fields, "Mouth policy", sketch.mouth_policy ?? "", MOUTH_POLICIES)
    : null;
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
      host.rememberScene(scene, cut);
      void host.run("Saving cut sketch…", "workflow_save_sketch", {
        sceneId: scene.id,
        cut: cut.id.cut,
        sketch: {
          title: nullable(title.value),
          shot_role: nullable(role.value),
          subject: nullable(subject.value),
          speaker: nullable(speaker.value),
          mouth_policy: mouth ? nullable(mouth.value) : null,
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
  card.append(renderStartFrameCard(scene, cut, true));
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

const CUT_IMAGE_ROLES = new Set([
  "cut_start_frame",
  "performance_source",
  "reaction_plate",
  "insert_plate",
  "edit_source",
]);

function paintProjectProfile(): HTMLElement {
  const wrap = document.createElement("section");
  wrap.className = "workflow-project";
  wrap.setAttribute("aria-label", "Production profile");
  const profile = host.state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bar = document.createElement("div");
  bar.className = "workflow-profile-bar";
  const recipe = document.createElement("span");
  recipe.className = "workflow-profile-recipe";
  recipe.textContent = host.activeRecipe()?.label ?? "Narrative dialogue";
  const summary = document.createElement("p");
  summary.className = "workflow-profile-summary";
  const bits = [
    deliverable?.name || host.state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  summary.textContent = bits.join(" · ");
  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "ghost-button";
  edit.textContent = profileOpen ? "Close profile" : "Edit profile";
  edit.disabled = Boolean(host.busy);
  edit.addEventListener("click", () => {
    profileOpen = !profileOpen;
    host.rerender();
  });
  bar.append(recipe, summary, edit);
  wrap.append(bar);
  if (profileOpen) wrap.append(renderProfileForm());
  wrap.append(renderRequirements());
  return wrap;
}

function renderProfileForm(): HTMLElement {
  const profile = host.state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const form = document.createElement("form");
  form.className = "workflow-profile-form";
  const recipeLabel = document.createElement("label");
  recipeLabel.textContent = "Recipe";
  const recipeInput = document.createElement("select");
  recipeInput.name = "recipe";
  const choices = host.report?.recipes?.length
    ? host.report.recipes
    : [
        { id: "narrative_dialogue", label: "Narrative dialogue" },
        { id: "montage", label: "Montage" },
      ];
  for (const choice of choices) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    option.selected = choice.id === (profile?.recipe ?? host.activeRecipe()?.id ?? "narrative_dialogue");
    recipeInput.append(option);
  }
  recipeLabel.append(recipeInput);
  form.append(recipeLabel);
  const fields: { key: string; label: string; value: string; wide?: boolean }[] = [
    { key: "name", label: "Deliverable", value: deliverable?.name ?? "" },
    { key: "audience", label: "Audience", value: profile?.audience ?? deliverable?.audience ?? "" },
    {
      key: "platforms",
      label: "Platforms",
      value: (profile?.platforms ?? deliverable?.platforms ?? []).join(", "),
    },
    { key: "runtime", label: "Runtime", value: profile?.runtime ?? deliverable?.runtime ?? "" },
    {
      key: "aspect",
      label: "Aspect ratio",
      value: deliverable?.aspect_ratio ?? profile?.aspect_ratios?.[0] ?? "",
    },
    {
      key: "resolution",
      label: "Resolution",
      value: profile?.resolution ?? deliverable?.resolution ?? "",
    },
    {
      key: "frame_rate",
      label: "Frame rate",
      value: profile?.frame_rate ?? deliverable?.frame_rate ?? "",
    },
    { key: "language", label: "Language", value: profile?.language ?? deliverable?.language ?? "" },
    { key: "medium", label: "Medium", value: profile?.medium ?? "" },
    { key: "style_notes", label: "Style notes", value: profile?.style_notes ?? "", wide: true },
    { key: "audio_plan", label: "Audio plan", value: profile?.audio_plan ?? "", wide: true },
    {
      key: "continuity_rules",
      label: "Continuity rules",
      value: (profile?.continuity_rules ?? []).join("\n"),
      wide: true,
    },
    {
      key: "safety_constraints",
      label: "Safety constraints",
      value: (profile?.safety_constraints ?? []).join("\n"),
      wide: true,
    },
    { key: "budget", label: "Budget", value: profile?.budget ?? "" },
    { key: "schedule", label: "Schedule", value: profile?.schedule ?? "" },
    { key: "rights_notes", label: "Rights notes", value: profile?.rights_notes ?? "", wide: true },
  ];
  for (const field of fields) {
    const label = document.createElement("label");
    label.className = field.wide ? "wide" : "";
    label.textContent = field.label;
    const input = field.wide ? document.createElement("textarea") : document.createElement("input");
    input.name = field.key;
    input.value = field.value;
    if (field.key === "medium") {
      input.setAttribute("placeholder", "Clay, live action, anime…");
    }
    label.append(input);
    form.append(label);
  }
  const save = document.createElement("button");
  save.type = "submit";
  save.className = "ghost-button";
  save.textContent = "Save profile";
  save.disabled = Boolean(host.busy);
  form.append(save);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const read = (key: string) =>
      (form.elements.namedItem(key) as HTMLInputElement | HTMLTextAreaElement | null)?.value ?? "";
    const blank = (value: string) => {
      const trimmed = value.trim();
      return trimmed ? trimmed : null;
    };
    const list = (value: string) =>
      value
        .split(/[,\n]/)
        .map((part) => part.trim())
        .filter(Boolean);
    const lines = (value: string) =>
      value
        .split("\n")
        .map((part) => part.trim())
        .filter(Boolean);
    const name = read("name").trim();
    const aspect = blank(read("aspect"));
    const platforms = list(read("platforms"));
    void host.run("Saving production profile", "workflow_save_profile", {
      profile: {
        recipe: recipeInput.value,
        deliverables: [
          {
            id: deliverable?.id ?? "",
            name,
            audience: blank(read("audience")),
            platforms,
            runtime: blank(read("runtime")),
            aspect_ratio: aspect,
            resolution: blank(read("resolution")),
            frame_rate: blank(read("frame_rate")),
            language: blank(read("language")),
          },
        ],
        audience: blank(read("audience")),
        platforms,
        runtime: blank(read("runtime")),
        aspect_ratios: aspect ? [aspect] : [],
        resolution: blank(read("resolution")),
        frame_rate: blank(read("frame_rate")),
        language: blank(read("language")),
        medium: blank(read("medium")),
        style_notes: blank(read("style_notes")),
        audio_plan: blank(read("audio_plan")),
        continuity_rules: lines(read("continuity_rules")),
        safety_constraints: lines(read("safety_constraints")),
        budget: blank(read("budget")),
        schedule: blank(read("schedule")),
        rights_notes: blank(read("rights_notes")),
      },
    });
  });
  return form;
}

function renderRequirements(): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-requirements";
  section.setAttribute("aria-label", "Requirements");
  const heading = document.createElement("h2");
  heading.textContent = "Requirements";
  const count = document.createElement("span");
  const requirements = [...(host.state?.requirements ?? [])];
  count.textContent = String(requirements.length);
  heading.append(count);
  const list = document.createElement("ul");
  list.className = "workflow-requirements-list";
  if (!requirements.length) {
    const empty = document.createElement("li");
    empty.className = "workflow-requirement empty";
    empty.textContent = "No requirements yet.";
    list.append(empty);
  }
  for (const requirement of requirements) {
    const item = document.createElement("li");
    item.className = "workflow-requirement";
    item.dataset.status = requirement.status;
    item.dataset.kind = requirement.kind;
    const inScene = host.selectedSceneId != null && (requirement.scene_ids ?? []).includes(host.selectedSceneId);
    if (inScene) item.classList.add("in-scene");
    if (inScene && host.requirementBlocks(requirement)) item.classList.add("blocking");
    const kind = document.createElement("span");
    kind.className = "workflow-requirement-kind";
    kind.textContent = host.requirementKindLabel(requirement.kind);
    const name = document.createElement("span");
    name.className = "workflow-requirement-name";
    name.textContent = requirement.name;
    const scenes = document.createElement("span");
    scenes.className = "workflow-requirement-scenes";
    scenes.textContent = (requirement.scene_ids ?? [])
      .map((id) => host.state?.scenes.find((scene) => scene.id === id)?.title ?? `Scene ${id}`)
      .join(", ");
    const status = document.createElement("select");
    status.className = "workflow-requirement-status";
    status.setAttribute("aria-label", `${requirement.name} status`);
    status.disabled = Boolean(host.busy);
    for (const option of REQUIREMENT_STATUSES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === requirement.status;
      status.append(choice);
    }
    status.addEventListener("change", () => {
      void host.run(`Updating ${requirement.name}`, "workflow_set_requirement_status", {
        requirementId: requirement.id,
        status: status.value as RequirementStatus,
      });
    });
    item.append(kind, name, scenes, status);
    list.append(item);
  }
  const add = document.createElement("form");
  add.className = "workflow-requirement-add";
  const nameInput = document.createElement("input");
  nameInput.name = "name";
  nameInput.placeholder = "Add a requirement";
  nameInput.setAttribute("aria-label", "Requirement name");
  const kindInput = document.createElement("select");
  kindInput.name = "kind";
  kindInput.setAttribute("aria-label", "Requirement kind");
  for (const option of REQUIREMENT_KINDS) {
    const choice = document.createElement("option");
    choice.value = option.id;
    choice.textContent = option.label;
    kindInput.append(choice);
  }
  const button = document.createElement("button");
  button.type = "submit";
  button.className = "ghost-button";
  button.textContent = "Add";
  button.disabled = Boolean(host.busy);
  add.append(nameInput, kindInput, button);
  add.addEventListener("submit", (event) => {
    event.preventDefault();
    const name = nameInput.value.trim();
    if (!name) return;
    void host.run(`Adding ${name}`, "workflow_add_requirement", {
      name,
      kind: kindInput.value,
      sceneId: host.selectedSceneId,
    });
  });
  section.append(heading, list, add);
  return section;
}


function paintCharactersStage(): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-characters";
  const characters: CharacterRecord[] = host.state?.characters ?? [];
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-inspector-actions";
  toolbar.append(
    actionButton(newCharacterOpen ? "Cancel new character" : "New character", () => {
      newCharacterOpen = !newCharacterOpen;
      if (!newCharacterOpen) characterDrafts.delete("__new__");
      host.rerender();
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
  queueLookThumbs(list);
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
    const voice = renderCharacterVoice(character);
    if (voice) card.append(voice);
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
      void host.run(
        character ? "Saving character profile…" : "Creating character…",
        "workflow_save_character",
        request,
      ).then((saved) => {
        if (!saved) return;
        characterDrafts.delete(key);
        if (!character) newCharacterOpen = false;
        host.rerender();
      });
    }),
  );
  if (character) {
    const directory = host.resolvePath(character.dir) ?? character.dir;
    actions.append(
      actionButton(character.voice_ready ? "Edit voice" : "Create voice", () => {
        host.hooks?.onOpenVoice(directory);
      }),
      actionButton(
        character.identity_ready ? "Update identity" : "Build identity",
        () => {
          const profilePath = host.resolvePath(character.profile_path);
          const identityBrief = draft.profile.identity_prompt?.trim();
          const prompt = `Build or update the model sheet for ${character.name} in ${character.dir}/model_sheet.${
            identityBrief ? ` Identity brief: ${identityBrief}` : ""
          }`;
          host.hooks?.onComposeChat(prompt, profilePath);
        },
      ),
    );
    if (character.profile_path) {
      actions.append(
        actionButton("Open profile", () => {
          const path = host.resolvePath(character.profile_path);
          if (path) host.hooks?.onOpenPath(path);
        }),
      );
    }
    if (character.model_sheet_manifest_path) {
      actions.append(
        actionButton("Open model sheet manifest", () => {
          const path = host.resolvePath(character.model_sheet_manifest_path);
          if (path) host.hooks?.onOpenPath(path);
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
    image.dataset.lookPath = host.resolvePath(asset.path) ?? asset.path;
    image.addEventListener("click", () => {
      host.hooks?.onOpenPath(host.resolvePath(asset.path) ?? asset.path);
    });
    const caption = document.createElement("figcaption");
    caption.textContent = asset.notes?.trim() || asset.role.replace(/_/g, " ");
    figure.append(image, caption);
    gallery.append(figure);
  }
  section.append(gallery);
  return section;
}

function renderCharacterVoice(character: CharacterRecord): HTMLElement | null {
  const assets = (character.asset_ids ?? [])
    .map(findAsset)
    .filter((asset): asset is AssetRecord => Boolean(asset));
  const sample = assets.find((asset) => asset.role === "voice_sample");
  const profile = assets.find((asset) => asset.role === "voice_profile");
  if (!sample && !profile) return null;
  const line = document.createElement("p");
  line.className = "workflow-field-hint";
  const file = sample?.path.split("/").pop() || profile?.path.split("/").pop() || "voice";
  const detail = profile?.notes?.trim();
  line.textContent = detail ? `Voice · ${file} · ${detail}` : `Voice · ${file}`;
  return line;
}

function paintProductionNotesStage(scene: SceneRecord): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-production-notes";
  const sync = actionButton("Build all production video prompts", () => {
    const chat = host.requireChat();
    if (!chat) return;
    void host.run(
      "Building production video prompts…",
      "workflow_sync_production_notes",
      { sceneId: scene.id, ...chat },
    );
  });
  sync.disabled = Boolean(host.busy);
  const refresh = actionButton("Refresh rendered takes", () => {
    void host.run("Refreshing rendered takes…", "workflow_import", { persist: true });
  });
  refresh.disabled = Boolean(host.busy);
  const intro = document.createElement("section");
  intro.className = "workflow-source-controls";
  intro.append(
    block(
      "workflow-field-hint",
      "Build complete moving-image prompts from the script, scene intent, ordered cut sketches, dialogue, approved scene references, and each cut’s approved visual source. Use this after planning or reference images change.",
    ),
    sync,
    refresh,
  );
  list.append(intro);
  for (const cut of scene.cuts) {
    list.append(renderProductionNoteCard(scene, cut));
  }
  return list;
}

function renderProductionNoteCard(
  scene: SceneRecord,
  cut: CutRecord,
  renderQueue = false,
): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-start-frame-card workflow-production-note-card";
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  const blocker = renderBlocker(cut);
  status.textContent = cut.package_needs_review
    ? blocker
      ? `Needs review · render blocked · ${blocker}`
      : `Needs review · ${cut.takes?.length ?? 0} take(s) kept`
    : blocker
      ? `Render blocked · ${blocker}`
      : `${cut.takes?.length ?? 0} take(s) · ready`;
  header.append(heading, status);
  card.append(header);
  if (cut.package_needs_review) {
    card.append(
      block(
        "workflow-banner warn",
        "Animatic timing changed. This shot package needs review. Existing renders were kept.",
      ),
      actionButton("Mark package reviewed", () => {
        void host.run("Marking shot package reviewed…", "workflow_clear_package_review", {
          sceneId: scene.id,
          cut: cut.id.cut,
        });
      }),
    );
  }
  const dependency = dependencyNote(cut.dependency_state);
  if (dependency) {
    card.append(block("workflow-banner warn", dependency));
  }

  const facts = document.createElement("p");
  facts.className = "workflow-field-hint";
  facts.textContent = [
    roleLabel(cut.sketch?.shot_role),
    speechLabel(cut.speech_mode),
    cut.render_lane?.replace(/_/g, " ") ?? "lane pending",
    visualSourceStatus(cut),
  ].join(" · ");
  card.append(facts);

  if (!renderQueue) {
    const promptLabel = document.createElement("label");
    promptLabel.className = "workflow-field";
    const promptTitle = document.createElement("span");
    promptTitle.textContent = "Production video prompt";
    const prompt = document.createElement("textarea");
    prompt.rows = 10;
    prompt.value = cut.prompt ?? cut.sketch?.visual_prompt ?? "";
    prompt.placeholder =
      "Describe the moving shot, performance or action, camera behavior, continuity, and sound.";
    promptLabel.append(promptTitle, prompt);

    const negativeLabel = document.createElement("label");
    negativeLabel.className = "workflow-field";
    const negativeTitle = document.createElement("span");
    negativeTitle.textContent = "Negative prompt";
    const negative = document.createElement("textarea");
    negative.rows = 3;
    negative.value = cut.negative_prompt ?? cut.sketch?.negative_prompt ?? "";
    negativeLabel.append(negativeTitle, negative);
    card.append(promptLabel, negativeLabel);

    if (cut.dialogue?.length) {
      const dialogue = document.createElement("p");
      dialogue.className = "workflow-production-dialogue";
      dialogue.textContent = cut.dialogue
        .map((line) => `${line.speaker}: “${line.text}”`)
        .join(" ");
      card.append(dialogue);
    }

    const save = actionButton("Save production prompt", () => {
      void host.run("Saving production prompt…", "workflow_save_production_prompt", {
        sceneId: scene.id,
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    });
    save.disabled = Boolean(host.busy) || !cut.production_note_path;
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    const sync = actionButton("Build full video prompt", () => {
      const chat = host.requireChat();
      if (!chat) return;
      void host.run(
        "Building production video prompt…",
        "workflow_sync_production_notes",
        {
          sceneId: scene.id,
          cut: cut.id.cut,
          ...chat,
        },
      );
    });
    sync.disabled = Boolean(host.busy) || !cut.production_note_path || !cut.sketch;
    sync.title =
      "Synthesize a complete video prompt from the script, cut sequence, current sketch, dialogue, and approved visual references.";
    actions.append(sync, save);
    const notePath = host.resolvePath(cut.production_note_path);
    if (notePath) {
      actions.append(actionButton("Open full handoff", () => host.hooks?.onOpenPath(notePath)));
    }
    actions.append(renderCutButton(scene, cut));
    card.append(actions);
  } else {
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    actions.append(renderCutButton(scene, cut));
    card.append(actions);
  }

  const takes = cut.takes ?? [];
  if (takes.length) {
    const takeList = document.createElement("div");
    takeList.className = "workflow-production-takes";
    for (const take of [...takes].reverse()) {
      const preview = document.createElement("figure");
      preview.className = "workflow-production-take";
      const video = document.createElement("video");
      video.controls = true;
      video.preload = "metadata";
      video.playsInline = true;
      video.dataset.takePath = host.resolvePath(take.path) ?? take.path;
      const caption = document.createElement("figcaption");
      caption.textContent = `${take.path.split("/").pop() ?? take.id} · ${take.status.replace(/_/g, " ")}`;
      const open = actionButton(
        "Open in viewer",
        () => host.hooks?.onOpenPath(host.resolvePath(take.path) ?? take.path),
      );
      const reviewActions = document.createElement("div");
      reviewActions.className = "workflow-inspector-actions";
      for (const [status, label] of [
        ["selected", "Select"],
        ["alternate", "Alternate"],
        ["rejected", "Reject"],
        ["needs_edit", "Needs edit"],
        ["needs_rerender", "Needs rerender"],
      ] as const) {
        const review = actionButton(label, () => {
          void confirmTakeStatus(scene, cut, take.id, status);
        });
        review.disabled = Boolean(host.busy) || take.status === status;
        reviewActions.append(review);
      }
      reviewActions.append(
        actionButton("Delete video", () => {
          void deleteProductionTake(scene, cut, take);
        }),
      );
      preview.append(video, caption, open, reviewActions);
      takeList.append(preview);
    }
    card.append(takeList);
    queueTakeVideos(takeList);
  }
  return card;
}

async function bindContinuityFrame(
  scene: SceneRecord,
  cut: CutRecord,
  predecessorCut: string,
  frameTime: string,
): Promise<void> {
  const seconds = Number(frameTime);
  if (!Number.isFinite(seconds) || seconds < 0) {
    await host.ask("Enter a frame time in seconds. The default is near the end of the selected take.", {
      title: "Choose a continuity frame",
      kind: "warning",
    });
    return;
  }
  const predecessor = scene.cuts.find((candidate) => candidate.id.cut === Number(predecessorCut));
  const dependents = predecessor ? host.continuityDependents(predecessor) : [];
  const replacing = Boolean(cut.visual_source?.asset_id);
  if (replacing || dependents.length) {
    const stale = dependents.length
      ? `These shots will need review:\n${dependents.join("\n")}`
      : "This cut's current continuity frame will be replaced.";
    const confirmed = await host.ask(
      `Bind the frame at ${seconds}s?\n\n${stale}\n\nThe successor stays render-blocked until this bind finishes.`,
      { title: `Bind Scene ${scene.id} Cut ${cut.id.cut}`, kind: "warning" },
    );
    if (!confirmed) return;
  }
  await host.run("Binding continuity frame…", "workflow_bind_continuity_frame", {
    sceneId: scene.id,
    cut: cut.id.cut,
    frameTimeSeconds: seconds,
  });
}

async function confirmTakeStatus(
  scene: SceneRecord,
  cut: CutRecord,
  takeId: string,
  status: string,
): Promise<void> {
  if (status === "selected") {
    const dependents = host.continuityDependents(cut).filter((label) => {
      const match = /Scene (\d+) Cut (\d+)/.exec(label);
      if (!match) return true;
      const dependent = host.state?.scenes
        ?.find((item) => item.id === Number(match[1]))
        ?.cuts.find((item) => item.id.cut === Number(match[2]));
      return Boolean(dependent?.visual_source?.asset_id);
    });
    if (dependents.length) {
      const confirmed = await host.ask(
        `Selecting this take unbinds continuity frames on:\n${dependents.join("\n")}\n\nThose shots stay render-blocked until you bind a reviewed frame. This does not approve a replacement.`,
        { title: `Select Scene ${scene.id} Cut ${cut.id.cut}`, kind: "warning" },
      );
      if (!confirmed) return;
    }
  }
  await host.run("Updating take review…", "workflow_set_take_status", {
    sceneId: scene.id,
    cut: cut.id.cut,
    takeId,
    status,
  });
}

async function deleteProductionTake(
  scene: SceneRecord,
  cut: CutRecord,
  take: TakeRecord,
): Promise<void> {
  const confirmed = await host.ask(
    `Delete ${take.path}?\n\nThis permanently removes this video take. If it is selected, dependent continuity frames will be unbound.`,
    { title: `Delete Scene ${scene.id} Cut ${cut.id.cut} take`, kind: "warning" },
  );
  if (!confirmed) return;
  await host.run("Deleting video take…", "workflow_delete_take", {
    sceneId: scene.id,
    cut: cut.id.cut,
    takeId: take.id,
  });
}

function renderCutButton(scene: SceneRecord, cut: CutRecord): HTMLButtonElement {
  const button = actionButton(`Render Cut ${cut.id.cut} new take`, () => {
    void host.run("Preparing render…", "workflow_prepare_render", {
      sceneId: scene.id,
      cut: cut.id.cut,
    });
  });
  const blocker = renderBlocker(cut);
  button.disabled = Boolean(host.busy) || Boolean(blocker);
  button.title = blocker
    ? `Render blocked: ${blocker}.`
    : "Prepare a provider request for approval.";
  return button;
}

function paintRenderStage(scene: SceneRecord): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-production-notes";
  const takes = scene.cuts.reduce((total, cut) => total + (cut.takes?.length ?? 0), 0);
  wrap.append(
    block(
      takes ? "workflow-banner" : "workflow-missing",
      takes
        ? `${takes} take(s) recorded. Render another take here or directly from Production Notes.`
        : "No takes recorded yet. Ready cuts can be rendered here or directly from Production Notes.",
    ),
  );
  for (const cut of scene.cuts) {
    wrap.append(renderProductionNoteCard(scene, cut, true));
  }
  return wrap;
}

function paintAssembleStage(scene: SceneRecord): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-production-notes";
  const missing: number[] = [];
  const selected: Array<{ cut: number; take: TakeRecord }> = [];
  for (const cut of scene.cuts) {
    const take = (cut.takes ?? []).find(
      (candidate) =>
        candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    if (take) selected.push({ cut: cut.id.cut, take });
    else missing.push(cut.id.cut);
  }

  wrap.append(
    block(
      missing.length ? "workflow-missing" : "workflow-banner",
      missing.length
        ? `Select one take for every cut before assembly. Missing: ${missing
            .map((cut) => `Cut ${cut}`)
            .join(", ")}.`
        : `${selected.length} selected takes are ready to assemble in cut order.`,
    ),
  );

  const summary = document.createElement("article");
  summary.className = "workflow-script";
  const heading = document.createElement("h3");
  heading.textContent = `Scene ${scene.id} master`;
  const list = document.createElement("ol");
  list.className = "workflow-readiness";
  for (const cut of scene.cuts) {
    const entry = document.createElement("li");
    const take = selected.find((item) => item.cut === cut.id.cut)?.take;
    entry.className = take ? "ready" : "missing";
    entry.textContent = take
      ? `Cut ${cut.id.cut} · ${take.path.split("/").pop() ?? take.id}`
      : `Cut ${cut.id.cut} · no selected take`;
    list.append(entry);
  }

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  const assemble = actionButton(`Prepare Scene ${scene.id} master`, () => {
    void host.run("Preparing assembly…", "workflow_prepare_assemble", {
      sceneId: scene.id,
    });
  });
  assemble.disabled = Boolean(host.busy) || missing.length > 0 || scene.cuts.length === 0;
  assemble.title = missing.length
    ? "Select a take for every cut in Production Notes first."
    : "Prepare a local ffmpeg assembly for approval. No provider credits are used.";
  const reviewPath = host.reviewExportPath(scene);
  const openReview = actionButton("Open review export", () => {
    if (!reviewPath) return;
    host.hooks?.onOpenPath(host.resolvePath(reviewPath) ?? reviewPath);
  });
  openReview.disabled = Boolean(host.busy) || !reviewPath;
  openReview.title = reviewPath
    ? reviewPath
    : "No review export yet. Prepare an assembly and refresh after the file is written.";
  controls.append(assemble, openReview);
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Assembly joins the complete reviewed takes in cut order, runs locally with ffmpeg after approval, and writes a versioned master to Final/.";
  summary.append(heading, list, controls, note);
  wrap.append(summary, renderPostPanel(scene, missing.length > 0));
  return wrap;
}

const CUT_VERSION_STAGES: Array<{ id: string; label: string }> = [
  { id: "selects", label: "Selects" },
  { id: "assembly", label: "Assembly" },
  { id: "fine_cut", label: "Fine cut" },
  { id: "picture_lock", label: "Picture lock" },
];

const POST_FIELDS: Array<{ id: string; label: string }> = [
  { id: "audio_mix", label: "Audio mix" },
  { id: "vfx", label: "VFX" },
  { id: "compositing", label: "Compositing" },
  { id: "captions", label: "Captions" },
  { id: "mastering", label: "Mastering" },
  { id: "qc", label: "QC" },
];

const POST_STATUSES: Array<{ id: string; label: string }> = [
  { id: "not_started", label: "Not started" },
  { id: "in_progress", label: "In progress" },
  { id: "needs_review", label: "Needs review" },
  { id: "approved", label: "Approved" },
];

function postStatus(post: SceneRecord["post"], field: string): string {
  switch (field) {
    case "audio_mix":
      return post?.audio_mix ?? "not_started";
    case "vfx":
      return post?.vfx ?? "not_started";
    case "compositing":
      return post?.compositing ?? "not_started";
    case "captions":
      return post?.captions ?? "not_started";
    case "mastering":
      return post?.mastering ?? "not_started";
    case "qc":
      return post?.qc ?? "not_started";
    default:
      return "not_started";
  }
}

function renderPostPanel(scene: SceneRecord, takesMissing: boolean): HTMLElement {
  const post = scene.post;
  const article = document.createElement("article");
  article.className = "workflow-script";
  const heading = document.createElement("h3");
  heading.textContent = "Cut versions";
  article.append(heading);

  const versions = post?.versions ?? [];
  if (versions.length === 0) {
    article.append(
      block(
        "workflow-banner",
        "No cut version yet. Marking snapshots the selected takes and leaves those files in place.",
      ),
    );
  }
  for (const version of versions) {
    const row = document.createElement("div");
    row.className = "workflow-field inline";
    const label = document.createElement("span");
    const locked = post?.picture_locked_id === version.id;
    label.textContent = `${version.label} · ${version.shots.length} shots${
      version.assembly_path ? ` · ${version.assembly_path}` : ""
    }${locked ? " · locked" : ""}`;
    const stage = document.createElement("select");
    stage.setAttribute("aria-label", `${version.label} stage`);
    stage.disabled = Boolean(host.busy);
    for (const option of CUT_VERSION_STAGES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === version.stage;
      stage.append(choice);
    }
    stage.addEventListener("change", () => {
      void host.run(`Updating ${version.label}`, "workflow_set_cut_version_stage", {
        sceneId: scene.id,
        versionId: version.id,
        stage: stage.value,
      });
    });
    row.append(label, stage);
    article.append(row);
  }

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  const mark = actionButton("Mark selects as cut version", () => {
    void host.run("Marking cut version…", "workflow_mark_cut_version", { sceneId: scene.id });
  });
  mark.disabled = Boolean(host.busy) || takesMissing || scene.cuts.length === 0;
  mark.title = takesMissing
    ? "Select a take for every cut in Production Notes first."
    : "Snapshot the selected takes as a cut version. Files stay where they are.";
  controls.append(mark);
  const latest = versions[versions.length - 1];
  if (latest && post?.picture_locked_id !== latest.id) {
    const lock = actionButton("Lock picture", () => {
      void host.run("Locking picture…", "workflow_lock_picture", {
        sceneId: scene.id,
        versionId: latest.id,
      });
    });
    controls.append(lock);
  }
  article.append(controls);

  const editorial = document.createElement("div");
  editorial.className = "workflow-field";
  const editorialLabel = document.createElement("span");
  editorialLabel.textContent = "Editorial path";
  const pathSelect = document.createElement("select");
  pathSelect.setAttribute("aria-label", "Editorial path");
  pathSelect.disabled = Boolean(host.busy);
  for (const option of [
    { id: "host_ffmpeg", label: "Host ffmpeg" },
    { id: "external_nle", label: "External NLE" },
  ]) {
    const choice = document.createElement("option");
    choice.value = option.id;
    choice.textContent = option.label;
    choice.selected = option.id === (post?.editorial_path ?? "host_ffmpeg");
    pathSelect.append(choice);
  }
  const noteInput = document.createElement("input");
  noteInput.type = "text";
  noteInput.placeholder = "Why this edit leaves the host";
  noteInput.setAttribute("aria-label", "External NLE note");
  noteInput.value = post?.external_nle_note ?? "";
  noteInput.disabled = Boolean(host.busy);
  const saveEditorial = actionButton("Record editorial path", () => {
    void host.run("Recording editorial path…", "workflow_set_editorial_path", {
      sceneId: scene.id,
      path: pathSelect.value,
      note: noteInput.value,
    });
  });
  editorial.append(editorialLabel, pathSelect, noteInput, saveEditorial);
  const editorialHint = document.createElement("p");
  editorialHint.className = "workflow-field-hint";
  editorialHint.textContent =
    "Host ffmpeg stays the assembly above. Recording an external NLE stores that decision and leaves the cut version and take files in place.";
  article.append(editorial, editorialHint);

  const statuses = document.createElement("div");
  statuses.className = "workflow-post-statuses";
  for (const field of POST_FIELDS) {
    const row = document.createElement("label");
    row.className = "workflow-field inline";
    row.textContent = field.label;
    const select = document.createElement("select");
    select.setAttribute("aria-label", field.label);
    select.disabled = Boolean(host.busy);
    const current = postStatus(post, field.id);
    for (const option of POST_STATUSES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === current;
      select.append(choice);
    }
    select.addEventListener("change", () => {
      void host.run(`Updating ${field.label}`, "workflow_set_post_status", {
        sceneId: scene.id,
        field: field.id,
        status: select.value,
      });
    });
    row.append(select);
    statuses.append(row);
  }
  article.append(statuses);

  const deliverables = host.state?.profile?.deliverables ?? [];
  const checkRow = document.createElement("div");
  checkRow.className = "workflow-inspector-actions";
  if (deliverables.length === 0) {
    article.append(
      block("workflow-banner", "Add a named deliverable on the profile before a QC check."),
    );
  } else {
    const deliverableSelect = document.createElement("select");
    deliverableSelect.setAttribute("aria-label", "Deliverable");
    deliverableSelect.disabled = Boolean(host.busy);
    for (const deliverable of deliverables) {
      const choice = document.createElement("option");
      choice.value = deliverable.id;
      const spec = [deliverable.resolution, deliverable.aspect_ratio].filter(Boolean).join(" ");
      choice.textContent = spec ? `${deliverable.name} · ${spec}` : deliverable.name;
      choice.selected = post?.last_check?.deliverable_id === deliverable.id;
      deliverableSelect.append(choice);
    }
    const check = actionButton("Check deliverable", () => {
      void host.run("Checking deliverable…", "workflow_check_deliverable", {
        sceneId: scene.id,
        deliverableId: deliverableSelect.value,
      });
    });
    checkRow.append(deliverableSelect, check);
    article.append(checkRow);
  }
  const last = post?.last_check;
  if (last) {
    article.append(
      block(
        last.passed ? "workflow-banner" : "workflow-missing",
        `${last.passed ? "Passed" : "Failed"}. ${(last.messages ?? []).join(" ")}`,
      ),
    );
  }
  return article;
}


function paintTakeReviewStage(scene: SceneRecord): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-take-review";
  const refresh = actionButton("Refresh takes from project", () => {
    void host.run("Refreshing takes…", "workflow_import", { persist: true });
  });
  wrap.append(
    block(
      "workflow-banner",
      "A generated clip is a take until you select it. Changing a selected take unbinds dependent continuity frames. It does not approve a replacement frame.",
    ),
    refresh,
  );
  let count = 0;
  for (const cut of scene.cuts) {
    const takes = cut.takes ?? [];
    if (!takes.length) continue;
    count += takes.length;
    const card = document.createElement("article");
    card.className = "workflow-start-frame-card";
    const heading = document.createElement("h3");
    heading.textContent = `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
    card.append(heading);
    for (const take of takes) {
      const row = document.createElement("div");
      row.className = "workflow-take-row";
      const summary = document.createElement("button");
      summary.type = "button";
      summary.className = "workflow-take-path";
      summary.textContent = `${take.path} · ${take.status.replace(/_/g, " ")}`;
      summary.addEventListener("click", () => {
        host.hooks?.onOpenPath(host.resolvePath(take.path) ?? take.path);
      });
      const actions = document.createElement("div");
      actions.className = "workflow-inspector-actions";
      for (const [status, label] of [
        ["selected", "Select"],
        ["alternate", "Alternate"],
        ["rejected", "Reject"],
        ["needs_edit", "Needs edit"],
        ["needs_rerender", "Needs rerender"],
      ] as const) {
        const button = actionButton(label, () => {
          void confirmTakeStatus(scene, cut, take.id, status);
        });
        button.disabled = Boolean(host.busy) || take.status === status;
        actions.append(button);
      }
      row.append(summary, actions);
      card.append(row);
    }
    wrap.append(card);
  }
  if (!count) {
    wrap.append(
      missingPanel(
        "No takes to review.",
        "Generate clips under Scenes/SceneN/CutN, then refresh this stage. The importer registers video files as unreviewed takes.",
      ),
    );
  }
  return wrap;
}

function paintReadiness(
  scene: SceneRecord | undefined,
  sceneStatus: SceneReport | undefined,
): HTMLElement {
  const list = document.createElement("ul");
  list.className = "workflow-readiness";
  const rows: [boolean, string][] = [
    [scene?.stages.script === "present", "Script beats"],
    [
      (host.state?.characters.length ?? 0) > 0,
      `${host.state?.characters.length ?? 0} characters (${host.report?.identity_ready ?? 0} identity-ready)`,
    ],
    [Boolean(sceneStatus?.scene_intent), "Scene intent"],
    [Boolean(sceneStatus?.scene_look), "Scene look board"],
    [Boolean(sceneStatus?.cut_sketches), "Cut sketches"],
    [
      (sceneStatus?.planned_visual_sources ?? 0) === (scene?.cuts.length ?? 0),
      `${sceneStatus?.planned_visual_sources ?? 0}/${scene?.cuts.length ?? 0} visual-source strategies planned`,
    ],
    [
      (sceneStatus?.render_ready_sources ?? 0) === (scene?.cuts.length ?? 0),
      `${sceneStatus?.render_ready_sources ?? 0}/${scene?.cuts.length ?? 0} concrete render sources ready`,
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



export function renderIntentStage(scene: SceneRecord): HTMLElement {
  const model = buildIntentModel({
    scene,
    busy: host.busy,
    error: host.error,
    chatReady: host.chatReady,
    blocked: host.blockerNames(scene.id),
  });
  return presentSurface(paintIntentStage(scene), model, true);
}

export function renderReferenceImagesStage(scene: SceneRecord, sceneStatus: SceneReport | undefined): HTMLElement {
  const model = buildReferenceModel({
    scene,
    report: sceneStatus,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
  });
  return presentSurface(paintReferenceImagesStage(scene, sceneStatus), model, true);
}

export function renderSketchStage(scene: SceneRecord, sceneStatus: SceneReport | undefined): HTMLElement {
  const model = buildSketchModel({
    scene,
    report: sceneStatus,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
    chatReady: host.chatReady,
  });
  const view = paintSketchStage(scene, sceneStatus);
  if (model.emptyTitle && model.emptyDetail && !view.querySelector(".workflow-missing")) {
    view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  }
  return presentSurface(view, model, Boolean(model.banner));
}

export function renderProductionNotesStage(scene: SceneRecord): HTMLElement {
  const model = buildNotesModel({
    scene,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
    chatReady: host.chatReady,
  });
  const view = paintProductionNotesStage(scene);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, false);
}

export function renderRenderStage(scene: SceneRecord): HTMLElement {
  const model = buildRenderQueueModel({
    scene,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
  });
  const view = paintRenderStage(scene);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, true);
}

export function renderTakeReviewStage(scene: SceneRecord): HTMLElement {
  const model = buildTakeReviewModel({
    scene,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
  });
  return presentSurface(paintTakeReviewStage(scene), model, true);
}

export function renderAssembleStage(scene: SceneRecord): HTMLElement {
  const model = buildAssembleModel({
    scene,
    busy: host.busy,
    error: host.error,
    blocked: host.blockerNames(scene.id),
    reviewPath: host.reviewExportPath(scene),
  });
  const view = paintAssembleStage(scene);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, true);
}

export function renderCharactersStage(): HTMLElement {
  const model = buildCharactersModel({
    characters: host.state?.characters ?? [],
    creating: newCharacterOpen,
    busy: host.busy,
    error: host.error,
  });
  return presentSurface(paintCharactersStage(), model, false);
}

export function renderProjectProfile(): HTMLElement {
  const profile = host.state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bits = [
    deliverable?.name || host.state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  const model = buildProfileModel({
    projectName: host.state?.project_name ?? null,
    recipeLabel: host.activeRecipe()?.label ?? null,
    summary: bits.join(" · "),
    requirementCount: host.state?.requirements?.length ?? 0,
    busy: host.busy,
    error: host.error,
  });
  return presentSurface(paintProjectProfile(), model, false);
}

export function renderReadiness(scene: SceneRecord | undefined, sceneStatus: SceneReport | undefined): HTMLElement {
  return paintReadiness(scene, sceneStatus);
}
