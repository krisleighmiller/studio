import { ask, invoke, openDialog, toMediaUrlAsync } from "./host";
import {
  REQUIREMENT_KINDS,
  type AssetRecord,
  type CutRecord,
  type LookSlot,
  type ProjectReport,
  type ProjectState,
  type RecipeView,
  type RequirementKind,
  type RequirementRecord,
  type SceneRecord,
  type SceneReport,
  type StageId,
  type WorkflowImportResult,
} from "./workflowTypes";
import { buildSceneList, buildSceneLook, sceneSelectionKey } from "./studio/sceneLookModel";
import { renderSceneListView, renderSceneLookView, type SceneLookIntent } from "./studio/sceneLookView";
import { buildJobsModel, type JobFilter } from "./studio/jobsModel";
import { renderJobsView, type JobsIntent } from "./studio/jobsView";
import { buildFinishModel, latestReviewExport } from "./studio/finishModel";
import { renderFinishView, type FinishIntent } from "./studio/finishView";
import {
  stagesFromRecipe,
  surfaceBanner,
  surfacesForArea,
  workAreaForStage,
  type DeskStage,
  type SurfaceId,
  type WorkArea,
} from "./studio/surfaces";
import { renderSurfaceNav } from "./studio/surfaceNav";
import { createStudioController } from "./studio/controller";
import { visibleDirectorSelection } from "./studio/selection";
import {
  describeDirectorContext,
  type DirectorContextEntry,
} from "./studio/selectors/directorContext";
import { requirementBlocks } from "./studio/selectors/project";
import { buildStudioView } from "./studio/viewModel";
import { renderProjectSummary } from "./studio/components/projectSummary";
import { renderWorkspaceStatus } from "./studio/components/workspaceStatus";
import { mixLabel, mouthLabel, roleLabel, speechLabel } from "./studio/labels";
import { bindSurfaceHost, clearIntentDraft, effectiveVisualSourceStrategy, renderAssembleStage, renderCharactersStage, renderIntentStage, renderProductionNotesStage, renderProjectProfile, renderReadiness, renderReferenceImagesStage, renderRenderStage, renderSketchStage, renderTakeReviewStage, resetSurfaceSession, setProfileOpen, stopRoughCut, visualSourceReady } from "./studio/surfaces/productionSurfaces";

export type WorkflowSelection = {
  sceneId: number | null;
  cut: number | null;
  path: string | null;
  excerpt: string | null;
  stage: StageId | null;
  sceneStableId: string | null;
  shotStableId: string | null;
  assetId: string | null;
  lookSlotId: string | null;
  requirementId: string | null;
  workArea: "plan" | "produce" | "finish" | null;
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
  requirementsTop: number;
};

let hooks: DeskHooks | null = null;
let state: ProjectState | null = null;
let report: ProjectReport | null = null;
let loadError: string | null = null;
let selectedSceneId: number | null = null;
let selectedSceneKey: string | null = null;
let selectedCut: number | null = null;
let selectedShotStableId: string | null = null;
let selectedLookSlotId: string | null = null;
const suppressedDirectorScope = new Set<string>();
let focusedLookSlotId: string | null = null;
let focusedLookAssetId: string | null = null;
let selectedRequirementId: string | null = null;
let selectedStage: StageId = "production_notes";
let selectedSurface: SurfaceId = "production_notes";
let detailsOpen = false;
let jobsOpen = true;
let jobsFilter: JobFilter | "all" = "all";
let busy: string | null = null;
let actionNote: string | null = null;

export function bindWorkflowDesk(next: DeskHooks): void {
  hooks = next;
}

export function workflowSelection(): WorkflowSelection {
  const scene = selectedScene();
  const cut = selectedCutRecord();
  const lookSlot = selectedLookSlot(scene);
  const requirement = selectedRequirement();
  return {
    sceneId: selectedSceneId,
    cut: selectedCut,
    path: resolveProjectPath(cut?.production_note_path ?? scene?.source_path ?? null),
    excerpt: selectionExcerpt(scene, cut, sceneReport(selectedSceneId)),
    stage: selectedStage,
    sceneStableId: scene?.stable_id || null,
    shotStableId: cut?.stable_id || null,
    assetId: directorAssetId(cut, lookSlot),
    lookSlotId: lookSlot?.id ?? null,
    requirementId: requirement?.id ?? null,
    workArea: workAreaFor(selectedStage),
  };
}

function directorScopeKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}

export type { DirectorContextEntry };

/** Selection actually sent with the next Director turn. */
export function directorTurnSelection(): WorkflowSelection {
  const selection = workflowSelection();
  if (!state) {
    return {
      ...selection,
      sceneId: null,
      cut: null,
      sceneStableId: null,
      shotStableId: null,
      assetId: null,
      lookSlotId: null,
      requirementId: null,
      workArea: null,
      stage: null,
    };
  }
  return { ...selection, ...visibleDirectorSelection(selection, suppressedDirectorScope) };
}

function recipeStageLabel(stageId: string | null): string | null {
  if (!stageId) return null;
  return activeStages().find((stage) => stage.id === stageId)?.label ?? null;
}

export function directorContextEntries(): DirectorContextEntry[] {
  const selection = directorTurnSelection();
  return describeDirectorContext(state, selection, recipeStageLabel(selection.stage)).entries;
}

export function omittedDirectorEntries(): DirectorContextEntry[] {
  if (!state) return [];
  const sent = new Set(directorContextEntries().map((entry) => directorScopeKey(entry.kind, entry.id)));
  const selection = workflowSelection();
  return describeDirectorContext(state, selection, recipeStageLabel(selection.stage)).entries.filter(
    (entry) => !sent.has(directorScopeKey(entry.kind, entry.id)),
  );
}

export function detachDirectorContext(kind: DirectorContextEntry["kind"], id: string): void {
  suppressedDirectorScope.add(directorScopeKey(kind, id));
  if (kind === "lookSlot" && selectedLookSlotId === id) selectedLookSlotId = null;
  if (kind === "requirement" && selectedRequirementId === id) selectedRequirementId = null;
  renderWorkflowDesk();
  emitSelection();
}

export function attachDirectorContext(kind: DirectorContextEntry["kind"], id: string): void {
  suppressedDirectorScope.delete(directorScopeKey(kind, id));
  if (kind === "lookSlot") {
    selectedLookSlotId = id;
    focusedLookSlotId = id;
  }
  if (kind === "requirement") selectedRequirementId = id;
  renderWorkflowDesk();
  emitSelection();
}

export function directorScopeHeadline(): string {
  const selection = directorTurnSelection();
  return describeDirectorContext(state, selection, recipeStageLabel(selection.stage)).headline;
}

export function directorScopeText(): string {
  const selection = directorTurnSelection();
  return describeDirectorContext(state, selection, recipeStageLabel(selection.stage)).scopeText;
}

export async function refreshWorkflowDesk(): Promise<void> {
  const root = hooks?.projectRoot();
  if (!root || !state) return;
  // Provider actions can create new stills, audio, and video files. Re-scan
  // instead of merely reloading the previous state so completed outputs appear
  // in the desk immediately after the Director finishes.
  const result = await invoke<WorkflowImportResult>("workflow_import", {
    projectRoot: root,
    persist: true,
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
    useStage("scene_intent");
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
  selectedSceneKey = null;
  selectedCut = null;
  selectedShotStableId = null;
  selectedLookSlotId = null;
  suppressedDirectorScope.clear();
  focusedLookSlotId = null;
  focusedLookAssetId = null;
  selectedRequirementId = null;
  selectedStage = "scene_intent";
  selectedSurface = "scene_intent";
  detailsOpen = false;
  busy = null;
  actionNote = null;
  resetSurfaceSession();
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
    rememberScene(result.state.scenes[0] ?? null);
    useStage(defaultStage(selectedScene()));
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
  clearIntentDraft();
  const scene = selectedScene();
  if (!scene) rememberScene(result.state.scenes[0] ?? null);
  else if (!selectedCutRecord()) rememberScene(scene, scene.cuts[0] ?? null);
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
  const controller = createStudioController({
    projectRoot: () => hooks?.projectRoot() ?? null,
    beforeRun: () => stopRoughCut(),
    onBusy: (next) => {
      busy = next;
      if (next) actionNote = null;
      renderWorkflowDesk();
    },
    onResult: (result) => {
      applyHostResult(result);
      if (result.pendingAction) hooks?.onPendingPlan?.(result.pendingAction);
    },
    onError: (message) => {
      actionNote = message;
      renderWorkflowDesk();
      hooks?.onStatus("Production", actionNote);
    },
  });
  return controller.run({ label, cmd, args });
}

export function openJobsDrawer(): void {
  jobsOpen = true;
  renderWorkflowDesk();
}

function syncSurfaceHost(): void {
  bindSurfaceHost({
    busy,
    error: actionNote,
    chatReady: Boolean(hooks?.chatModel()?.modelId),
    state,
    report,
    selectedCut,
    selectedSceneId,
    selectedRequirementId,
    run: runAction,
    requireChat,
    rerender: () => renderWorkflowDesk(),
    setError: (message) => {
      actionNote = message;
    },
    rememberScene: (scene, cut) => rememberScene(scene, cut),
    recipeRequires,
    resolvePath: resolveProjectPath,
    ask,
    pickFile: openDialog,
    mediaUrl: toMediaUrlAsync,
    continuityDependents,
    reviewExportPath,
    activeRecipe,
    requirementKindLabel,
    requirementBlocks,
    blockerNames: (sceneId) => sceneBlockers(sceneId).map((item) => item.name),
    hooks,
  });
}

export function renderWorkflowDesk(): void {
  syncSurfaceHost();
  const root = hooks?.root;
  if (!root) {
    renderFinishMount();
    return;
  }
  const viewport = captureWorkflowViewport(root);
  root.replaceChildren();
  if (loadError) {
    root.append(block("workflow-error", `Could not import workflow state.\n${loadError}`));
    renderFinishMount();
    return;
  }
  if (!state || !report) {
    root.append(
      block(
        "empty-state center",
        "Open a project folder to see scenes, cuts, and missing preproduction artifacts.",
      ),
    );
    renderFinishMount();
    return;
  }

  ensureSurface();
  const desk = document.createElement("div");
  desk.className = "workflow-desk";
  desk.dataset.sceneId = selectedSceneId?.toString() ?? "";
  desk.dataset.stage = selectedSurface;
  if (busy) desk.classList.add("busy");
  const head = document.createElement("div");
  if (actionNote) head.append(block(noteClass(actionNote), actionNote));
  if (busy) head.append(block("workflow-banner", busy));
  desk.append(head, renderBody());
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
    requirementsTop:
      root.querySelector<HTMLElement>(".workflow-requirements-list")?.scrollTop ?? 0,
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
  const requirements = root.querySelector<HTMLElement>(".workflow-requirements-list");
  if (requirements) requirements.scrollTop = viewport.requirementsTop;
}

function noteClass(note: string): string {
  const lower = note.toLowerCase();
  if (lower.includes("reject")) return "workflow-banner reject";
  if (lower.includes("warning") || lower.includes("warn")) return "workflow-banner warn";
  return "workflow-banner";
}

function defaultStage(scene: SceneRecord | undefined): StageId {
  const stages = activeStages();
  const offered = (id: string) => stages.some((stage) => stage.id === id && stage.available);
  const fallback = stages.find((stage) => stage.available)?.id ?? stages[0]?.id ?? "script";
  if (!scene) return fallback;
  const intentMissing = scene.stages.scene_intent === "missing" || !scene.intent;
  if ((intentMissing || !scene.intent_approved) && offered("scene_intent")) return "scene_intent";
  const lookMissing =
    scene.stages.scene_look === "missing" ||
    !scene.look_board?.slots?.length ||
    scene.look_board.slots.every((slot) => !slot.approved_asset_id);
  if (lookMissing && offered("scene_look")) return "scene_look";
  if (scene.stages.cut_sketches === "missing" && offered("cut_sketches")) return "cut_sketches";
  if (scene.stages.production_notes === "present" && offered("production_notes")) return "production_notes";
  if (scene.stages.script === "present" && offered("script")) return "script";
  return fallback;
}

function rememberScene(scene: SceneRecord | null, cut?: CutRecord | null): void {
  if (!scene) {
    selectedSceneId = null;
    selectedSceneKey = null;
    selectedCut = null;
    selectedShotStableId = null;
    return;
  }
  selectedSceneId = scene.id;
  selectedSceneKey = sceneSelectionKey(scene);
  const chosen =
    cut === undefined
      ? scene.cuts.find((item) => item.stable_id && item.stable_id === selectedShotStableId) ??
        scene.cuts[0] ??
        null
      : cut;
  selectedCut = chosen?.id.cut ?? null;
  selectedShotStableId = chosen?.stable_id || null;
}

function selectedScene(): SceneRecord | undefined {
  if (!state) return undefined;
  if (selectedSceneKey) {
    const match = state.scenes.find((scene) => sceneSelectionKey(scene) === selectedSceneKey);
    if (match) {
      selectedSceneId = match.id;
      return match;
    }
  }
  return state.scenes.find((scene) => scene.id === selectedSceneId);
}

function selectedCutRecord(): CutRecord | undefined {
  const scene = selectedScene();
  if (!scene) return undefined;
  if (selectedShotStableId) {
    const match = scene.cuts.find((cut) => cut.stable_id === selectedShotStableId);
    if (match) {
      selectedCut = match.id.cut;
      return match;
    }
  }
  const byNumber = scene.cuts.find((cut) => cut.id.cut === selectedCut);
  if (byNumber) selectedShotStableId = byNumber.stable_id || null;
  return byNumber;
}

function workAreaFor(stage: StageId | null): "plan" | "produce" | "finish" | null {
  return workAreaForStage(stage, activeStages());
}

function useStage(stage: StageId): void {
  selectedStage = stage;
  selectedSurface = stage;
}

function ensureSurface(): void {
  if (selectedSurface === "profile") return;
  const stages = activeStages();
  const current = stages.find((stage) => stage.id === selectedStage);
  if (!current || !current.available) {
    const next = stages.find((stage) => stage.available) ?? stages[0];
    useStage(next?.id ?? "script");
    return;
  }
  selectedSurface = selectedStage;
}

export function setStudioWorkArea(area: WorkArea): void {
  const current = workAreaFor(selectedStage);
  if (current === area) return;
  const stages = activeStages();
  const next = stages.find((stage) => stage.workArea === area && stage.available) ?? stages.find((stage) => stage.workArea === area);
  if (!next) {
    if (area === "plan") {
      selectedSurface = "profile";
      setProfileOpen(true);
      renderWorkflowDesk();
      emitSelection();
    }
    return;
  }
  useStage(next.id);
  renderWorkflowDesk();
  emitSelection();
}

function selectedLookSlot(scene: SceneRecord | undefined): LookSlot | undefined {
  if (!scene || !selectedLookSlotId) return undefined;
  return scene.look_board?.slots?.find((slot) => slot.id === selectedLookSlotId);
}

function selectedRequirement(): RequirementRecord | undefined {
  if (!selectedRequirementId) return undefined;
  return state?.requirements?.find((item) => item.id === selectedRequirementId);
}

function directorAssetId(
  cut: CutRecord | undefined,
  lookSlot: LookSlot | undefined,
): string | null {
  if (cut?.visual_source?.asset_id) return cut.visual_source.asset_id;
  if (
    cut?.selected_take &&
    state?.assets?.some((asset) => asset.id === cut.selected_take)
  ) {
    return cut.selected_take;
  }
  return lookSlot?.approved_asset_id ?? null;
}

function continuityDependents(predecessor: CutRecord): string[] {
  const labels: string[] = [];
  for (const scene of state?.scenes ?? []) {
    for (const cut of scene.cuts) {
      const source = cut.visual_source;
      if (
        !source ||
        source.strategy !== "previous_selected_take_exit_frame" ||
        !source.asset_id
      ) {
        continue;
      }
      const linked =
        (source.predecessor?.source_scene === predecessor.id.source_scene &&
          source.predecessor.cut === predecessor.id.cut) ||
        Boolean(
          predecessor.stable_id &&
            source.predecessor_shot_id === predecessor.stable_id,
        );
      if (linked) labels.push(`Scene ${scene.id} Cut ${cut.id.cut}`);
    }
  }
  return labels;
}

function reviewExportPath(scene: SceneRecord): string | null {
  return latestReviewExport(scene, state?.jobs ?? []);
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

function sceneBlockers(sceneId: number): RequirementRecord[] {
  return (state?.requirements ?? []).filter(
    (requirement) =>
      (requirement.scene_ids ?? []).includes(sceneId) && requirementBlocks(requirement),
  );
}

function activeRecipe(): RecipeView | null {
  return report?.recipe ?? null;
}

function activeStages(): DeskStage[] {
  const scene = selectedScene();
  return stagesFromRecipe({
    navigation: sceneReport(scene?.id ?? null)?.navigation,
    recipeStages: activeRecipe()?.stages,
    scene: scene ?? null,
  });
}

function recipeRequires(role: string): boolean {
  return (activeRecipe()?.stages ?? []).some((stage) => stage.required_roles.includes(role));
}

function requirementKindLabel(kind: RequirementKind): string {
  return REQUIREMENT_KINDS.find((item) => item.id === kind)?.label ?? kind;
}

function renderBody(): HTMLElement {
  const body = document.createElement("div");
  body.className = `workflow-body${jobsOpen ? "" : " jobs-collapsed"}`;
  body.classList.add("scene-look-body");
  const scenes = renderSceneList();
  scenes.classList.add("studio-scenes-list");
  body.append(scenes, renderJobsPanel(), renderMain());
  renderFinishMount();
  return body;
}

function renderFinishMount(): void {
  const mount = document.querySelector("#studio-finish-mount");
  if (!mount) return;
  mount.replaceChildren(renderFinishPanel());
}

function currentStudioView() {
  if (!state) return null;
  return buildStudioView({
    state,
    report,
    selection: directorTurnSelection(),
    scene: selectedScene() ?? null,
    cut: selectedCutRecord() ?? null,
    stage: selectedStage,
    workArea: workAreaFor(selectedStage),
    lookSlotId: selectedLookSlotId,
    jobFilter: jobsFilter,
    busy,
    resolvePath: resolveProjectPath,
  });
}

function renderSceneList(): HTMLElement {
  const selected = selectedScene();
  const blockerCounts = new Map<number, number>();
  for (const scene of state?.scenes ?? []) {
    blockerCounts.set(scene.id, sceneBlockers(scene.id).length);
  }
  const aside = renderSceneListView(
    buildSceneList({
      scenes: state?.scenes ?? [],
      reports: report?.scenes ?? [],
      assets: state?.assets ?? [],
      selectedKey: selected ? sceneSelectionKey(selected) : null,
      blockerCounts,
      resolvePath: resolveProjectPath,
    }),
    (key) => {
      const scene = state?.scenes.find((item) => sceneSelectionKey(item) === key);
      if (!scene) return;
      rememberScene(scene, scene.cuts[0] ?? null);
      useStage(defaultStage(scene));
      focusedLookSlotId = null;
      focusedLookAssetId = null;
      renderWorkflowDesk();
      emitSelection();
    },
  );
  const studio = currentStudioView();
  if (studio) {
    aside.querySelector("h2")?.after(renderProjectSummary(studio.project, studio.scenes));
    for (const button of aside.querySelectorAll<HTMLButtonElement>("button[data-scene-key]")) {
      const summary = studio.scenes.find((item) => item.key === button.dataset.sceneKey);
      if (summary) button.title = summary.detail;
    }
  }
  queueLookThumbs(aside);
  return aside;
}

function showSurface(id: SurfaceId): void {
  selectedSurface = id;
  if (id !== "profile") selectedStage = id;
  if (id === "profile") setProfileOpen(true);
  renderWorkflowDesk();
  emitSelection();
}

function renderMain(): HTMLElement {
  const main = document.createElement("section");
  main.className = "workflow-main";
  main.setAttribute("aria-label", "Workspace");
  const scene = selectedScene();
  const area = workAreaFor(selectedStage) ?? "plan";
  main.append(
    renderSurfaceNav(
      surfacesForArea({
        area,
        stages: activeStages(),
        scene: scene ?? null,
        active: selectedSurface,
      }),
      showSurface,
    ),
  );
  const studio = currentStudioView();
  if (studio) main.append(renderWorkspaceStatus(studio.workspace));
  const blockers = scene ? sceneBlockers(scene.id) : [];
  const currentStage = activeStages().find((stage) => stage.id === selectedSurface);
  const banner = surfaceBanner({
    surface: selectedSurface,
    hasScene: Boolean(scene),
    blockers: blockers.map((item) => item.name),
    busy,
    disabledReason: currentStage?.disabledReason,
  });
  if (banner?.tone === "empty") {
    main.append(block("empty-state", banner.text));
    return main;
  }
  if (banner?.tone === "blocked") {
    const notice = document.createElement("div");
    notice.className = "workflow-banner warn";
    notice.textContent = banner.text;
    if (!currentStage?.disabledReason) {
      for (const item of blockers) {
        const attach = actionButton(
          selectedRequirementId === item.id ? `${item.name} attached` : `Attach ${item.name}`,
          () => {
            if (selectedRequirementId === item.id) detachDirectorContext("requirement", item.id);
            else attachDirectorContext("requirement", item.id);
          },
        );
        attach.disabled = Boolean(busy);
        notice.append(document.createTextNode(" "), attach);
      }
    }
    main.append(notice);
  }
  if (selectedSurface === "profile") {
    main.append(renderProjectProfile());
    return main;
  }
  const sceneStatus = sceneReport(selectedSceneId);
  if (scene && sceneStatus) {
    const summary = document.createElement("p");
    summary.className = "studio-scene-line";
    summary.textContent = `${scene.title} · ${mixLabel(sceneStatus.mix)} · ${sceneStatus.approved_start_frames} start frames`;
    main.append(summary);
  }
  if (sceneStatus?.effectively_performance_heavy && selectedSurface === "production_notes") {
    main.append(
      block(
        "workflow-banner warn",
        "This mix is effectively performance-heavy for a cinematic-balanced request.",
      ),
    );
  }
  if (selectedSurface === "scene_look" && scene) main.append(renderSceneLookWorkspace(scene));
  else if (selectedSurface === "characters") main.append(renderCharactersStage());
  else main.append(renderStageContent(scene, sceneStatus));
  if (!scene) return main;
  const details = actionButton(detailsOpen ? "Hide details" : "Details", () => {
    detailsOpen = !detailsOpen;
    renderWorkflowDesk();
  });
  main.append(details);
  if (detailsOpen) main.append(renderInspector());
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
        wrap.append(renderProductionNotesStage(scene));
      }
      break;
    case "render":
      wrap.append(renderRenderStage(scene));
      break;
    case "take_review":
      wrap.append(renderTakeReviewStage(scene));
      break;
    case "assemble":
      wrap.append(renderAssembleStage(scene));
      break;
    case "scene_look":
      break;
    default:
      wrap.append(renderRecipeStage(selectedStage));
      break;
  }
  return wrap;
}

function renderRecipeStage(stageId: string): HTMLElement {
  const stage = activeStages().find((item) => item.id === stageId);
  const wrap = document.createElement("div");
  wrap.className = "workflow-recipe-stage";
  const title = document.createElement("h2");
  title.textContent = stage?.label ?? stageId;
  const copy = document.createElement("p");
  copy.className = stage?.disabledReason ? "workflow-banner warn" : "empty-state";
  copy.textContent = stage?.disabledReason ?? `${title.textContent} is part of this recipe. This build tracks it and does not have a separate editor for it yet.`;
  wrap.append(title, copy);
  return wrap;
}

function renderSceneLookWorkspace(scene: SceneRecord): HTMLElement {
  const slot = scene.look_board?.slots?.find((item) => item.id === focusedLookSlotId)
    ?? scene.look_board?.slots?.find((item) => item.id === selectedLookSlotId)
    ?? scene.look_board?.slots?.[0];
  const workspace = renderSceneLookView(
    buildSceneLook({
      scene,
      report: sceneReport(scene.id),
      assets: state?.assets ?? [],
      jobs: state?.jobs ?? [],
      characters: (state?.characters ?? []).map((character) => ({ id: character.id, name: character.name })),
      blockers: sceneBlockers(scene.id).map((requirement) => requirement.name),
      focusedSlotId: focusedLookSlotId,
      focusedAssetId: focusedLookAssetId,
      directorSlotId: selectedLookSlotId,
      identityHint: slot ? lookIdentityHint(scene, slot) : null,
      busy,
      resolvePath: resolveProjectPath,
    }),
    (intent) => handleSceneLookIntent(scene, intent),
  );
  queueLookThumbs(workspace);
  return workspace;
}

function handleSceneLookIntent(scene: SceneRecord, intent: SceneLookIntent): void {
  const slot = scene.look_board?.slots?.find((item) => item.id === ("slotId" in intent ? intent.slotId : focusedLookSlotId));
  switch (intent.type) {
    case "select-slot":
      focusedLookSlotId = intent.slotId;
      focusedLookAssetId = null;
      renderWorkflowDesk();
      break;
    case "select-version":
      focusedLookAssetId = intent.assetId;
      renderWorkflowDesk();
      break;
    case "draft-board": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting scene look…", "workflow_draft_look", { sceneId: scene.id, ...chat });
      break;
    }
    case "save-prompt":
      void runAction("Saving reference prompt…", "workflow_update_look_slot", {
        sceneId: scene.id,
        slotId: intent.slotId,
        role: intent.role,
        title: intent.title,
        prompt: intent.prompt,
        negativePrompt: intent.negative,
      });
      break;
    case "attach":
      void attachLookImage(scene.id, intent.slotId);
      break;
    case "generate":
      void runAction("Preparing look generate…", "workflow_generate_look", {
        sceneId: scene.id,
        slotId: intent.slotId,
        role: intent.role,
        title: intent.title,
        prompt: intent.prompt,
        negativePrompt: intent.negative,
      });
      break;
    case "approve":
      focusedLookAssetId = intent.assetId;
      void runAction("Approving look still…", "workflow_approve_look", {
        sceneId: scene.id,
        slotId: intent.slotId,
        assetId: intent.assetId,
      });
      break;
    case "reject":
      void runAction("Rejecting look still…", "workflow_reject_look", {
        sceneId: scene.id,
        slotId: intent.slotId,
        assetId: intent.assetId,
      });
      break;
    case "delete": {
      if (!slot) return;
      const asset = findLookAsset(slot, intent.assetId);
      if (!asset) return;
      void deleteLookImage(scene, slot, asset);
      break;
    }
    case "open":
      hooks?.onOpenPath(intent.path);
      break;
    case "attach-director":
      if (selectedLookSlotId === intent.slotId) detachDirectorContext("lookSlot", intent.slotId);
      else attachDirectorContext("lookSlot", intent.slotId);
      break;
    case "build-plates":
      void runAction("Preparing wardrobe plates…", "workflow_generate_wardrobe_plate", { sceneId: scene.id });
      break;
    case "delete-plate": {
      const asset = state?.assets?.find((item) => item.id === intent.assetId);
      if (asset) void deleteWardrobePlate(asset);
      break;
    }
    default:
      break;
  }
}

const LOOK_PEOPLE_ROLES = new Set([
  "interior_reference",
  "group_reference",
  "wardrobe_reference",
  "location_reference",
  "composition_reference",
]);

async function deleteWardrobePlate(asset: AssetRecord): Promise<void> {
  const confirmed = await ask(
    `Delete ${asset.path}?\n\nStart frames will fall back to the bare model sheet for this view until you build it again.`,
    { title: "Delete wardrobe plate", kind: "warning" },
  );
  if (!confirmed) return;
  await runAction("Deleting wardrobe plate…", "workflow_delete_wardrobe_plate", {
    assetId: asset.id,
  });
}

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
    (item) => item.role === "wardrobe_reference" && approvedLookAsset(item),
  );
  if (wardrobe) {
    const plates = (state?.assets ?? []).filter(
      (asset) => asset.role === "wardrobe_plate" && asset.scene_id === scene.id && asset.status !== "rejected",
    );
    if (plates.length) {
      return `Generate will prefer scene wardrobe plates over bare model sheets where they exist (${plates.length} plate${plates.length === 1 ? "" : "s"}): ${hits.join(", ")}`;
    }
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

function findLookAsset(slot: LookSlot, id: string): AssetRecord | undefined {
  const matches = (state?.assets ?? []).filter((asset) => asset.id === id);
  return matches.find((asset) => asset.role === slot.role) ?? matches[0];
}

function approvedLookAsset(slot: LookSlot): AssetRecord | undefined {
  if (slot.approved_asset_id) {
    const chosen = findLookAsset(slot, slot.approved_asset_id);
    if (chosen?.status === "approved" && chosen.role === slot.role) return chosen;
  }
  const ids = [...(slot.asset_ids ?? [])].reverse();
  for (const id of ids) {
    const asset = findLookAsset(slot, id);
    if (asset?.status === "approved" && asset.role === slot.role) return asset;
  }
  return undefined;
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
  const blockers = sceneBlockers(scene.id);
  if (blockers.length) {
    const blockerBlock = document.createElement("div");
    blockerBlock.className = "workflow-banner warn";
    blockerBlock.textContent = "Blocked on:";
    for (const item of blockers) {
      const attach = actionButton(
        selectedRequirementId === item.id
          ? `${item.name} attached`
          : `Attach ${item.name}`,
        () => {
          if (selectedRequirementId === item.id) detachDirectorContext("requirement", item.id);
          else attachDirectorContext("requirement", item.id);
        },
      );
      attach.disabled = Boolean(busy);
      blockerBlock.append(document.createTextNode(" "), attach);
    }
    aside.append(blockerBlock);
  }
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
    if (recipeRequires("mouth_policy")) {
      addFact(dl, "Mouth", mouthLabel(cut.sketch?.mouth_policy));
    }
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

function renderJobsPanel(): HTMLElement {
  return renderJobsView(
    buildJobsModel({
      jobs: state?.jobs ?? [],
      scenes: state?.scenes ?? [],
      assets: state?.assets ?? [],
      filter: jobsFilter,
    }),
    jobsOpen,
    jobsFilter,
    handleJobsIntent,
  );
}

function handleJobsIntent(intent: JobsIntent): void {
  if (intent.type === "toggle") {
    jobsOpen = !jobsOpen;
    renderWorkflowDesk();
    return;
  }
  if (intent.type === "filter") {
    jobsFilter = intent.filter;
    renderWorkflowDesk();
    return;
  }
  const row = buildJobsModel({
    jobs: state?.jobs ?? [],
    scenes: state?.scenes ?? [],
    assets: state?.assets ?? [],
    filter: "all",
  }).rows.find((item) => item.id === intent.jobId);
  if (!row) return;
  if (row.sceneId != null) {
    const scene = state?.scenes.find((item) => item.id === row.sceneId);
    if (scene) {
      const cut = row.cut == null ? undefined : scene.cuts.find((item) => item.id.cut === row.cut);
      rememberScene(scene, cut);
      if (row.stage) useStage(row.stage);
      if (row.lookSlotId) {
        selectedLookSlotId = row.lookSlotId;
        focusedLookSlotId = row.lookSlotId;
      }
      renderWorkflowDesk();
      emitSelection();
    }
  }
  if (!row.outputPath) return;
  hooks?.onOpenPath(resolveProjectPath(row.outputPath) ?? row.outputPath);
}

function renderFinishPanel(): HTMLElement {
  const scene = selectedScene();
  return renderFinishView(
    buildFinishModel({
      scene: scene ?? null,
      jobs: state?.jobs ?? [],
      deliverables: state?.profile?.deliverables ?? [],
      busyReason: busy,
    }),
    (intent) => handleFinishIntent(intent),
  );
}

function handleFinishIntent(intent: FinishIntent): void {
  const scene = selectedScene();
  if (!scene) return;
  if (intent.type === "open-review") {
    const path = latestReviewExport(scene, state?.jobs ?? []);
    if (!path) return;
    hooks?.onOpenPath(resolveProjectPath(path) ?? path);
    return;
  }
  if (intent.type === "assemble") {
    void runAction("Preparing assembly…", "workflow_prepare_assemble", { sceneId: scene.id });
    return;
  }
  void runAction("Checking deliverable…", "workflow_check_deliverable", {
    sceneId: scene.id,
    deliverableId: intent.deliverableId,
  });
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
    const sourceStrategy = effectiveVisualSourceStrategy(cut);
    if (sourceStrategy) {
      lines.push(
        `Visual source: ${sourceStrategy} (${visualSourceReady(cut) ? "bound/render ready" : "planned/unresolved"})`,
      );
    }
    if (cut.sketch?.visual_prompt) lines.push(cut.sketch.visual_prompt.slice(0, 1200));
    else if (cut.prompt) lines.push(cut.prompt.slice(0, 1200));
  }
  const text = lines.filter(Boolean).join("\n\n").trim();
  return text || null;
}
