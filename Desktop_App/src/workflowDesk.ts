import { scriptDetailTasks, runScriptDetailTasks } from "./studio/scriptDetails";
import { buildModelSheetRequest } from "./studio/modelSheetRequest";
import { approveExactOperation, cancelOperationApproval, operationApprovalOpen } from "./studio/operationApproval";
import { cancelGenerationPrompt, editGenerationPrompt, generationPromptOpen, generationPromptSpec } from "./studio/generationPrompt";
import type { CharacterRecord } from "./workflowTypes";
import { characterDrafts } from "./studio/surfaces/session";
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
import { reviewImageCrop } from "./studio/imageCropReview";
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
import { beginRequest, handoffRequestToJob, renderActiveRequestIndicator, renderRequestReceipts, settleRequest, syncRequestJobProgress, type RequestReceipt, type RequestOutcome } from "./studio/requestReceipt";
import { visibleDirectorSelection } from "./studio/selection";
import {
  describeDirectorContext,
  type DirectorContextEntry,
} from "./studio/selectors/directorContext";
import { continuityChoice } from "./studio/selectors/continuityChoice";
import { requirementBlocks } from "./studio/selectors/project";
import { audioSections, buildAudioStatus } from "./studio/selectors/audio";
import { buildAssetsModel, type AssetFilterId } from "./studio/selectors/assets";
import { renderAssetsView } from "./studio/assetsView";
import { buildStudioView } from "./studio/viewModel";
import { renderProjectSummary } from "./studio/components/projectSummary";
import { renderEditReceipt } from "./studio/components/editReceipt";
import { renderWorkspaceStatus } from "./studio/components/workspaceStatus";
import { mouthLabel, roleLabel, speechLabel } from "./studio/labels";
import { paintUnknownRecipeStage } from "./studio/recipeStage";
import { clearIntentDraft, hasEditedIntentDraft, renderSceneIntentView, type SceneIntentIntent } from "./studio/sceneIntentView";
import { buildIntentModel } from "./studio/surfaces/surfaceModels";
import { bindSurfaceHost, effectiveVisualSourceStrategy, noteCharacterSaved, renderAssembleStage, renderCharactersStage, renderProductionNotesStage, renderProjectProfile, renderReadiness, renderReferenceImagesStage, renderRenderStage, renderSketchStage, renderTakeReviewStage, resetSurfaceSession, setProfileOpen, stopRoughCut, visualSourceReady, type AssembleIntent, type CharacterIntent, type NotesIntent, type ProfileIntent, type ReferenceIntent, type SketchIntent, type TakeReviewIntent } from "./studio/surfaces/productionSurfaces";
import { block, missingPanel } from "./studio/surfaces/shared";
import { stopPerformanceAudio } from "./studio/surfaces/session";
import { renderScriptStage } from "./studio/surfaces/scriptStage";
import { renderScriptDraftForm, updateDraftRecoveryStatus, type ScriptDraft } from "./studio/surfaces/scriptDraftForm";
import { recoverScriptDraft, persistScriptDraft, discardScriptDraft } from "./studio/scriptDraftRecovery";
import type { ScriptDraftIntent } from "./studio/surfaces/surfaceIntents";
import { loadScriptNotes } from "./studio/surfaces/scriptNotes";
import { ingestScriptFile, saveProjectScript } from "./studio/scriptFiles";
import { reviewProductionHandoff, type HandoffPreview } from "./studio/surfaces/productionHandoffReview";

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
  onPathTrashed?: (path: string) => void;
  onProjectFilesChanged?: (isCurrent: () => boolean) => void;
  onAddToChat: (path: string) => void;
  onOpenVoice: (characterDirectory: string) => void;
  onComposeChat: (text: string, attachment?: string | null) => void;
  onSelectionChange: (selection: WorkflowSelection) => void;
  chatModel: () => { apiId: string; modelId: string } | null;
  imageRoutes?: () => { id: string; label: string }[];
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
let assetFilter: AssetFilterId = "all";
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
let jobsOpen = false;
let jobsFilter: JobFilter | "all" = "all";
let busy: string | null = null;
let busyControl: string | null = null;
let actionNote: string | null = null;
let actionError: string | null = null;
let scriptDraft: ScriptDraft | null = null;

// Settled history belongs to its originating project even after replacement.
// Refresh only this read-only region, never stale model/draft/busy/approval state.
function finishRequest(request: RequestReceipt, outcome: Exclude<RequestOutcome, "working" | "queued" | "running">, detail: string): void {
  settleRequest(request, outcome, detail);
  if (hooks?.projectRoot() !== request.projectRoot || selectedSurface !== request.surface) return;
  const indicator = hooks.root.querySelector<HTMLElement>("[data-global-request-indicator]");
  const nextIndicator = renderActiveRequestIndicator(request.projectRoot, showSurface);
  if (indicator && nextIndicator) indicator.replaceWith(nextIndicator);
  else if (indicator) indicator.remove();
  const next = renderRequestReceipts(request.projectRoot, request.surface);
  const previous = hooks.root.querySelector(".workflow-request-receipts");
  if (previous && next) previous.replaceWith(next);
}

export function bindWorkflowDesk(next: DeskHooks): void {
  hooks = next;
}

// Capture before awaiting the write; tree reads can outlive transaction busy,
// drafts and operation ownership. Only the project session owns publication.
function projectFileInvalidator(): () => void {
  const owner = hooks;
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  const current = () => Boolean(root) && hooks === owner
    && owner?.projectRoot() === root && modelSheetEpoch === epoch;
  return () => { if (current()) owner?.onProjectFilesChanged?.(current); };
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

export async function refreshWorkflowJobsOnly(isCurrent: () => boolean = () => true): Promise<void> {
  const root = hooks?.projectRoot();
  if (!root || !state || !isCurrent()) return;
  try {
    const result = await invoke<{ state?: { jobs?: import("./workflowTypes").JobRecord[] } }>("workflow_status", {
      projectRoot: root,
      refresh: false,
      jobsOnly: true,
    });
    if (!result || !result.state || !isCurrent() || hooks?.projectRoot() !== root) return;
    if (state && result.state.jobs !== undefined) {
      state.jobs = result.state.jobs;
      syncRequestJobProgress(root, state.jobs);
      // Refresh only the jobs drawer and shell indicator; do not rerender stage or clear drafts.
      const rootEl = hooks?.root;
      if (rootEl) {
        const indicator = rootEl.querySelector<HTMLElement>("[data-global-request-indicator]");
        const nextIndicator = renderActiveRequestIndicator(root, showSurface);
        if (indicator && nextIndicator) indicator.replaceWith(nextIndicator);
        else if (indicator) indicator.remove();
        const receipts = rootEl.querySelector<HTMLElement>(".workflow-main > .workflow-request-receipts");
        const nextReceipts = renderRequestReceipts(root, selectedSurface);
        if (receipts && nextReceipts) receipts.replaceWith(nextReceipts);
        const panel = rootEl.querySelector<HTMLElement>(".studio-jobs-panel");
        if (panel) {
          const open = jobsOpen;
          const filter = jobsFilter;
          const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
          const hadFocus = Boolean(active && panel.contains(active));
          const focusJob = hadFocus ? active?.closest<HTMLElement>(".studio-job")?.dataset.jobId : undefined;
          const focusFilter = hadFocus ? active?.dataset.jobFilter : undefined;
          const focusToggle = hadFocus && active?.matches(".studio-jobs-toggle");
          const panelTop = panel.scrollTop;
          const bodyTop = panel.querySelector<HTMLElement>(".workflow-jobs-body")?.scrollTop ?? 0;
          const jobsEl = renderJobsView(
            buildJobsModel({
              jobs: state.jobs,
              scenes: state.scenes ?? [],
              assets: state.assets ?? [],
              filter,
            }),
            open,
            filter,
            handleJobsIntent,
          );
          panel.replaceChildren(jobsEl);
          if (hadFocus) {
            const target = focusJob !== undefined
              ? [...jobsEl.querySelectorAll<HTMLButtonElement>(".studio-job")].find(button => button.dataset.jobId === focusJob)
              : focusFilter !== undefined
                ? [...jobsEl.querySelectorAll<HTMLButtonElement>(".studio-job-filters button")].find(button => button.dataset.jobFilter === focusFilter)
                : focusToggle ? jobsEl.querySelector<HTMLButtonElement>(".studio-jobs-toggle") : null;
            const fallback = jobsEl.querySelector<HTMLButtonElement>(".studio-jobs-toggle");
            (target && !target.disabled ? target : fallback)?.focus({ preventScroll: true });
          }
          panel.scrollTop = panelTop;
          const body = panel.querySelector<HTMLElement>(".workflow-jobs-body");
          if (body) body.scrollTop = bodyTop;
        }
      }
      // Update shell Jobs button text/indicator via existing summary path.
      const btn = document.querySelector<HTMLElement>("#jobs-btn");
      if (btn) {
        const summary = jobsSummary();
        const parts = [`Jobs`];
        if (summary.total) parts.push(`${summary.total}`);
        if (summary.active) parts.push(`• ${summary.active} active`);
        if (summary.failed) parts.push(`• ${summary.failed} failed`);
        btn.textContent = parts.join(" ");
        btn.classList.toggle("jobs-failed", summary.failed > 0);
        btn.title = summary.failed
          ? `${summary.failed} job(s) failed. Open the Jobs drawer for details.`
          : summary.active ? `${summary.active} job(s) active.` : "Open the Jobs drawer.";
      }
    }
  } catch {
    // Polling errors must never resubmit or replace original result.
  }
}

export async function refreshWorkflowDesk(isCurrent: () => boolean = () => true, publishStatus = true): Promise<void> {
  const root = hooks?.projectRoot();
  if (!root || !state || !isCurrent()) return;
  // Provider actions can create new stills, audio, and video files. Re-scan
  // instead of merely reloading the previous state so completed outputs appear
  // in the desk immediately after the Director finishes.
  const result = await invoke<WorkflowImportResult>("workflow_import", {
    projectRoot: root,
    persist: true,
  });
  // Director refreshes carry their originating operation/epoch through the await.
  if (!isCurrent() || hooks?.projectRoot() !== root) return;
  const keepNote = actionNote;
  const previousIntentMissing = selectedScene()?.stages.scene_intent === "missing";
  applyHostResult(result, publishStatus);
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
  scriptDetailsOwner = null;
  cancelGenerationPrompt();
  for (const owner of modelSheetMediaOwners.values()) owner.retire?.();
  cancelOperationApproval();
  consumedStudioApprovals.clear();
  modelSheetEpoch += 1;
  modelSheetMediaOwners.clear();
  scriptDraft = null;
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
  busyControl = null;
  actionNote = null;
  actionError = null;
  resetSurfaceSession();
  syncSurfaceHost();
  if (hooks) hooks.root.replaceChildren();
}

export async function loadWorkflowDesk(projectRoot: string): Promise<void> {
  scriptDetailsOwner = null;
  cancelGenerationPrompt();
  for (const owner of modelSheetMediaOwners.values()) owner.retire?.();
  cancelOperationApproval();
  consumedStudioApprovals.clear();
  modelSheetMediaOwners.clear();
  const epoch = ++modelSheetEpoch;
  const owner = hooks;
  const current = () => modelSheetEpoch === epoch && hooks === owner && hooks?.projectRoot() === projectRoot;
  loadError = null;
  try {
    const result = await invoke<WorkflowImportResult>("workflow_import", {
      projectRoot,
      persist: true,
    });
    if (!current()) return;
    state = activeProjectState(result.state);
    report = result.report;
    rememberScene(state.scenes[0] ?? null);
    useStage(defaultStage(selectedScene()));
    scriptDraft = recoverScriptDraft(projectRoot);
    if (scriptDraft) useStage("script");
    renderWorkflowDesk();
    emitSelection();
  } catch (error) {
    if (!current()) return;
    loadError = error instanceof Error ? error.message : String(error);
    state = null;
    report = null;
    renderWorkflowDesk();
  }
}

export type ScriptImportFeedback = { importFeedback: string };

function persistCurrentDraft(draft: ScriptDraft): void {
  const root = hooks?.projectRoot();
  if (root) persistScriptDraft(root, draft);
  else {
    draft.recoveryFailed = true;
    draft.recoveryStatus = "Open a project for local draft recovery. Text is only in memory.";
  }
  // Do not rerender editable fields on keystrokes (focus/selection must survive).
  if (hooks) updateDraftRecoveryStatus(hooks.root, draft);
}

function openScriptDraft(): void {
  const root = hooks?.projectRoot();
  scriptDraft ??= (root ? recoverScriptDraft(root) : null) ?? { notes: "", title: "", script: "" };
}

/** Recovery is local only; native consent and explicit script Save remain separate. */
export async function handleScriptDraftIntent(intent: ScriptDraftIntent) {
  if (scriptDraft?.busy) return;
  if (intent.type === "change") {
    scriptDraft = { ...intent.draft };
    persistCurrentDraft(scriptDraft);
    return scriptDraft;
  }
  if (intent.type === "cancel") {
    scriptDraft = null;
    renderWorkflowDesk();
    return "cancelled" as const;
  }
  if (!scriptDraft) return;
  if (intent.type === "save" || intent.type === "generate" || intent.type === "save-notes") {
    scriptDraft = { ...scriptDraft, ...intent.draft };
    persistCurrentDraft(scriptDraft);
  }
  const draft = scriptDraft;
  const owner = hooks;
  const projectRoot = owner?.projectRoot();
  const invalidateFiles = projectFileInvalidator();
  const current = () => scriptDraft === draft && hooks === owner && hooks?.projectRoot() === projectRoot;
  const previousError = draft.validationError;
  let request: RequestReceipt | null = null;
  draft.busy = true;
  draft.busyAction = intent.type;
  draft.validationError = undefined;
  renderWorkflowDesk();
  try {
    if (intent.type === "discard") {
      const accepted = await ask("Discard this project's local Title, Notes and Script draft? This removes only the recovery copy, not saved project files.", { title: "Discard local draft", kind: "warning" });
      if (!current() || !accepted) return;
      if (!projectRoot || !discardScriptDraft(projectRoot, draft)) return;
      scriptDraft = null;
      renderWorkflowDesk();
      return "cancelled" as const;
    }
    if (intent.type === "save-notes") {
      if (!projectRoot) throw new Error("Open a project before saving notes.");
      const savedPath = await invoke<string>("save_project_notes", { projectRoot, text: draft.notes });
      if (!current()) return;
      invalidateFiles();
      const success = `Notes saved to ${savedPath}`;
      draft.recoveryStatus = `${success}. Local recovery is also current.`;
      owner?.onStatus(success);
      return draft;
    }
    if (intent.type === "reload-notes") {
      if (!projectRoot) throw new Error("Open a project before reloading notes.");
      let savedNotes: string;
      try {
        savedNotes = await invoke<string>("read_text_file", { path: `${projectRoot}/Notes.md` });
      } catch (error) {
        if (current()) {
          const detail = error instanceof Error ? error.message : String(error);
          draft.validationError = `Notes.md was not loaded. Your current Notes were kept. ${detail}`;
        }
        return draft;
      }
      if (!current()) return;
      draft.notes = savedNotes;
      draft.recoveryFailed = false;
      draft.recoveryStatus = "Notes reloaded from this project's Notes.md file. Title and Script unchanged.";
      return draft;
    }
    if (intent.type === "retry-recovery") {
      persistCurrentDraft(draft);
      return draft;
    }
    if (intent.type === "load-notes") {
      const loaded = await loadScriptNotes();
      if (!current()) return;
      if (loaded === "cancelled") return "cancelled" as const;
      if ("readError" in loaded) {
        draft.validationError = loaded.readError;
      } else {
        draft.notes = loaded.text;
        persistCurrentDraft(draft);
      }
      return draft;
    }
    if (intent.type === "generate") {
      if (!projectRoot) throw new Error("Open a project before generating a script.");
      if (!draft.notes.trim()) throw new Error("Add notes before generating a script.");
      const model = owner?.chatModel();
      if (!model?.apiId || !model.modelId) throw new Error("Select a text/chat model in the chat model dropdown first. Manage APIs and pinned text models in Settings.");
      const notes = await editGenerationPrompt({ title: "Generate script from notes", prompt: draft.notes,
        context: `Title: ${draft.title || "Untitled"}\nText model: ${model.apiId} / ${model.modelId}\nOriginal notes:\n${draft.notes}\nExisting script: ${draft.script.trim() ? "will be replaced only after successful generation" : "none"}` });
      if (notes === null || !current()) return "cancelled" as const;
      request = beginRequest(projectRoot, "script", "Generate script from notes", "workflow_generate_script", {
        title: draft.title, notes, apiId: model.apiId, modelId: model.modelId,
        replaceExisting: Boolean(draft.script.trim()),
      });
      if (!request) return;
      renderWorkflowDesk();
      const result = await invoke<{ script?: string; cancelled?: boolean }>("workflow_generate_script", {
        projectRoot, title: draft.title, notes,
        apiId: model.apiId, modelId: model.modelId, replaceExisting: Boolean(draft.script.trim()),
      });
      if (result.cancelled) {
        finishRequest(request, "cancelled", "Native request cancelled; your draft was retained.");
        if (current()) draft.validationError = previousError;
        return "cancelled" as const;
      }
      if (typeof result.script !== "string" || !result.script.trim()) throw new Error("The model returned no script. Your draft was retained.");
      finishRequest(request, "returned", current()
        ? "Script returned. Review the draft and explicitly Save script to register it."
        : "Script returned after the project session changed. It was not applied to the reopened draft.");
      if (!current()) return;
      draft.script = result.script;
      persistCurrentDraft(draft);
      return draft;
    }
    if (!projectRoot) {
      draft.validationError = "Open a project before saving a script.";
      return draft;
    }
    if (!draft.script.trim()) {
      draft.validationError = "Add script text before saving.";
      return draft;
    }
    const title = draft.title.trim() || "script";
    const saved = await saveProjectScript(projectRoot, /\.(md|markdown|txt)$/i.test(title) ? title : `${title}.md`, draft.script);
    if (!current()) return;
    if (typeof saved !== "string") {
      draft.validationError = saved.saveError;
      return saved;
    }
    invalidateFiles();
    // Close only after native save succeeds; SC05 owns rescan feedback and selection.
    scriptDraft = null;
    await refreshScriptSelection(saved);
    return saved;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (request) finishRequest(request, "error", detail);
    if (current()) {
      const feedback = intent.type === "save-notes"
        ? `Notes.md was not updated. Your latest Notes remain in local recovery. ${detail}`
        : detail;
      draft.validationError = feedback;
      if (intent.type === "save-notes") owner?.onStatus(feedback);
    }
  } finally {
    if (current()) {
      draft.busy = false;
      draft.busyAction = undefined;
      renderWorkflowDesk();
    }
  }
}

/** Register a project file; omission opens Plan's project-rooted chooser. Native SC04 owns confinement/idempotence. */
export async function handleUseAsProjectScript(projectFilePath?: string): Promise<"cancelled" | SceneRecord | ScriptImportFeedback> {
  if (busy || scriptDraft?.busy) return { importFeedback: "Wait for the current action to finish, then register the script." };
  const owner = hooks;
  const invalidateFiles = projectFileInvalidator();
  const projectRoot = owner?.projectRoot();
  const current = () => hooks === owner && hooks?.projectRoot() === projectRoot;
  busy = "Registering project script…";
  renderWorkflowDesk();
  try {
    if (!projectRoot) throw new Error("Open a project before registering a script.");
    const sourcePath = projectFilePath ?? await openDialog({
      title: "Use Existing Project File", defaultPath: projectRoot, directory: false, multiple: false,
      filters: [{ name: "Scripts", extensions: ["md", "markdown", "txt"] }],
    });
    if (!current()) return { importFeedback: "The active project changed. Choose the file again in the intended project." };
    if (sourcePath === null) return "cancelled";
    if (Array.isArray(sourcePath)) throw new Error("Select exactly one project script file.");
    if (!/\.(md|markdown|txt)$/i.test(sourcePath)) throw new Error("Choose a .md, .markdown, or .txt file inside the open project.");
    const ingested = await ingestScriptFile(projectRoot, sourcePath, "existing-project-file");
    if (!current()) return { importFeedback: "The active project changed. Return to the script's project and refresh again." };
    if (typeof ingested !== "string") throw new Error(ingested.ingestError);
    invalidateFiles();
    scriptDraft = null;
    return await refreshScriptSelection(ingested);
  } catch (error) {
    const feedback = `Could not use project script. Choose a supported file inside the open project and try again. For an external source, use Import script instead. ${error instanceof Error ? error.message : String(error)}`;
    if (current()) {
      actionError = feedback;
      actionNote = feedback;
      hooks?.onStatus("Production", feedback);
    }
    return { importFeedback: feedback };
  } finally {
    if (current()) {
      busy = null;
      renderWorkflowDesk();
    }
  }
}

/** Import one dialog-granted external script; native ingestion owns all disk policy. */
export async function handleImportScript(): Promise<"cancelled" | SceneRecord | ScriptImportFeedback> {
  if (busy || scriptDraft?.busy) return { importFeedback: "Wait for the current action to finish, then import the script." };
  const owner = hooks;
  const invalidateFiles = projectFileInvalidator();
  const projectRoot = owner?.projectRoot();
  const current = () => hooks === owner && hooks?.projectRoot() === projectRoot;
  busy = "Importing script…";
  renderWorkflowDesk();
  try {
    if (!projectRoot) throw new Error("Open a project before importing a script.");
    const sourcePath = await openDialog({
      title: "Import Script", directory: false, multiple: false,
      filters: [{ name: "Scripts", extensions: ["md", "markdown", "txt"] }],
    });
    if (!current()) return { importFeedback: "The active project changed. Import again in the intended project." };
    if (!sourcePath) return "cancelled";
    if (Array.isArray(sourcePath)) throw new Error("Select exactly one script file.");
    if (!/\.(md|markdown|txt)$/i.test(sourcePath)) throw new Error("Choose a .md, .markdown, or .txt script file.");
    const ingested = await ingestScriptFile(projectRoot, sourcePath, "external-import");
    if (!current()) return { importFeedback: "The active project changed. Return to the script's project and refresh again." };
    if (typeof ingested !== "string") throw new Error(ingested.ingestError);
    invalidateFiles();
    scriptDraft = null;
    return await refreshScriptSelection(ingested);
  } catch (error) {
    const feedback = `Could not import script. Check the selected file and project access, then try again. ${error instanceof Error ? error.message : String(error)}`;
    if (current()) {
      actionError = feedback;
      actionNote = feedback;
      hooks?.onStatus("Production", feedback);
    }
    return { importFeedback: feedback };
  } finally {
    if (current()) {
      busy = null;
      renderWorkflowDesk();
    }
  }
}

/** Rescan the active project and focus only a scene from the supplied script. */
export async function refreshScriptSelection(relativeScriptPath: string): Promise<SceneRecord | ScriptImportFeedback> {
  const owner = hooks;
  const projectRoot = owner?.projectRoot();
  let feedback: string;
  try {
    if (!projectRoot) throw new Error("Open a project before refreshing the script.");
    const result = await invoke<WorkflowImportResult>("workflow_import", { projectRoot, persist: true });
    if (hooks !== owner || hooks?.projectRoot() !== projectRoot) {
      return { importFeedback: "The active project changed. Return to the script's project and refresh again." };
    }
    // Publish native truth before selecting/emitting; never briefly select an unrelated scene.
    state = activeProjectState(result.state);
    report = result.report;
    loadError = null;
    actionError = null;
    actionNote = null;
    clearIntentDraft();
    const scene = state.scenes.find(item => item.source_path === relativeScriptPath);
    rememberScene(scene ?? null, scene?.cuts[0] ?? null);
    focusedLookSlotId = null;
    focusedLookAssetId = null;
    selectedRequirementId = null;
    selectedLookSlotId = null;
    useStage(defaultStage(scene));
    if (scene) {
      actionNote = result.note ?? null;
      renderWorkflowDesk();
      emitSelection();
      hooks?.onStatus("Production", result.note || workflowStatusLine());
      return scene;
    }
    feedback = `No scenes were imported from ${relativeScriptPath}. Check that the script contains Scene headings, then save and refresh again.`;
  } catch (error) {
    if (hooks !== owner || hooks?.projectRoot() !== projectRoot) {
      return { importFeedback: "The active project changed. Return to the script's project and refresh again." };
    }
    feedback = `Could not refresh ${relativeScriptPath}. Check the script and project access, then try again. ${error instanceof Error ? error.message : String(error)}`;
    rememberScene(null);
  }
  actionError = feedback;
  actionNote = feedback;
  if (!state || !report) loadError = feedback;
  renderWorkflowDesk();
  emitSelection();
  hooks?.onStatus("Production", feedback);
  return { importFeedback: feedback };
}

/** Files saves are imports only for registered sources or discoverable Scripts text. */
export async function refreshSavedScript(path: string, isCurrent: () => boolean): Promise<boolean> {
  const owner = hooks;
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  if (!root || !state || !isCurrent()) return false;
  const prefix = root.replace(/\\/g, "/").replace(/\/+$/, "") + "/";
  const normalized = path.replace(/\\/g, "/");
  if (!normalized.startsWith(prefix)) return false;
  const relative = normalized.slice(prefix.length);
  if (relative.split("/").some(part => part === "." || part === "..")) return false;
  if (!state.scripts?.some(script => script.path === relative)
      && !(relative.startsWith("Scripts/") && relative.split("/").length === 2 && /\.(md|markdown|txt)$/i.test(relative))) return false;
  const current = () => isCurrent() && hooks === owner && hooks?.projectRoot() === root && modelSheetEpoch === epoch;
  const key = selectedSceneKey;
  try {
    const result = await invoke<WorkflowImportResult>("workflow_import", { projectRoot: root, persist: true, editedScriptPath: relative });
    const receipt = result.editedScript;
    if (!receipt || receipt.path !== relative || !result.persistedPath || !Number.isSafeInteger(receipt.stateRevision)
        || !Array.isArray(receipt.affectedSceneIds) || !/^[a-f0-9]{64}$/.test(receipt.sourceSha256)) {
      throw new Error("Native import did not confirm a persisted revision for this source.");
    }
    if (!current()) return true;
    applyHostResult(result, false, true);
    // Keep the current scene when it still exists; otherwise select another
    // active scene rather than retaining a deleted heading's stale selection.
    if (key && !state?.scenes.some(scene => sceneSelectionKey(scene) === key)) {
      rememberScene(state?.scenes[0] ?? null);
    }
    const note = `${relative} · workflow revision ${receipt.stateRevision} · source SHA-256 ${receipt.sourceSha256} · affected scenes: ${receipt.affectedSceneIds.length ? receipt.affectedSceneIds.join(", ") : "none"}. Review current project edit receipt for affected work.`;
    actionNote = note;
    renderWorkflowDesk();
    emitSelection();
    hooks?.onStatus("Script saved and imported", note);
  } catch (error) {
    if (!current()) return true;
    actionError = `Saved on disk, but Studio was not updated: ${String(error)}`;
    actionNote = actionError;
    renderWorkflowDesk();
    hooks?.onStatus("Saved on disk; Studio refresh failed", actionError);
  }
  return true;
}

function activeProjectState(source: ProjectState): ProjectState {
  // Host retains missing-source scenes (and their production work) for recovery;
  // only the live workflow presentation omits them.
  return { ...source, scenes: source.scenes.filter(scene => !scene.source_missing) };
}

function applyHostResult(result: WorkflowImportResult, publishStatus = true, preserveDrafts = false): void {
  state = activeProjectState(result.state);
  report = result.report;
  actionNote = result.note ?? null;
  actionError = null;
  if (!preserveDrafts) clearIntentDraft();
  const scene = selectedScene();
  if (!scene) rememberScene(state.scenes[0] ?? null);
  else if (!selectedCutRecord()) rememberScene(scene, scene.cuts[0] ?? null);
  renderWorkflowDesk();
  emitSelection();
  if (publishStatus) hooks?.onStatus("Production", result.note || workflowStatusLine());
}

function requireChat(): { apiId: string; modelId: string } | null {
  const model = hooks?.chatModel() ?? null;
  if (!model?.modelId) {
    actionError = "Pick a chat model in the dropdown first.";
    actionNote = actionError;
    renderWorkflowDesk();
    hooks?.onStatus("Production", actionError);
    return null;
  }
  return { apiId: model.apiId, modelId: model.modelId };
}

// Artifact writes/copies/trash, not state-only selection, approval or import.
// Preparation belongs here only when it writes a production handoff, not because
// an approval card promises a future media file.
const fileChangingCommands = new Set([
  "workflow_save_profile", "workflow_save_character",
  "workflow_save_production_prompt", "workflow_sync_production_notes",
  "workflow_refresh_production_handoff", "workflow_prepare_render", "workflow_prepare_assemble",
  "workflow_version_audio", "workflow_bind_continuity_frame",
  "workflow_import_performance",
  "workflow_attach_scratch_audio", "workflow_clear_scratch_audio",
  "workflow_attach_start_frame", "workflow_delete_start_frame",
  "workflow_delete_take", "workflow_attach_look", "workflow_delete_look",
  "workflow_delete_wardrobe_plate", "workflow_attach_character_wardrobe",
]);

async function runAction(
  label: string,
  cmd: string,
  args: Record<string, unknown>,
  isCurrent: () => boolean = () => true,
  control: string | null = null,
): Promise<boolean> {
  if (operationApprovalOpen() || modelSheetMediaOwners.size) return false;
  const owner = hooks;
  const invalidateFiles = projectFileInvalidator();
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  const current = () => hooks === owner && owner?.projectRoot() === root
    && modelSheetEpoch === epoch && isCurrent();
  const surface = selectedSurface;
  let savePrompt: { cmd: string; args: Record<string, unknown> } | null = null;
  const spec = generationPromptSpec(cmd, args, state);
  if (spec) {
    if (!root || busy || generationPromptOpen() || !current()) return false;
    const text = await editGenerationPrompt(spec);
    if (text === null || !current()) return false;
    if (cmd === "workflow_prepare_render" || cmd === "workflow_generate_start_frame") {
      if (text !== spec.prompt) {
        const cut = state?.scenes.find(s => s.id === args.sceneId)?.cuts.find(c => c.id.cut === args.cut);
        savePrompt = { cmd: cmd === "workflow_prepare_render" ? "workflow_save_production_prompt" : "workflow_save_start_frame_prompt",
          args: { sceneId: args.sceneId, cut: args.cut, prompt: text,
            negativePrompt: (cmd === "workflow_prepare_render" ? cut?.negative_prompt : cut?.start_frame_negative_prompt) ?? "" } };
      }
    } else args = { ...args, [spec.field]: text };
  }
  const request = root ? beginRequest(root, surface, label, cmd, savePrompt ? {...args, prompt: savePrompt.args.prompt} : args) : null;
  if (root && !request) return false;
  const previousJobIds = new Set((state?.jobs ?? []).map(job => job.id));
  const controller = createStudioController({
    projectRoot: () => root ?? null,
    beforeRun: () => stopRoughCut(),
    beforeInvoke: async () => {
      if (!current()) throw new Error("Project session changed before preparation; prepare again.");
      if (savePrompt) {
        await invoke(savePrompt.cmd, { projectRoot: root, ...savePrompt.args });
        if (!current()) throw new Error("Project session changed while saving prompt; no preparation sent.");
        invalidateFiles();
      }
    },
    onBusy: (next) => {
      if (!current()) return;
      busy = next;
      if (next) busyControl = control;
      else if (busyControl === control) busyControl = null;
      if (next) {
        actionNote = null;
        actionError = null;
      }
      renderWorkflowDesk();
    },
    onResult: (result) => {
      const queuedJobs = (result.state?.jobs ?? []).filter(job => !previousJobIds.has(job.id)
        && ["pending", "running"].includes((job.status ?? "").toLowerCase()));
      if (request && !current()) {
        finishRequest(request, "cancelled", "The originating project session closed before this host response returned. The old result is historical; no state or approval was applied to the reopened session.");
      } else if (request && queuedJobs.length === 1) {
        handoffRequestToJob(request, queuedJobs[0].id);
      } else if (request) finishRequest(request, result.pendingAction ? "pending" : "success",
          result.pendingAction ? "Host preparation returned a pending approval. No execution or media completion is implied." : result.note || "Host command completed.");
      if (!current()) {
        // A preparation can finish after the originating project was retired.
        // Revoke only its exact ID; never issue a late process-wide Stop.
        const ids = [...(result.pendingAction ?? "").matchAll(/\[\[workbench_approve:([A-Za-z0-9]+)\]\]/g)].map(m => m[1]);
        if (ids.length === 1) void invoke("director_cancel", {id: ids[0]}).catch(() => {
          console.warn("Could not revoke a retired Studio preparation; inspect Jobs.");
        });
        return;
      }
      applyHostResult(result);
      if (fileChangingCommands.has(cmd)) invalidateFiles();
      if (result.pendingAction) void authorizeStudioOperation(result.pendingAction, request, current, invalidateFiles);
    },
    onError: (message) => {
      if (request) finishRequest(request, "error", message);
      if (!current()) return;
      actionError = cmd === "workflow_prepare_render" && args.resumeSavedAudio === true
        ? `Cannot resume Scene ${args.sceneId} Cut ${args.cut} with saved audio. ${message} No speech or video was submitted. Do not use Render Cut as a workaround. Generate or import dialogue in Scene Look and explicitly select a complete WAV before separately preparing video. Video never synthesizes speech.`
        : message;
      actionNote = actionError;
      renderWorkflowDesk();
      hooks?.onStatus("Production", actionError);
    },
  });
  return controller.run({ label, cmd, args });
}

let operationSerial = 0;
const consumedStudioApprovals = new Set<string>();

/** Reuse the direct media owner bridge: no chat tab or planner turn is created. */
async function authorizeStudioOperation(text: string, request: RequestReceipt | null, sessionCurrent: () => boolean, invalidateFiles: () => void): Promise<void> {
  const ownerId = `studio-operation-${++operationSerial}`;
  let live = true;
  let operationId: string | null = null;
  const current = () => live && sessionCurrent() && modelSheetMediaOwners.get(ownerId) === owner;
  const feedback = (outcome: Exclude<RequestOutcome, "working" | "queued" | "running">, detail: string) => {
    // A prepared native job remains the lifecycle authority while its separate
    // approval is pending. The modal carries approval state; do not downgrade
    // the visible queued-job handoff back into a renderer-only request state.
    if (request && !(request.jobId && outcome === "pending")) finishRequest(request, outcome, detail);
    if (!current()) return;
    actionNote = detail; actionError = outcome === "error" ? detail : null;
    renderWorkflowDesk(); hooks?.onStatus("Studio operation", detail);
  };
  const release = () => {
    if (request) request.stop = undefined;
    live = false; modelSheetMediaOwners.delete(ownerId);
  };
  const stop = async (retiring = false) => {
    if (!live || (!retiring && !current())) return;
    // Retire the owner before awaiting cancellation so late completion cannot publish.
    cancelOperationApproval();
    feedback("cancelled", "Stopped; no further operation or continuation authorized. Inspect Jobs for already submitted work.");
    live = false;
    if (request) request.stop = undefined;
    try { if (operationId) await invoke("director_cancel", {id: operationId}); }
    catch (error) {
      if (sessionCurrent()) {
        actionError = `Stop request failed: ${error instanceof Error ? error.message : String(error)}. Inspect Jobs; do not retry automatically.`;
        actionNote = actionError; renderWorkflowDesk();
      }
    } finally { modelSheetMediaOwners.delete(ownerId); }
  };
  const approve = async (pendingText: string): Promise<void> => {
    if (!current()) return;
    const ids = [...pendingText.matchAll(/\[\[workbench_approve:([A-Za-z0-9]+)\]\]/g)].map(m => m[1]);
    if (ids.length !== 1) throw new Error("Host did not supply exactly one operation approval ID. Prepare again; nothing was dispatched.");
    const id = ids[0];
    operationId = id;
    const key = `${modelSheetEpoch}:${id}`;
    if (consumedStudioApprovals.has(key)) throw new Error("This operation approval was already handled. Prepare a fresh request.");
    consumedStudioApprovals.add(key);
    feedback("pending", "Inspect the exact host operation in the authorization modal. No media execution authorized.");
    const accepted = await approveExactOperation(pendingText);
    if (!current()) return;
    if (!accepted) { await stop(); return; }
    feedback("pending", "Operation approved once; working or waiting for separate paid-media confirmation. Use Stop to cancel further work.");
    // Exact-operation approval can keep the host call open while a native job
    // changes state. Observe that authoritative record without requiring Jobs
    // to be opened or a separate paid-media confirmation to start polling.
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    const poll = async (): Promise<void> => {
      if (!current()) return;
      await refreshWorkflowJobsOnly(current);
      if (current()) pollTimer = setTimeout(() => { void poll(); }, 500);
    };
    pollTimer = setTimeout(() => { void poll(); }, 500);
    let result: {reply: string; toolEvents: string[]; stopped?: boolean};
    try {
      result = await invoke("director_approve", {id, progressId: ownerId});
    } finally {
      if (pollTimer) clearTimeout(pollTimer);
    }
    if (!current()) return;
    if (result.stopped) { feedback("cancelled", result.reply); release(); return; }
    if (result.toolEvents.includes("approve:error")) throw new Error(result.reply);
    if (!result.toolEvents.includes("approve:awaiting-spend-authorization")) await owner.settle({success: true, result: {continuation: {reply: result.reply}}});
  };
  const owner = {
    isCurrent: current,
    authorizeMedia: () => undefined,
    retire: () => { void stop(true); },
    settle: async (outcome: {success: boolean; result?: unknown; error?: unknown}): Promise<void> => {
      if (!current()) return;
      try {
        if (!outcome.success) throw new Error(String(outcome.error ?? "Media was not authorized."));
        const result = outcome.result as {continuation?: {reply?: string}} | undefined;
        await refreshWorkflowDesk(current, false);
        if (!current()) return;
        invalidateFiles();
        const reply = result?.continuation?.reply ?? "";
        if (/\[\[workbench_approve:/.test(reply)) { await approve(reply); return; }
        // Reconcile the linked receipt from refreshed native Jobs before the
        // continuation acknowledgement settles it. Only a completed JobRecord
        // with an output path may replace the acknowledgement with saved output.
        if (request) syncRequestJobProgress(request.projectRoot, state?.jobs ?? []);
        if (request?.outcome === "saved") feedback("saved", request.detail);
        else feedback("success", result?.continuation ? reply || "Host operation completed; inspect refreshed output and Jobs." : "Media response returned; inspect Jobs and refreshed output for execution status.");
        release();
      } catch (error) {
        if (!current()) return;
        feedback("error", error instanceof Error ? error.message : String(error)); release();
      }
    },
  };
  if (!sessionCurrent()) return;
  modelSheetMediaOwners.set(ownerId, owner);
  if (request) request.stop = { isCurrent: current, run: () => { void stop(); } };
  try { await approve(text); }
  catch (error) { if(current()) {feedback("error", error instanceof Error ? error.message : String(error)); release();} }
}

export function openJobsDrawer(): void {
  jobsOpen = true;
  renderWorkflowDesk();
}

export function toggleJobsDrawer(): void {
  jobsOpen = !jobsOpen;
  renderWorkflowDesk();
}

export function jobsDrawerOpen(): boolean {
  return jobsOpen;
}

export function jobsSummary(): { total: number; active: number; failed: number } {
  const jobs = state?.jobs ?? [];
  let active = 0;
  let failed = 0;
  for (const job of jobs) {
    const status = (job.status ?? "").toLowerCase();
    if (status === "failed" || status === "error") failed += 1;
    else if (status === "pending" || status === "running" || status === "awaiting" || status === "in_progress") active += 1;
  }
  return { total: jobs.length, active, failed };
}

function syncSurfaceHost(): void {
  bindSurfaceHost({
    busy,
    busyControl,
    error: actionError,
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
      actionError = message;
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
  stopPerformanceAudio();
  syncSurfaceHost();
  const root = hooks?.root;
  if (!root) {
    renderFinishMount();
    return;
  }
  const viewport = captureWorkflowViewport(root);
  root.replaceChildren();
  if (loadError) {
    const error = block("workflow-error", `Could not import workflow state.\n${loadError}`);
    error.addEventListener("contextmenu", () => {
      const selection = window.getSelection();
      if (!selection) return;
      const range = document.createRange();
      range.selectNodeContents(error);
      selection.removeAllRanges();
      selection.addRange(range);
    });
    root.append(error);
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
  if (actionNote) {
    const feedback = block(actionError ? "workflow-banner reject" : noteClass(actionNote), actionNote);
    if (actionError) {
      feedback.setAttribute("role", "alert");
      feedback.addEventListener("contextmenu", () => {
        const selection = window.getSelection();
        if (!selection) return;
        const range = document.createRange();
        range.selectNodeContents(feedback);
        selection.removeAllRanges();
        selection.addRange(range);
      });
    }
    head.append(feedback);
  }
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
  if (selectedSurface === "profile" || selectedSurface === "assets") return;
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
  body.append(scenes, renderMain(), renderJobsPanel());
  renderFinishMount();
  renderStateMount();
  // Director context has one presentation: the interactive "Attached context"
  // list in #director-context (add/remove without deleting project state).
  // The former read-only chips mount was removed from index.html entirely.
  return body;
}

function renderFinishMount(): void {
  const mount = document.querySelector("#studio-finish-mount");
  if (!mount) return;
  mount.replaceChildren(renderFinishPanel());
}

function renderStateMount(): void {
  const mount = document.querySelector("#studio-state-mount");
  if (!mount) return;
  const receipt = renderEditReceipt(state);
  if (receipt) mount.replaceChildren(receipt);
  else mount.replaceChildren();
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
      state: state ?? null,
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
  const create = actionButton("Create script", () => {
    openScriptDraft();
    useStage("script");
    renderWorkflowDesk();
  });
  create.disabled = Boolean(busy || scriptDraft?.busy);
  const importScript = actionButton("Import script", () => { void handleImportScript(); });
  importScript.disabled = Boolean(busy || scriptDraft?.busy);
  const useExisting = actionButton("Use existing project file", () => { void handleUseAsProjectScript(); });
  useExisting.disabled = Boolean(busy || scriptDraft?.busy);
  aside.append(create, importScript, useExisting);
  return aside;
}

function showSurface(id: SurfaceId): void {
  selectedSurface = id;
  if (id !== "profile" && id !== "assets") selectedStage = id;
  if (id === "profile") setProfileOpen(true);
  renderWorkflowDesk();
  emitSelection();
}

let scriptDetailsOwner: object | null = null;
async function generateScriptDetails(scene: SceneRecord): Promise<void> {
  const owner = hooks, root = owner?.projectRoot(), epoch = modelSheetEpoch, source = scene.source_path;
  if (scriptDetailsOwner || busy || !owner || !root || !source || !state?.scripts?.some(s => s.path === source)) return;
  const chat = requireChat();
  if (!chat) return;
  const tasks = scriptDetailTasks(state.scenes, source).filter(task => task.cmd !== "workflow_draft_intent" || !hasEditedIntentDraft(task.sceneId));
  if (!tasks.length) {
    actionNote = "No missing details to draft for this script. Existing intent and look data were preserved.";
    actionError = null; renderWorkflowDesk(); return;
  }
  const token = {}; scriptDetailsOwner = token;
  const current = () => hooks === owner && owner.projectRoot() === root && modelSheetEpoch === epoch && scriptDetailsOwner === token;
  try {
    const approved = await ask(`Generate missing details for ${source}?\n\nStudio will send up to ${tasks.length} text-model requests for scene intent and reference-image suggestions using model ${chat.modelId} (API ${chat.apiId}). Your script and scene context will be sent to the configured provider; charges may apply. Existing details will be kept. No images or video will be generated.`, { title: "Generate script details", kind: "warning" });
    if (!approved || !current()) return;
    const outcome = await runScriptDetailTasks(tasks, current, async (task, index, total) => {
      if (!current()) return false;
      actionNote = `Generate details: ${index + 1} of ${total}, Scene ${task.sceneId} ${task.cmd === "workflow_draft_intent" ? "intent" : "look"}.`;
      actionError = null; renderWorkflowDesk();
      if (task.cmd === "workflow_draft_intent" && hasEditedIntentDraft(task.sceneId)) {
        actionError = "Unsaved scene intent was preserved. Save it before generating details.";
        actionNote = actionError; renderWorkflowDesk(); return false;
      }
      const ok = await runAction(actionNote, task.cmd, { ...task, ...chat }, current);
      if (ok && current() && task.cmd === "workflow_draft_intent" && !hasEditedIntentDraft(task.sceneId)) clearIntentDraft();
      return ok;
    });
    if (current()) {
      if (outcome === "complete") actionNote = "Script details drafted. Review scene intent and look suggestions before use; existing details were preserved.";
      else if (!actionError) actionNote = "Generate details stopped. Completed drafts were kept; no further requests were sent.";
    }
  } finally {
    if (scriptDetailsOwner === token) scriptDetailsOwner = null;
    if (hooks === owner && owner.projectRoot() === root && modelSheetEpoch === epoch) renderWorkflowDesk();
  }
}

/** Plan's script entry is reachable before any scene-selection guard. */
export function renderPlanScriptEntry(project: ProjectState | null, scene: SceneRecord | undefined, draft: ScriptDraft | null): HTMLElement {
  if (draft) return renderScriptDraftForm(draft, intent => { void handleScriptDraftIntent(intent); }, hooks?.projectRoot() ?? undefined);
  if (scene) return renderScriptStage(scene, () => { void generateScriptDetails(scene); }, Boolean(busy || scriptDetailsOwner));
  if (project?.scenes.length) return block("empty-state", "Select a scene.");
  const panel = document.createElement("article");
  panel.className = "workflow-script-entry";
  const title = document.createElement("h1");
  title.textContent = "Start your script";
  panel.append(title, block("workflow-script-guidance",
    "Create a script locally, import a .md, .markdown or .txt file, or use an existing project file. Scripts need Scene N headings (for example, Scene 1 — Arrival). If a file has no scene headings, add them and try again."));
  const create = actionButton("Create script", () => {
    openScriptDraft();
    useStage("script");
    renderWorkflowDesk();
  });
  const importScript = actionButton("Import script", () => { void handleImportScript(); });
  const useExisting = actionButton("Use existing project file", () => { void handleUseAsProjectScript(); });
  for (const button of [create, importScript, useExisting]) button.disabled = Boolean(busy);
  panel.append(create, importScript, useExisting);
  return panel;
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
  const activeRequest = renderActiveRequestIndicator(hooks?.projectRoot() ?? null, showSurface);
  if (activeRequest) main.append(activeRequest);
  const receipts = renderRequestReceipts(hooks?.projectRoot() ?? null, selectedSurface);
  if (receipts) main.append(receipts);
  // A draft belongs to Script, not to the whole workspace. Keep the nav above
  // this guard so empty projects and unfinished drafts never trap navigation.
  if (selectedSurface === "script" && (scriptDraft || !state?.scenes.length)) {
    main.append(renderPlanScriptEntry(state, scene, scriptDraft));
    return main;
  }
  if (selectedSurface === "assets") {
    main.append(renderAssetsPanel());
    return main;
  }
  const studio = currentStudioView();
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
    main.append(block("empty-state", !state?.scenes.length
      ? "Create or import a script in Script first, then select a scene to use this surface."
      : banner.text));
    return main;
  }
  if (scene && selectedSurface !== "scene_look") {
    const header = document.createElement("header");
    header.className = "studio-scene-header";
    const title = document.createElement("h1");
    title.textContent = scene.title;
    header.append(title);
    if (studio) {
      const status = renderWorkspaceStatus(studio.workspace);
      status.classList.add("studio-scene-sub");
      header.append(status);
    }
    main.append(header);
  } else if (studio) {
    const status = renderWorkspaceStatus(studio.workspace);
    status.classList.add("studio-scene-sub");
    main.append(status);
  }
  // Scene Look owns its compact blocker summary inside the view (header -> blockers -> hero),
  // so the shell must not prepend a second copy above the artifact.
  if (banner?.tone === "blocked" && selectedSurface !== "scene_look") {
    const details = document.createElement("details");
    details.className = "studio-blocker-summary";
    const summary = document.createElement("summary");
    summary.textContent =
      currentStage?.disabledReason ??
      (blockers.length
        ? `⚠ ${blockers.length} requirement${blockers.length === 1 ? "" : "s"} blocking production · Review requirements`
        : banner.text);
    details.append(summary);
    if (!currentStage?.disabledReason) {
      for (const item of blockers) {
        const row = document.createElement("div");
        row.className = "studio-blocker-row";
        const name = document.createElement("span");
        name.textContent = item.name;
        const attach = actionButton(
          selectedRequirementId === item.id ? `${item.name} attached` : `Attach ${item.name}`,
          () => {
            if (selectedRequirementId === item.id) detachDirectorContext("requirement", item.id);
            else attachDirectorContext("requirement", item.id);
          },
        );
        attach.disabled = Boolean(busy);
        row.append(name, attach);
        details.append(row);
      }
    }
    main.append(details);
  }
  if (scene && selectedSurface !== "scene_look" && selectedSurface !== "profile") {
    const audioPanel = renderSceneAudio(scene);
    if (audioPanel) main.append(audioPanel);
  }
  if (selectedSurface === "profile") {
    main.append(renderProjectProfile((intent) => handleProfileIntent(intent)));
    return main;
  }
  const sceneStatus = sceneReport(selectedSceneId);
  if (sceneStatus?.effectively_performance_heavy && selectedSurface === "production_notes") {
    main.append(
      block(
        "workflow-banner warn",
        "This mix is effectively performance-heavy for a cinematic-balanced request.",
      ),
    );
  }
  if (selectedSurface === "scene_look" && scene) main.append(renderSceneLookWorkspace(scene));
  else if (selectedSurface === "characters") main.append(renderCharactersStage((intent) => handleCharacterIntent(intent)));
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

function renderAssetsPanel(): HTMLElement {
  if (!state) {
    const wrap = document.createElement("div");
    wrap.className = "workflow-stage-content";
    wrap.append(block("empty-state", "Open a project to browse assets."));
    return wrap;
  }
  return renderAssetsView(buildAssetsModel(state, assetFilter), (id) => {
    assetFilter = id;
    renderWorkflowDesk();
  });
}

function renderSceneAudio(scene: SceneRecord): HTMLElement | null {
  if (!state) return null;
  const status = buildAudioStatus(state, scene.id);
  if (!status) return null;
  const sections = audioSections(status);
  if (sections.length === 0) return null;

  const panel = document.createElement("section");
  panel.className = "studio-scene-audio";

  const heading = document.createElement("h2");
  heading.className = "studio-audio-heading";
  heading.textContent = "Audio";
  panel.append(heading);

  for (const section of sections) {
    const row = document.createElement("div");
    row.className = "studio-audio-section";
    const label = document.createElement("span");
    label.className = "studio-audio-label";
    label.textContent = section.label;
    row.append(label);
    const detail = document.createElement("span");
    detail.className = "studio-audio-status";
    detail.textContent = section.entries
      .map((entry) =>
        entry.present ? `${entry.label} audio ${entry.approved ? "approved" : "draft"}` : `${entry.label} audio missing`,
      )
      .join(" · ");
    row.append(detail);
    panel.append(row);
  }
  return panel;
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
      wrap.append(renderPlanScriptEntry(state, scene, null));
      break;
    case "characters":
      wrap.append(renderCharactersStage((intent) => handleCharacterIntent(intent)));
      break;
    case "scene_intent":
      wrap.append(renderSceneIntentWorkspace(scene));
      break;
    case "cut_sketches":
      wrap.append(renderSketchStage(
        scene,
        sceneStatus,
        (intent) => handleSketchIntent(scene, intent),
        (intent) => handleReferenceIntent(scene, intent),
        (intent) => handleNotesIntent(scene, intent),
      ));
      break;
    case "reference_images":
      wrap.append(renderReferenceImagesStage(scene, sceneStatus, (intent) => handleReferenceIntent(scene, intent),
        (slotId) => renderSceneLookWorkspace(scene, slotId)));
      break;
    case "production_notes":
      wrap.append(renderReadiness(scene, sceneStatus));
      if (!scene.cuts.length) {
        wrap.append(missingPanel("No production notes for this scene.", "Run scene plan or import a project that has Scene_Prompts."));
      } else {
        wrap.append(renderProductionNotesStage(scene, (intent) => handleNotesIntent(scene, intent)));
      }
      break;
    case "render":
      wrap.append(renderRenderStage(scene, (intent) => handleNotesIntent(scene, intent)));
      break;
    case "take_review":
      wrap.append(renderTakeReviewStage(scene, (intent) => handleTakeReviewIntent(scene, intent)));
      break;
    case "assemble":
      wrap.append(renderAssembleStage(scene, (intent) => handleAssembleIntent(scene, intent)));
      break;
    case "scene_look":
      break;
    default: {
      const stage = activeStages().find((item) => item.id === selectedStage);
      wrap.append(
        paintUnknownRecipeStage({
          id: selectedStage,
          label: stage?.label ?? selectedStage,
          disabledReason: stage?.disabledReason ?? null,
        }),
      );
      break;
    }
  }
  return wrap;
}

function handleProfileIntent(intent: ProfileIntent): void {
  switch (intent.type) {
    case "save":
      void runAction("Saving production profile", "workflow_save_profile", { profile: intent.profile }, () => true, "profile-save");
      break;
    case "set-requirement":
      void runAction(`Updating ${intent.name}`, "workflow_set_requirement_status", {
        requirementId: intent.requirementId,
        status: intent.status,
      });
      break;
    case "add-requirement":
      void runAction(`Adding ${intent.name}`, "workflow_add_requirement", {
        name: intent.name,
        kind: intent.kind,
        sceneId: intent.sceneId,
      });
      break;
    default:
      break;
  }
}

// Direct surface operations use the existing exact-media confirmation, never chat.
export const modelSheetMediaOwners = new Map<string, {
  isCurrent: () => boolean;
  retire?: () => void;
  authorizeMedia: (request: { operation: string; args: Record<string, unknown> }) => boolean | undefined;
  settle: (outcome: { success: boolean; result?: unknown; error?: unknown }) => Promise<void>;
}>();
let modelSheetSerial = 0;
let modelSheetEpoch = 0;
let characterReferenceOwner: symbol | null = null;

export async function handleCreateModelSheet(characterId: string, approvalMode: "each" | "whole" = "each"): Promise<void> {
  const projectRoot = hooks?.projectRoot();
  if (busy || !projectRoot || !state || modelSheetMediaOwners.size || generationPromptOpen()) return;
  const promptEpoch = modelSheetEpoch;
  const promptOwner = hooks;
  const promptCurrent = () => hooks === promptOwner && promptEpoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot;
  const character = state.characters.find(person => person.id === characterId);
  const draft = characterDrafts.get(characterId);
  const dirty = draft && character && (draft.name !== character.name || (draft.references?.length ?? 0) > 0
    || Object.entries(draft.profile).some(([field, value]) =>
      JSON.stringify(value) !== JSON.stringify(character.profile?.[field as keyof typeof character.profile] ?? (Array.isArray(value) ? [] : null))));
  if (!character || dirty) {
    actionError = "Save required: save this named character and its unsaved visual/reference edits to associate the model sheet with the correct saved inputs. No full profile is required.";
    actionNote = actionError; renderWorkflowDesk(); return;
  }
  const request = buildModelSheetRequest(character, character.profile?.identity_prompt ?? "", state.assets ?? []);
  if ("validationError" in request) {
    actionError = request.validationError.message; actionNote = actionError; renderWorkflowDesk(); return;
  }
  const identity = await editGenerationPrompt({ title: `Create model sheet · ${character.name}`, prompt: request.inputs.identity_text,
    context: `Saved character: ${character.name}\nProfile: ${character.profile_path}\nSelected references:\n${request.inputs.reference_images.join("\n") || "None (text-only identity)"}\nOutput: ${request.inputs.output_path}\nThis edits this request only; the saved character profile is unchanged.` });
  if (identity === null || !promptCurrent() || busy) return;
  request.inputs.identity_text = identity;
  if (approvalMode === "whole") {
    const accepted = await ask(
      `Generate the entire model sheet for ${character.name}? This authorizes up to 12 separate image generations, one per unfinished pose, without further approval dialogs. Text-only characters use a first identity plate and then the saved plate as a reference for the rest. Models come from Settings; the provider's total cost may be unknown or unbounded. No alternate model or automatic retry is authorized. Cancel to keep per-image approval available.`,
      { title: "Authorize entire model sheet", kind: "warning" },
    );
    if (!accepted || !promptCurrent() || busy) return;
  }
  const requestReceipt = beginRequest(projectRoot, "characters", "Preparing model sheet", "start_character_model_sheet", {
    characterId,
    prompt: identity,
  });
  if (!requestReceipt) return;
  const previousPaths = new Set((state.assets ?? []).map(asset => asset.path));
  const epoch = modelSheetEpoch;
  const ownerId = `modelsheet${++modelSheetSerial}`;
  const wholeSheetShots = new Set<string>();
  const wholeSheetDir = `${projectRoot.replace(/\/$/, "")}/${character.dir}/model_sheet/`;
  const current = () => epoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot && modelSheetMediaOwners.get(ownerId) === owner;
  const feedback = (message: string, error = false) => {
    if (!current()) return;
    actionNote = message; actionError = error ? message : null;
    renderWorkflowDesk(); hooks?.onStatus("Model sheet", message);
  };
  const approve = async (text: string): Promise<void> => {
    const id = /\[\[workbench_approve:([A-Za-z0-9]+)\]\]/.exec(text)?.[1];
    if (!id) throw new Error("Model-sheet approval was not supplied by the host.");
    feedback("Model sheet pending approval; no generation authorized.");
    const accepted = approvalMode === "whole" ? true : await ask(text.replace(/\[\[workbench_approve:[A-Za-z0-9]+\]\]/g, ""), { title: "Approve model sheet", kind: "warning" });
    if (!current()) return;
    if (!accepted) {
      await invoke("director_cancel");
      feedback("Model sheet denied; no new generation authorized.");
      modelSheetMediaOwners.delete(ownerId); return;
    }
    feedback("Model sheet pending generation / exact media authorization.");
    const result = await invoke<{ reply: string; toolEvents: string[]; stopped?: boolean }>("director_approve", { id, progressId: ownerId });
    if (!current()) return;
    if (result.stopped || result.toolEvents.includes("approve:error")) throw new Error(result.reply);
    // Exact media confirmation owns settlement after this callback returns.
    if (!result.toolEvents.includes("approve:awaiting-spend-authorization")) {
      if (/\[\[workbench_approve:/.test(result.reply)) await approve(result.reply);
      else await owner.settle({ success: true, result: { continuation: { reply: result.reply } } });
    }
  };
  const owner = {
    isCurrent: current,
    authorizeMedia: ({ operation, args }: { operation: string; args: Record<string, unknown> }): boolean | undefined => {
      if (approvalMode !== "whole") return undefined;
      if (!current() || operation !== "atlas_generate_image" || typeof args.save_path !== "string"
        || !args.save_path.startsWith(wholeSheetDir)) return false;
      const filename = args.save_path.slice(wholeSheetDir.length);
      const shot = /(?:^|_)(closeup_front|closeup_left|closeup_right|standing_front|standing_left|standing_right|standing_back|t_pose_front|expression_happy|expression_sad|expression_excited|expression_surprised)\.(?:png|jpg|jpeg|webp)$/.exec(filename)?.[1];
      if (!shot || wholeSheetShots.has(shot) || wholeSheetShots.size >= 12) return false;
      wholeSheetShots.add(shot);
      return true;
    },
    settle: async (outcome: { success: boolean; result?: unknown; error?: unknown }): Promise<void> => {
      if (!current()) return;
      try {
        if (!outcome.success) throw new Error(String(outcome.error ?? "Model-sheet generation was not authorized."));
        await refreshWorkflowDesk(current, false);
        if (!current()) return;
        const result = outcome.result as { continuation?: { reply?: string } } | undefined;
        const reply = result?.continuation?.reply ?? "";
        if (/\[\[workbench_approve:/.test(reply)) {
          for (const asset of state?.assets ?? []) previousPaths.add(asset.path);
          await approve(reply); return;
        }
        // Do not equate a submitted prediction without saved output with completion.
        const saved = state?.characters.find(person => person.id === characterId);
        const alreadyComplete = /\bModel sheet pack complete \(\d+ saved\)\./.test(reply);
        const hasOutput = state?.assets?.some(asset => (alreadyComplete || !previousPaths.has(asset.path)) && saved?.asset_ids?.includes(asset.id)
          && asset.notes !== "uploaded reference image" && !saved?.reference_asset_ids?.includes(asset.id)
          && ["identity_reference", "body_reference", "expression_reference"].includes(asset.role));
        feedback(hasOutput ? "Model sheet completed; saved stills refreshed." : "Model sheet pending output; no new saved stills yet.");
        modelSheetMediaOwners.delete(ownerId);
      } catch (error) {
        feedback(`Model sheet error / not authorized: ${error instanceof Error ? error.message : String(error)}`, true);
        modelSheetMediaOwners.delete(ownerId);
      }
    },
  };
  modelSheetMediaOwners.set(ownerId, owner);
  busy = "Preparing model sheet…";
  busyControl = `character-model-sheet:${characterId}:${approvalMode}`;
  actionError = null; renderWorkflowDesk();
  try {
    const result = await invoke<{ pendingAction: string }>("start_character_model_sheet", { projectRoot, request });
    finishRequest(requestReceipt, "pending", current()
      ? "Host preparation returned a pending approval. No execution or media completion is implied."
      : "Host response returned after the originating session changed. No state or approval was applied here; reload current state and prepare again if needed.");
    if (current()) await approve(result.pendingAction);
  } catch (error) {
    const message = `Model sheet error: ${error instanceof Error ? error.message : String(error)}`;
    finishRequest(requestReceipt, "error", message);
    feedback(message, true);
    modelSheetMediaOwners.delete(ownerId);
  } finally {
    if (epoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot) {
      busy = null;
      if (busyControl === `character-model-sheet:${characterId}:${approvalMode}`) busyControl = null;
      renderWorkflowDesk();
    }
  }
}

async function handleDeleteModelSheetPhoto(characterId: string, assetId: string): Promise<void> {
  const projectRoot = hooks?.projectRoot();
  if (busy || !projectRoot || !state) return;
  const epoch = modelSheetEpoch;
  const current = () => epoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot;
  const character = state.characters.find(person => person.id === characterId);
  const asset = state.assets?.find(item => item.id === assetId && item.character_id === characterId);
  if (!character || !asset || !(character.asset_ids ?? []).includes(assetId)) {
    actionError = "The selected model-sheet photo is no longer available.";
    renderWorkflowDesk(); return;
  }
  const label = asset.notes?.trim() || asset.path.split("/").pop() || "photo";
  const previewPath = resolveProjectPath(asset.path);
  const accepted = await ask(
    `Delete ${character.name}'s model-sheet photo “${label}”?\n\nThe file will be moved to the project's recoverable trash, and this shot can be regenerated later.`,
    { title: "Delete model-sheet photo", kind: "warning" },
  );
  if (!accepted || !current()) return;
  busy = "Deleting model-sheet photo…"; actionError = null; renderWorkflowDesk();
  try {
    const result = await invoke<WorkflowImportResult>("workflow_delete_model_sheet_photo", {
      projectRoot, characterId, assetId,
    });
    if (!current()) return;
    applyHostResult(result, false, true);
    if (previewPath) hooks?.onPathTrashed?.(previewPath);
    actionNote = `Moved ${label} to project trash. The shot can be regenerated.`;
  } catch (error) {
    if (current()) {
      actionError = `Could not delete model-sheet photo: ${error instanceof Error ? error.message : String(error)}`;
      actionNote = actionError;
    }
  } finally {
    if (current()) { busy = null; renderWorkflowDesk(); }
  }
}

async function handleDeleteCharacterReference(characterId: string, assetId: string): Promise<void> {
  const projectRoot = hooks?.projectRoot();
  if (busy || !projectRoot || !state) return;
  const epoch = modelSheetEpoch;
  const current = () => epoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot;
  const character = state.characters.find(person => person.id === characterId);
  const asset = state.assets?.find(item => item.id === assetId && item.character_id === characterId);
  if (!character || !asset || !(character.reference_asset_ids ?? []).includes(assetId)) {
    actionError = "The selected uploaded reference is no longer available.";
    renderWorkflowDesk(); return;
  }
  const label = asset.path.split("/").pop() || "reference image";
  const previewPath = resolveProjectPath(asset.path);
  const accepted = await ask(
    `Delete ${character.name}'s saved reference “${label}”?\n\nOnly Studio's project copy will be moved to the project's recoverable trash. The original external source file will not be changed.`,
    { title: "Delete saved character reference", kind: "warning" },
  );
  if (!accepted || !current()) return;
  const invalidateFiles = projectFileInvalidator();
  busy = "Deleting saved character reference…"; actionError = null; renderWorkflowDesk();
  try {
    const result = await invoke<Pick<WorkflowImportResult, "state" | "note">>("workflow_delete_character_reference", {
      projectRoot, characterId, assetId,
    });
    if (!current()) return;
    // Native deletion returns only the updated state and note, not an import
    // report. Keep this session's valid report/navigation without reimporting
    // (which would perform unrelated reconciliation and history writes).
    state = activeProjectState(result.state);
    actionError = null;
    if (previewPath) hooks?.onPathTrashed?.(previewPath);
    invalidateFiles();
    actionNote = `Moved ${label} to recoverable project trash. The original source was not changed.`;
  } catch (error) {
    if (current()) {
      actionError = `Could not delete saved character reference: ${error instanceof Error ? error.message : String(error)}`;
      actionNote = actionError;
    }
  } finally {
    if (current()) { busy = null; renderWorkflowDesk(); }
  }
}

// Owns profile-save and reference events together so unsaved inputs never need assets.
export async function handleCharacterIntent(intent: CharacterIntent): Promise<void> {
  if (intent.type === "generate-character-wardrobe") {
    await runAction("Preparing character wardrobe…", "workflow_generate_character_wardrobe", { characterId: intent.characterKey, prompt: intent.prompt, referenceAssetIds: intent.referenceAssetIds });
    return;
  }
  if (intent.type === "attach-character-wardrobe") {
    const owner = hooks;
    const root = owner?.projectRoot();
    const epoch = modelSheetEpoch;
    if (!owner || !root || busy) return;
    const current = () => hooks === owner && owner.projectRoot() === root && modelSheetEpoch === epoch;
    try {
      const selected = await openDialog({ title: "Attach wardrobe image", multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }] });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (path && current()) await runAction("Attaching character wardrobe…", "workflow_attach_character_wardrobe", { characterId: intent.characterKey, sourcePath: path }, current);
    } catch (error) {
      if (current()) { actionError = error instanceof Error ? error.message : String(error); actionNote = actionError; renderWorkflowDesk(); }
    }
    return;
  }
  if (intent.type === "build-wardrobe-plates") {
    await runAction("Preparing wardrobe plates…", "workflow_generate_wardrobe_plate", { sceneId: intent.sceneId, characterId: intent.characterKey, wardrobeAssetId: intent.wardrobeAssetId });
    return;
  }
  if (intent.type === "delete-wardrobe-plate") {
    const asset = state?.assets?.find(item => item.id === intent.assetId && item.character_id === intent.characterKey && item.role === "wardrobe_plate");
    if (asset) await deleteWardrobePlate(asset);
    return;
  }
  if (intent.type === "create-model-sheet") return handleCreateModelSheet(intent.characterKey, intent.approvalMode);
  if (intent.type === "delete-model-sheet-photo") return handleDeleteModelSheetPhoto(intent.characterKey, intent.assetId);
  if (intent.type === "remove-reference" && intent.characterKey !== "__new__") {
    return handleDeleteCharacterReference(intent.characterKey, intent.assetId);
  }
  const projectRoot = hooks?.projectRoot();
  const invalidateFiles = projectFileInvalidator();
  if (busy || !projectRoot || !state) return;
  const key = intent.type === "save" ? intent.characterId ?? "__new__" : intent.characterKey;
  const draft = characterDrafts.get(key);
  const epoch = modelSheetEpoch; // Clear/load invalidates even a same-root reopen.
  const owner = Symbol("character references");
  characterReferenceOwner = owner;
  const current = () => epoch === modelSheetEpoch && hooks?.projectRoot() === projectRoot
    && characterReferenceOwner === owner;
  const owned: string[] = [];
  let associated = false;
  busy = "Updating character references…";
  actionError = null;
  renderWorkflowDesk();
  try {
    let paths: string[] = [];
    if (intent.type === "add-reference") {
      const selected = await openDialog({ title: "Choose character reference image", multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }] });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!current() || !path) return;
      paths = [path];
    }
    if (key === "__new__" && intent.type !== "save") {
      if (!draft || characterDrafts.get(key) !== draft) return; // Cancel while picker was open.
      if (intent.type === "remove-reference") {
        draft.references = draft.references?.filter(ref => ref.id !== intent.assetId);
      } else if (!draft.references?.some(ref => ref.path === paths[0])) {
        draft.references = [...(draft.references ?? []), { id: paths[0], path: paths[0] }];
      }
      return;
    }
    let characterId = intent.type === "save" ? intent.characterId ?? draft?.savedCharacterId : key;
    if (intent.type === "save") {
      // A failed acquisition/association may have consumed the dialog's one-time
      // grant. Recover at the picker boundary, never by replaying permission.
      // Complete reselection before any profile write; cancel preserves the draft.
      for (const sourcePath of draft?.referenceReselectionPaths ?? []) {
        if (!draft?.references?.some(ref => ref.path === sourcePath)) continue;
        const selected = await openDialog({ title: `Choose the same reference image again: ${sourcePath}`,
          multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }] });
        if (!current() || characterDrafts.get(key) !== draft) return;
        const path = Array.isArray(selected) ? selected[0] : selected;
        if (!path) throw new Error("Reference reselection cancelled. Draft preserved; retry and choose the same reference image again.");
        if (path !== sourcePath) throw new Error("Choose the same reference image again, or remove the staged reference and add a replacement. Draft preserved.");
      }
      // Save the profile first; acquisition requires its real, persisted directory.
      // This transaction owns busy until reference acquisition/association settles.
      // runAction releases it after the profile response, rendering enabled but
      // inert controls while the following reference awaits are still pending.
      stopRoughCut();
      busy = characterId ? "Saving character profile…" : "Creating character…";
      actionNote = null;
      renderWorkflowDesk();
      const saved = await invoke<WorkflowImportResult>("workflow_save_character", {
        projectRoot, name: intent.name, profile: intent.profile, ...(characterId ? { characterId } : {}),
      });
      if (!current()) return;
      busy = "Updating character references…";
      applyHostResult(saved);
      invalidateFiles();
      characterId ??= state?.characters.find(person => person.name === intent.name)?.id;
      if (!characterId) throw new Error("Saved character could not be resolved.");
      if (draft && key === "__new__") draft.savedCharacterId = characterId;
      paths = (draft?.references ?? []).map(ref => ref.path);
    }
    let character = state?.characters.find(person => person.id === characterId);
    if (!character) throw new Error("Character no longer exists.");
    const assetIds = [...(character.reference_asset_ids ?? [])];
    if (intent.type === "remove-reference") {
      const index = assetIds.indexOf(intent.assetId);
      if (index >= 0) assetIds.splice(index, 1);
    }
    for (const sourcePath of paths) {
      // Mark before the call: even a failed decoder/copy can consume authority.
      if (draft && intent.type === "save" && !draft.referenceReselectionPaths?.includes(sourcePath)) {
        draft.referenceReselectionPaths = [...(draft.referenceReselectionPaths ?? []), sourcePath];
      }
      const receipt = await invoke<{ path: string; created: boolean }>("ingest_character_reference_image", {
        projectRoot, characterDir: character.dir, sourcePath, receipt: true,
      });
      // Once replaced, leave any A-side copy for A's native rediscovery/reuse.
      // Do not claim it was associated or delete it through the new session.
      if (!current()) return;
      if (receipt.created) owned.push(receipt.path);
      if (receipt.created) invalidateFiles();
      const imported = await invoke<WorkflowImportResult>("workflow_import", { projectRoot, persist: true });
      if (!current()) return;
      applyHostResult(imported);
      const asset = state?.assets?.find(asset => asset.path === receipt.path && asset.character_id === characterId);
      if (!asset) throw new Error("Acquired reference was not discovered for this character.");
      if (!assetIds.includes(asset.id)) assetIds.push(asset.id);
    }
    if (intent.type !== "save" || paths.length) {
      character = await invoke<CharacterRecord>("workflow_save_character_reference_selection", {
        projectRoot, characterId, assetIds,
      });
      if (!current()) return;
      associated = true;
      // CR02 returns the persisted record, not a WorkflowImportResult.
      if (state) state.characters = state.characters.map(person => person.id === characterId ? character! : person);
    }
    if (intent.type === "save") noteCharacterSaved(intent.characterId);
    actionNote = "Character saved.";
  } catch (error) {
    if (!current()) return;
    let message = error instanceof Error ? error.message : String(error);
    if (!associated) {
      try {
        for (const path of owned) {
          if (!current()) return;
          // Only this acquisition's newly created, still-unselected copies are eligible.
          const asset = state?.assets?.find(asset => asset.path === path);
          if (asset && state?.characters.some(person => person.reference_asset_ids?.includes(asset.id))) continue;
          await invoke("delete_path", { path: `${projectRoot}/${path}` });
          if (!current()) return;
          invalidateFiles();
        }
        if (owned.length) {
          const imported = await invoke<WorkflowImportResult>("workflow_import", { projectRoot, persist: true });
          if (!current()) return;
          applyHostResult(imported);
        }
      } catch (cleanupError) { message += ` Cleanup failed: ${String(cleanupError)}`; }
    }
    if (!current()) return;
    if (draft?.referenceReselectionPaths?.some(path => draft.references?.some(ref => ref.path === path))) {
      message += " To retry, choose the same reference image again when prompted; cancelling keeps your draft.";
    }
    actionError = message;
    actionNote = message;
    hooks?.onStatus("Production", message);
  } finally {
    if (current()) {
      characterReferenceOwner = null;
      busy = null;
      renderWorkflowDesk();
    }
  }
}

function handleAssembleIntent(scene: SceneRecord, intent: AssembleIntent): void {
  switch (intent.type) {
    case "reorder-sequence":
      void runAction("Reordering rendered sequence…", "workflow_reorder_animatic", { sceneId: scene.id, shotIds: intent.shotIds });
      break;
    case "set-shot-trim":
      void runAction("Saving selected-take edit…", "workflow_set_shot_trim", {
        sceneId: scene.id, shotId: intent.shotId,
        trimInSeconds: intent.trimInSeconds, trimOutSeconds: intent.trimOutSeconds,
        expectedTakeId: intent.expectedTakeId, sourceEndSeconds: intent.sourceEndSeconds,
      });
      break;
    case "approve-rendered":
      void runAction("Saving rendered rough-cut review…", "workflow_approve_rendered_sequence", { jobId: intent.jobId });
      break;
    case "prepare-project":
      void runAction("Preparing project assembly…", "workflow_prepare_assemble", {});
      break;
    case "prepare":
      void runAction("Preparing assembly…", "workflow_prepare_assemble", { sceneId: scene.id });
      break;
    case "set-version-stage":
      void runAction(`Updating ${intent.label}`, "workflow_set_cut_version_stage", {
        sceneId: scene.id,
        versionId: intent.versionId,
        stage: intent.stage,
      });
      break;
    case "mark-version":
      void runAction("Marking cut version…", "workflow_mark_cut_version", { sceneId: scene.id });
      break;
    case "lock-picture":
      void runAction("Locking picture…", "workflow_lock_picture", {
        sceneId: scene.id,
        versionId: intent.versionId,
      });
      break;
    case "set-editorial":
      void runAction("Recording editorial path…", "workflow_set_editorial_path", {
        sceneId: scene.id,
        path: intent.path,
        note: intent.note,
      });
      break;
    case "set-post-status":
      void runAction(`Updating ${intent.label}`, "workflow_set_post_status", {
        sceneId: scene.id,
        field: intent.field,
        status: intent.status,
      });
      break;
    case "check-deliverable":
      void runAction("Checking deliverable…", "workflow_check_deliverable", {
        sceneId: scene.id,
        deliverableId: intent.deliverableId,
      });
      break;
    default:
      break;
  }
}

function handleTakeReviewIntent(scene: SceneRecord, intent: TakeReviewIntent): void {
  switch (intent.type) {
    case "refresh":
      void runAction("Refreshing takes…", "workflow_import", { persist: true });
      break;
    case "alternate":
      void runAction("Adding alternate angle…", "workflow_add_alternate_shot", { sceneId: scene.id });
      break;
    case "set-status":
      void confirmNotesTakeStatus(scene, {
        type: "set-take-status",
        cut: intent.cut,
        takeId: intent.takeId,
        status: intent.status,
      });
      break;
    case "delete-take":
      void confirmNotesDeleteTake(scene, {
        type: "delete-take",
        cut: intent.cut,
        takeId: intent.takeId,
        path: intent.path,
      });
      break;
    default:
      break;
  }
}

async function importPerformance(scene: SceneRecord, cutNumber: number): Promise<void> {
  const owner = hooks;
  const projectRoot = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  const cut = scene.cuts.find(item => item.id.cut === cutNumber);
  if (!owner || !projectRoot || !state || !cut || cut.speech_mode !== "source_video_lipsync"
    || busy || operationApprovalOpen() || modelSheetMediaOwners.size) return;
  const shotStableId = cut.stable_id;
  const expectedRevision = state.state_revision;
  const invalidateFiles = projectFileInvalidator();
  const current = () => hooks === owner && owner.projectRoot() === projectRoot && modelSheetEpoch === epoch;
  stopRoughCut();
  busy = "Choosing dialogue WAV…";
  actionNote = null; actionError = null;
  renderWorkflowDesk();
  try {
    const selected = await openDialog({ title: "Import dialogue WAV", directory: false, multiple: false,
      filters: [{ name: "WAV audio", extensions: ["wav"] }] });
    if (!current() || selected === null) return;
    if (typeof selected !== "string" || !selected.trim() || !/\.wav$/i.test(selected)) {
      throw new Error("Choose a single WAV file to import.");
    }
    busy = "Importing dialogue WAV…";
    renderWorkflowDesk();
    const result = await invoke<WorkflowImportResult>("workflow_import_performance", {
      projectRoot, sceneId: scene.id, cut: cutNumber, shotStableId, expectedRevision, sourcePath: selected,
    });
    if (!current()) return;
    applyHostResult(result);
    invalidateFiles();
  } catch (error) {
    if (!current()) return;
    actionError = error instanceof Error ? error.message : String(error);
    actionNote = actionError;
    owner.onStatus("Production", actionError);
  } finally {
    if (current()) { busy = null; renderWorkflowDesk(); }
  }
}

function handleNotesIntent(scene: SceneRecord, intent: NotesIntent): void {
  switch (intent.type) {
    case "generate-dialogue": {
      if (busy) return;
      const epoch = modelSheetEpoch, owner = hooks;
      void runAction("Preparing dialogue audio…", "workflow_prepare_dialogue_audio", {sceneId: scene.id, cut: intent.cut, voiceAssetId: intent.voiceAssetId}, () => modelSheetEpoch === epoch && hooks === owner);
      break;
    }
    case "set-shot-trim":
      handleAssembleIntent(scene, intent);
      break;
    case "import-performance":
      void importPerformance(scene, intent.cut);
      break;
    case "select-performance":
    case "version-audio":
    case "version-pitch":
    case "version-trim":
    case "prepare-performance": {
      if (busy) return;
      const epoch = modelSheetEpoch;
      const owner = hooks;
      const isCurrent = () => modelSheetEpoch === epoch && hooks === owner;
      if (intent.type === "select-performance") {
        void runAction("Saving performance selection…", "workflow_select_performance", {sceneId: scene.id, cut: intent.cut, assetId: intent.assetId, reviewedAssetId: intent.reviewedAssetId ?? null}, isCurrent);
      } else if (intent.type === "version-audio") {
        void runAction("Creating local WAV version…", "workflow_version_audio", {sceneId: scene.id, cut: intent.cut, assetId: intent.assetId, speed: intent.speed}, isCurrent);
      } else if (intent.type === "version-pitch") {
        void runAction("Creating local pitch version…", "workflow_version_audio", {sceneId: scene.id, cut: intent.cut, assetId: intent.assetId, pitch: intent.pitch}, isCurrent);
      } else if (intent.type === "version-trim") {
        void runAction("Creating local trimmed WAV…", "workflow_version_audio", {sceneId: scene.id, cut: intent.cut, assetId: intent.assetId, trim: {startSeconds:intent.startSeconds,endSeconds:intent.endSeconds}}, isCurrent);
      } else {
        void runAction("Preparing video only…", "workflow_prepare_render", {sceneId: scene.id, cut: intent.cut, selectedPerformance: true, reviewedAssetId: intent.reviewedAssetId}, isCurrent);
      }
      break;
    }
    case "sync-all": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Building production video prompts…", "workflow_sync_production_notes", {
        sceneId: scene.id,
        ...chat,
      });
      break;
    }
    case "review-handoff": {
      if (busy || operationApprovalOpen()) return;
      const root = hooks?.projectRoot();
      const owner = hooks;
      const epoch = modelSheetEpoch;
      if (!root) return;
      void (async () => {
        try {
          const preview = await invoke("workflow_preview_production_handoff", { projectRoot: root, sceneId: scene.id, cut: intent.cut }) as HandoffPreview;
          if (hooks !== owner || owner?.projectRoot() !== root || modelSheetEpoch !== epoch) return;
          const accepted = await reviewProductionHandoff(preview, scene.id, intent.cut);
          if (accepted && hooks === owner && owner?.projectRoot() === root && modelSheetEpoch === epoch) {
            void runAction("Refreshing reviewed production note…", "workflow_refresh_production_handoff", {
              sceneId: scene.id, cut: intent.cut, revision: preview.revision,
            }, () => hooks === owner && modelSheetEpoch === epoch);
          }
        } catch (error) {
          if (hooks !== owner || modelSheetEpoch !== epoch) return;
          actionError = error instanceof Error ? error.message : String(error);
          renderWorkflowDesk();
        }
      })();
      break;
    }
    case "refresh-takes":
      void runAction("Refreshing rendered takes…", "workflow_import", { persist: true });
      break;
    case "alternate":
      void runAction("Adding alternate angle…", "workflow_add_alternate_shot", { sceneId: scene.id });
      break;
    case "clear-package":
      void runAction("Marking shot package reviewed…", "workflow_clear_package_review", {
        sceneId: scene.id,
        cut: intent.cut,
      });
      break;
    case "save-prompt":
      void runAction("Saving production prompt…", "workflow_save_production_prompt", {
        sceneId: scene.id,
        cut: intent.cut,
        prompt: intent.prompt,
        negativePrompt: intent.negativePrompt,
      });
      break;
    case "sync-cut": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Building production video prompt…", "workflow_sync_production_notes", {
        sceneId: scene.id,
        cut: intent.cut,
        ...chat,
      });
      break;
    }
    case "resume-audio": {
      const epoch = modelSheetEpoch;
      const owner = hooks;
      void runAction("Preparing saved-audio recovery…", "workflow_prepare_render", { sceneId: scene.id, cut: intent.cut, resumeSavedAudio: true },
        () => modelSheetEpoch === epoch && hooks === owner);
      break;
    }
    case "render":
      void runAction("Preparing render…", "workflow_prepare_render", {
        sceneId: scene.id,
        cut: intent.cut,
      });
      break;
    case "set-take-status":
      void confirmNotesTakeStatus(scene, intent);
      break;
    case "delete-take":
      void confirmNotesDeleteTake(scene, intent);
      break;
    default:
      break;
  }
}

async function confirmNotesTakeStatus(
  scene: SceneRecord,
  intent: Extract<NotesIntent, { type: "set-take-status" }>,
): Promise<void> {
  if (intent.status === "selected") {
    const cut = scene.cuts.find((candidate) => candidate.id.cut === intent.cut);
    const dependents = (cut ? continuityDependents(cut) : []).filter((label) => {
      const match = /Scene (\d+) Cut (\d+)/.exec(label);
      if (!match) return true;
      const dependent = state?.scenes
        ?.find((item) => item.id === Number(match[1]))
        ?.cuts.find((item) => item.id.cut === Number(match[2]));
      return Boolean(dependent?.visual_source?.asset_id);
    });
    if (dependents.length) {
      const confirmed = await ask(
        `Selecting this take unbinds continuity frames on:\n${dependents.join("\n")}\n\nThose shots stay render-blocked until you bind a reviewed frame. This does not approve a replacement.`,
        { title: `Select Scene ${scene.id} Cut ${intent.cut}`, kind: "warning" },
      );
      if (!confirmed) return;
    }
  }
  await runAction("Updating take review…", "workflow_set_take_status", {
    sceneId: scene.id,
    cut: intent.cut,
    takeId: intent.takeId,
    status: intent.status,
  });
}

async function confirmNotesDeleteTake(
  scene: SceneRecord,
  intent: Extract<NotesIntent, { type: "delete-take" }>,
): Promise<void> {
  const confirmed = await ask(
    `Delete ${intent.path}?\n\nThis permanently removes this video take. If it is selected, dependent continuity frames will be unbound.`,
    { title: `Delete Scene ${scene.id} Cut ${intent.cut} take`, kind: "warning" },
  );
  if (!confirmed) return;
  await runAction("Deleting video take…", "workflow_delete_take", {
    sceneId: scene.id,
    cut: intent.cut,
    takeId: intent.takeId,
  });
}

async function confirmDeleteLookSlot(scene: SceneRecord, intent: Extract<ReferenceIntent, { type: "delete-look-slot" }>): Promise<void> {
  const root = hooks?.projectRoot();
  const confirmed = await ask(`Remove the look “${intent.title}” from this scene?\n\nIts image files and assets will be kept. Looks used by cuts or pending work cannot be removed until those uses are cleared.`, { title: "Delete look", kind: "warning" });
  if (!confirmed || !root || hooks?.projectRoot() !== root || selectedScene()?.stable_id !== scene.stable_id) return;
  await runAction("Removing look…", "workflow_delete_look_slot", { sceneId: scene.id, slotId: intent.slotId });
}

function handleReferenceIntent(scene: SceneRecord, intent: ReferenceIntent): void {
  switch (intent.type) {
    case "delete-look-slot":
      void confirmDeleteLookSlot(scene, intent);
      break;
    case "save-description":
      void runAction("Saving reference description…", "workflow_save_reference_description", {
        sceneId: scene.id, slotId: intent.slotId, description: intent.description, category: intent.category,
      });
      break;
    case "set-source": {
      const args: Record<string, unknown> = {
        sceneId: scene.id,
        cut: intent.cut,
        strategy: intent.strategy,
      };
      if (intent.strategy === "previous_selected_take_exit_frame" && intent.predecessorCut != null) {
        args.predecessorCut = intent.predecessorCut;
        args.predecessorScene = intent.predecessorScene;
      }
      if (intent.strategy === "shared_reference" && intent.assetId) {
        args.assetId = intent.assetId;
      }
      void runAction("Saving visual-source strategy…", "workflow_set_visual_source", args);
      break;
    }
    case "bind-frame":
      void confirmBindFrame(scene, intent);
      break;
    case "draft-prompt": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting start-frame prompt…", "workflow_draft_start_frame_prompt", {
        sceneId: scene.id,
        cut: intent.cut,
        ...chat,
      });
      break;
    }
    case "save-prompt":
      void runAction("Saving start-frame prompt…", "workflow_save_start_frame_prompt", {
        sceneId: scene.id,
        cut: intent.cut,
        prompt: intent.prompt,
        negativePrompt: intent.negativePrompt,
      });
      break;
    case "generate":
      void runAction("Preparing start-frame generate…", "workflow_generate_start_frame", {
        sceneId: scene.id,
        cut: intent.cut,
        referenceAssetIds: intent.referenceAssetIds,
      });
      break;
    case "attach-still":
      void attachStartFrame(scene.id, intent.cut);
      break;
    case "approve":
      void runAction("Approving cut start frame…", "workflow_approve_start_frame", {
        sceneId: scene.id,
        cut: intent.cut,
        assetId: intent.assetId,
      });
      break;
    case "reject":
      void runAction("Rejecting cut start frame…", "workflow_reject_start_frame", {
        sceneId: scene.id,
        cut: intent.cut,
        assetId: intent.assetId,
      });
      break;
    case "edit":
      if (!intent.brief.trim()) {
        actionError = `Enter an edit brief under Cut ${intent.cut} first.`;
        actionNote = actionError;
        renderWorkflowDesk();
        return;
      }
      void runAction("Preparing start-frame edit…", "workflow_edit_start_frame", {
        sceneId: scene.id,
        cut: intent.cut,
        assetId: intent.assetId,
        brief: intent.brief.trim(),
      });
      break;
    case "delete":
      void confirmDeleteStartFrame(scene, intent);
      break;
    default:
      break;
  }
}

async function confirmBindFrame(
  scene: SceneRecord,
  intent: Extract<ReferenceIntent, { type: "bind-frame" }>,
): Promise<void> {
  const seconds = intent.frameTimeSeconds;
  if (!Number.isFinite(seconds) || seconds < 0) {
    await ask("Enter a frame time in seconds. The default is near the end of the selected take.", {
      title: "Choose a continuity frame",
      kind: "warning",
    });
    return;
  }
  const cut = scene.cuts.find((candidate) => candidate.id.cut === intent.cut);
  const predecessor = state?.scenes?.find((candidate) => candidate.id === intent.predecessorScene)
    ?.cuts.find((candidate) => candidate.id.cut === intent.predecessorCut);
  const dependents = predecessor ? continuityDependents(predecessor) : [];
  const choice = continuityChoice({
    strategy: "previous_selected_take_exit_frame",
    predecessorTakeSelected: true,
    durationSeconds: seconds,
    editedTimeSeconds: seconds,
    boundAssetId: cut?.visual_source?.asset_id,
    dependentLabels: dependents,
    busy: null,
  });
  if (choice.confirmBeforeReplace) {
    const stale = dependents.length
      ? `These shots will need review:\n${dependents.join("\n")}`
      : "This cut's current continuity frame will be replaced.";
    const confirmed = await ask(
      `Bind the frame at ${seconds}s?\n\n${stale}\n\nThe successor stays render-blocked until this bind finishes.`,
      { title: `Bind Scene ${scene.id} Cut ${intent.cut}`, kind: "warning" },
    );
    if (!confirmed) return;
  }
  await runAction("Binding continuity frame…", "workflow_bind_continuity_frame", {
    sceneId: scene.id,
    cut: intent.cut,
    frameTimeSeconds: seconds,
  });
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

async function confirmDeleteStartFrame(
  scene: SceneRecord,
  intent: Extract<ReferenceIntent, { type: "delete" }>,
): Promise<void> {
  const confirmed = await ask(
    `Delete ${intent.path}?\n\nThis removes this exact cut-image version and its registry entry. Provider job history is retained.`,
    { title: "Delete cut image", kind: "warning" },
  );
  if (!confirmed) return;
  await runAction("Deleting cut image…", "workflow_delete_start_frame", {
    sceneId: scene.id,
    cut: intent.cut,
    assetId: intent.assetId,
  });
}

function handleSketchIntent(scene: SceneRecord, intent: SketchIntent): void {
  switch (intent.type) {
    case "set-max-cuts":
      void runAction("Saving scene cut limit…", "workflow_set_max_cuts", {
        sceneId: scene.id,
        maxCuts: intent.maxCuts,
      });
      break;
    case "set-strategy":
      void runAction("Setting strategy…", "workflow_set_strategy", {
        sceneId: scene.id,
        strategy: intent.strategy,
        override: intent.override,
      });
      break;
    case "alternate":
      void runAction("Adding alternate angle…", "workflow_add_alternate_shot", { sceneId: scene.id });
      break;
    case "resolve-script-dialogue":
      void runAction("Keeping authored cut dialogue…", "workflow_resolve_script_dialogue", {
        sceneId: scene.id, shotId: intent.shotId, choice: intent.choice,
      });
      break;
    case "reopen-sketch":
      void confirmReopenSketch(scene, intent.shotId);
      break;
    case "remove-cut":
      void confirmRemoveCut(scene, intent.shotId);
      break;
    case "backfill":
      void runAction("Backfilling cut sketches…", "workflow_backfill_sketches", {
        sceneId: scene.id,
        replace: true,
      });
      break;
    case "draft": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting cut sketches…", "workflow_draft_sketches", {
        sceneId: scene.id,
        strategy: intent.strategy,
        ...chat,
      });
      break;
    }
    case "approve-animatic":
      void runAction("Approving sketch animatic…", "workflow_approve_animatic", { sceneId: scene.id });
      break;
    case "set-duration":
      void runAction("Saving animatic timing…", "workflow_set_animatic_duration", {
        sceneId: scene.id,
        shotId: intent.shotId,
        durationSeconds: intent.durationSeconds,
      });
      break;
    case "set-shot-trim":
      void runAction("Saving trim…", "workflow_set_shot_trim", {
        sceneId: scene.id,
        shotId: intent.shotId,
        trimInSeconds: intent.trimInSeconds,
        trimOutSeconds: intent.trimOutSeconds,
        expectedTakeId: intent.expectedTakeId,
      });
      break;
    case "set-shot-transition":
      void runAction("Saving transition…", "workflow_set_shot_transition", {
        sceneId: scene.id,
        shotId: intent.shotId,
        transition: intent.transition,
      });
      break;
    case "reorder-animatic":
      void runAction("Reordering animatic…", "workflow_reorder_animatic", {
        sceneId: scene.id,
        shotIds: intent.shotIds,
      });
      break;
    case "attach-scratch":
      void attachScratchAudio(scene.id, intent.shotId);
      break;
    case "clear-scratch":
      void runAction("Removing scratch audio…", "workflow_clear_scratch_audio", {
        sceneId: scene.id,
        shotId: intent.shotId,
      });
      break;
    case "reorder-cuts":
      void runAction("Reordering cuts…", "workflow_reorder_cuts", {
        sceneId: scene.id,
        shotIds: intent.shotIds,
      });
      break;
    case "save-sketch": {
      const cut = scene.cuts.find((item) => intent.shotId
        ? item.stable_id === intent.shotId : item.id.cut === intent.cut);
      if (cut) rememberScene(scene, cut);
      void runAction("Saving cut sketch…", "workflow_save_sketch", {
        sceneId: scene.id,
        cut: intent.cut,
        shotId: intent.shotId,
        sketch: intent.sketch,
      });
      break;
    }
    default:
      break;
  }
}

async function confirmReopenSketch(scene: SceneRecord, shotId: string): Promise<void> {
  const owner = hooks;
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  const cut = scene.cuts.find(item => item.stable_id === shotId);
  const current = () => hooks === owner && modelSheetEpoch === epoch && hooks?.projectRoot() === root
    && selectedScene()?.stable_id === scene.stable_id
    && Boolean(selectedScene()?.cuts.some(item => item.stable_id === shotId));
  if (!root || !cut || busy) return;
  const accepted = await ask(`Reopen the scene's sketch animatic for revision of Cut ${cut.id.cut}? This clears the scene's sketch approval, not rendered-media approval. Nothing is removed. You can then use Remove cut separately for an empty sketch. Media, production notes and dependent shots remain protected.`,
    { title: "Reopen sketch for revision", kind: "warning" });
  if (!accepted || !current()) return;
  await runAction("Reopening sketch for revision…", "workflow_reopen_sketch", { sceneId: scene.id, shotId }, current);
}

async function confirmRemoveCut(scene: SceneRecord, shotId: string): Promise<void> {
  const root = hooks?.projectRoot();
  const cut = scene.cuts.find(item => item.stable_id === shotId);
  if (!root || !cut || busy) return;
  const accepted = await ask(`Remove Cut ${cut.id.cut} from this scene? Its sketch and rough-cut timing will be removed. No files or media will be deleted. Cuts with attached production work or dependent shots are protected.`,
    { title: "Remove cut", kind: "warning" });
  if (!accepted || hooks?.projectRoot() !== root || selectedScene()?.stable_id !== scene.stable_id
      || !selectedScene()?.cuts.some(item => item.stable_id === shotId)) return;
  void runAction("Removing cut…", "workflow_remove_cut", { sceneId: scene.id, shotId },
    () => hooks?.projectRoot() === root && selectedScene()?.stable_id === scene.stable_id);
}

async function attachScratchAudio(sceneId: number, shotId: string): Promise<void> {
  const selected = await openDialog({
    title: "Attach scratch audio",
    multiple: false,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "m4a", "aac", "ogg", "flac"] }],
  });
  const path = Array.isArray(selected) ? selected[0] : selected;
  if (!path) return;
  await runAction("Attaching scratch audio…", "workflow_attach_scratch_audio", {
    sceneId,
    shotId,
    sourcePath: path,
  });
}

function renderSceneIntentWorkspace(scene: SceneRecord): HTMLElement {
  return renderSceneIntentView(
    scene,
    buildIntentModel({
      scene,
      busy,
      error: actionError,
      chatReady: Boolean(hooks?.chatModel()?.modelId),
      blocked: sceneBlockers(scene.id).map((requirement) => requirement.name),
    }),
    (intent) => handleSceneIntent(scene, intent),
  );
}

function handleSceneIntent(scene: SceneRecord, intent: SceneIntentIntent): void {
  switch (intent.type) {
    case "draft": {
      const chat = requireChat();
      if (!chat) return;
      void runAction("Drafting scene intent…", "workflow_draft_intent", {
        sceneId: scene.id,
        ...chat,
      });
      break;
    }
    case "save":
      void runAction("Saving scene intent…", "workflow_save_intent", {
        sceneId: scene.id,
        intent: intent.intent,
      });
      break;
    case "approve":
      void runAction("Approving scene intent…", "workflow_approve_intent", {
        sceneId: scene.id,
      });
      break;
    default:
      break;
  }
}

function renderSceneLookWorkspace(scene: SceneRecord, referenceSlotId?: string): HTMLElement {
  const slot = scene.look_board?.slots?.find((item) => item.id === (referenceSlotId ?? focusedLookSlotId))
    ?? scene.look_board?.slots?.find((item) => item.id === selectedLookSlotId)
    ?? scene.look_board?.slots?.[0];
  const workspace = renderSceneLookView(
    { ...buildSceneLook({
      scene,
      report: sceneReport(scene.id),
      assets: state?.assets ?? [],
      jobs: state?.jobs ?? [],
      characters: (state?.characters ?? []).map((character) => ({ id: character.id, name: character.name })),
      blockers: sceneBlockers(scene.id).map((requirement) => requirement.name),
      focusedSlotId: referenceSlotId ?? focusedLookSlotId,
      focusedAssetId: focusedLookAssetId,
      directorSlotId: selectedLookSlotId,
      identityHint: slot ? lookIdentityHint(scene, slot) : null,
      busy,
      resolvePath: resolveProjectPath,
    }), imageRoutes: hooks?.imageRoutes?.() ?? [] },
    (intent) => handleSceneLookIntent(scene, intent),
    !!referenceSlotId,
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
      void attachLookImage(scene.id, intent.slotId, intent.role);
      break;
    case "crop":
      void cropLookImage(scene.id, intent);
      break;
    case "generate":
      void runAction("Preparing look generate…", "workflow_generate_look", {
        sceneId: scene.id,
        slotId: intent.slotId,
        role: intent.role,
        title: intent.title,
        prompt: intent.prompt,
        negativePrompt: intent.negative,
        attachCharacterImage: intent.attachCharacterImage,
        referenceAssetIds: intent.referenceAssetIds,
        imageCapability: intent.imageCapability,
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
      // Review images live at project-relative paths; the app's file-open
      // hook needs the resolved project path, exactly like review exports.
      hooks?.onOpenPath(intent.path ? resolveProjectPath(intent.path) ?? intent.path : intent.path);
      break;
    case "attach-director":
      if (selectedLookSlotId === intent.slotId) detachDirectorContext("lookSlot", intent.slotId);
      else attachDirectorContext("lookSlot", intent.slotId);
      break;

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

async function cropLookImage(sceneId: number, intent: Extract<SceneLookIntent, { type: "crop" }>): Promise<void> {
  const owner = hooks;
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  if (!owner || !root || busy) return;
  const current = () => hooks === owner && owner.projectRoot() === root && modelSheetEpoch === epoch;
  busy = "Reviewing local crop…";
  renderWorkflowDesk();
  try {
    const url = await toMediaUrlAsync(intent.path);
    if (!current()) return;
    const crop = await reviewImageCrop(url);
    if (!crop || !current()) return;
    busy = "Saving cropped draft…";
    renderWorkflowDesk();
    const result = await invoke<WorkflowImportResult & { editedLook: { assetId: string; sourceAssetId: string } }>("workflow_crop_look", {
      projectRoot: root, sceneId, slotId: intent.slotId, assetId: intent.assetId, ...crop,
    });
    if (!current()) return;
    focusedLookSlotId = intent.slotId;
    focusedLookAssetId = result.editedLook.assetId;
    applyHostResult(result);
    projectFileInvalidator()();
  } catch (error) {
    if (!current()) return;
    actionError = error instanceof Error ? error.message : String(error);
    actionNote = actionError;
    owner.onStatus("Production", actionError);
  } finally {
    if (current()) { busy = null; renderWorkflowDesk(); }
  }
}

async function attachLookImage(sceneId: number, slotId?: string, role?: string): Promise<void> {
  const owner = hooks;
  const invalidateFiles = projectFileInvalidator();
  const root = owner?.projectRoot();
  const epoch = modelSheetEpoch;
  if (!owner || !root || busy) return;
  const current = () => hooks === owner && modelSheetEpoch === epoch && owner.projectRoot() === root;
  stopRoughCut();
  busy = "Choosing look image…";
  actionNote = null;
  actionError = null;
  renderWorkflowDesk();
  try {
    const selected = await openDialog({
      title: "Attach scene look still",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (!current()) return;
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;
    busy = "Attaching look still…";
    renderWorkflowDesk();
    const result = await invoke<WorkflowImportResult & { attachedLook?: { slotId: string; assetId: string } }>("workflow_attach_look", {
      projectRoot: root, sceneId, slotId, role, sourcePath: path,
    });
    if (!current()) return;
    // Select the just-copied version even when this slot has an approved older still.
    const attached = result.attachedLook;
    focusedLookSlotId = attached?.slotId ?? slotId ?? null;
    const slotAssets = result.state.scenes.find(s => s.id === sceneId)?.look_board?.slots?.find(s => s.id === slotId)?.asset_ids ?? [];
    focusedLookAssetId = attached?.assetId ?? slotAssets[slotAssets.length - 1] ?? null;
    applyHostResult(result);
    invalidateFiles();
  } catch (error) {
    if (!current()) return;
    actionError = error instanceof Error ? error.message : String(error);
    actionNote = actionError;
    owner.onStatus("Production", actionError);
  } finally {
    if (current()) {
      busy = null;
      renderWorkflowDesk();
    }
  }
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
  const panel = document.createElement("div");
  panel.className = "studio-jobs-panel";
  const jobs = renderJobsView(
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
  if (jobsOpen) {
    panel.append(jobs);
  } else {
    panel.append(jobs);
  }
  return panel;
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
  if (intent.type === "output-context") {
    if ((row.bucket === "completed" || row.bucket === "awaiting") && row.outputPath) {
      document.dispatchEvent(new CustomEvent("studio-job-output-context", {
        detail: { path: resolveProjectPath(row.outputPath) ?? row.outputPath, x: intent.x, y: intent.y },
      }));
    }
    return;
  }
  if (row.predictionId && row.actionLabel === "Check provider status") {
    void checkProviderJobStatus(row.id);
    return;
  }
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

async function checkProviderJobStatus(jobId: string): Promise<void> {
  const projectRoot = hooks?.projectRoot();
  if (!projectRoot) {
    hooks?.onStatus("Provider status unavailable", "Open a saved project first.");
    return;
  }
  try {
    const raw = await invoke<string>("media_recovery_status", { projectRoot, jobId });
    const result = JSON.parse(raw) as { status?: string };
    hooks?.onStatus("Provider status checked", result.status?.trim() || "unknown");
  } catch (error) {
    hooks?.onStatus("Provider status unavailable", String(error));
  }
}

function renderFinishPanel(): HTMLElement {
  const scene = selectedScene();
  return renderFinishView(
    buildFinishModel({
      scene: scene ?? null,
      jobs: state?.jobs ?? [],
      deliverables: state?.profile?.deliverables ?? [],
      busyReason: busy,
      state,
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
