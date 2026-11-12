/**
 * Phase 6 surface façade.
 *
 * The former 2,589-line monolith is now split along the
 * selector -> view-model -> component boundary:
 *
 * - surfaceSelectors.ts: pure derivation from records (no host, no DOM).
 * - surfaceModels.ts: pure view-model builders (SurfacePresentation).
 * - session.ts: the single owner of surface host binding + transient UI state.
 * - shared.ts: shared presentational primitives (block/missing/action/queues).
 * - surfaceIntents.ts: intent types; effects are handled by workflowDesk.
 * - *Stage.ts: one component module per production surface.
 *
 * This module re-exports the stable surface API so existing importers
 * (workflowDesk, studio acceptance tests) keep working unchanged.
 */
export { bindSurfaceHost, resetSurfaceSession, setProfileOpen, noteCharacterSaved, stopRoughCut } from "./session";
export type {
  ScriptDraftIntent,
  SketchIntent,
  ReferenceIntent,
  NotesIntent,
  TakeReviewIntent,
  AssembleIntent,
  ProfileIntent,
  CharacterIntent,
} from "./surfaceIntents";
export { effectiveVisualSourceStrategy, visualSourceReady } from "./surfaceSelectors";
export { renderReferenceImagesStage } from "./referenceStage";
export { renderSketchStage } from "./sketchStage";
export { renderProductionNotesStage } from "./notesStage";
export { renderRenderStage } from "./renderStage";
export { renderTakeReviewStage } from "./takeReviewStage";
export { renderAssembleStage } from "./assembleStage";
export { renderCharactersStage } from "./charactersStage";
export { renderProjectProfile } from "./profileStage";
export { renderReadiness } from "./readinessStage";
export { renderScriptDraftForm } from "./scriptDraftForm";
export type { ScriptDraft, ScriptDraftText } from "./scriptDraftForm";
