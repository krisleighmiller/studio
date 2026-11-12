import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "../src/markdown";
import { renderFinishView } from "../src/studio/finishView";
import { buildFinishModel } from "../src/studio/finishModel";
import { buildJobsModel, elapsedLabel } from "../src/studio/jobsModel";
import { renderJobsView } from "../src/studio/jobsView";
import { buildSceneList } from "../src/studio/sceneLookModel";
import { sceneHeadingTitle } from "../src/studio/selectors/scenes";
import { renderSceneListView } from "../src/studio/sceneLookView";
import { visibleDirectorSelection, type DirectorSelection } from "../src/studio/selection";
import { clearIntentDraft, renderSceneIntentView } from "../src/studio/sceneIntentView";
import { continuityChoice } from "../src/studio/selectors/continuityChoice";
import { describeDirectorContext } from "../src/studio/selectors/directorContext";
import { buildProjectSummary } from "../src/studio/selectors/project";
import { buildSceneSummaries } from "../src/studio/selectors/scenes";
import { buildWorkspace } from "../src/studio/selectors/workspace";
import {
  bindSurfaceHost,
  renderAssembleStage,
  renderCharactersStage,
  renderProductionNotesStage,
  renderProjectProfile,
  renderRenderStage,
  renderReferenceImagesStage,
  renderSketchStage,
  renderTakeReviewStage,
  resetSurfaceSession,
  setProfileOpen,
} from "../src/studio/surfaces/productionSurfaces";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import { getSurfaceHost } from "../src/studio/surfaces/session";
import { applySurfaceActions } from "../src/studio/surfaceActions";
import { renderSurfaceNav } from "../src/studio/surfaceNav";
import { paintUnknownRecipeStage, stageHasEditor } from "../src/studio/recipeStage";
import { stagesFromRecipe, surfacesForArea, workAreaForStage, type DeskStage } from "../src/studio/surfaces";
import {
  buildAssembleModel,
  buildCharactersModel,
  buildIntentModel,
  buildNotesModel,
  buildReferenceModel,
  buildRenderQueueModel,
  buildSketchModel,
  buildTakeReviewModel,
} from "../src/studio/surfaces/surfaceModels";
import {
  EXAMPLE_RECIPE_STAGE,
  MONTAGE_RECIPE,
  NARRATIVE_RECIPE,
  type CutRecord,
  type JobRecord,
  type ProjectState,
  type SceneRecord,
  type StageId,
  type StageMap,
  stageStatus,
} from "../src/workflowTypes";

const root = join(import.meta.dirname, "..");

function stages(partial: Partial<StageMap> = {}): StageMap {
  return {
    script: "missing",
    characters: "missing",
    scene_intent: "missing",
    scene_look: "missing",
    cut_sketches: "missing",
    reference_images: "missing",
    production_notes: "missing",
    render: "missing",
    take_review: "missing",
    assemble: "missing",
    ...partial,
  };
}

function cut(partial: Partial<CutRecord> & { cut: number }): CutRecord {
  return {
    id: { scene: 1, cut: partial.cut },
    stable_id: partial.stable_id ?? `shot-${partial.cut}`,
    takes: partial.takes ?? [],
    selected_take: partial.selected_take ?? null,
    ...partial,
  } as CutRecord;
}

function scene(partial: Partial<SceneRecord> & { id: number }): SceneRecord {
  return {
    id: partial.id,
    title: partial.title ?? `Scene ${partial.id}`,
    stable_id: partial.stable_id ?? `scene-${partial.id}`,
    stages: partial.stages ?? stages(),
    cuts: partial.cuts ?? [],
    dialogue: partial.dialogue ?? [],
    ...partial,
  } as SceneRecord;
}

function desk(items: Array<Partial<DeskStage> & { id: string; workArea: DeskStage["workArea"] }>): DeskStage[] {
  return items.map((item) => ({
    label: item.label ?? item.id,
    available: item.available ?? true,
    disabledReason: item.disabledReason ?? null,
    ...item,
  }));
}

describe("phase 1 remote preview images", () => {
  it("strips remote image sources and keeps a local preview", () => {
    const html = renderMarkdown("![remote](https://evil.test/a.png)\n![local](./stills/look.png)");
    expect(html).not.toContain("https://evil.test");
    expect(html).toContain("./stills/look.png");
  });
});

describe("phase 4 continuity frame", () => {
  it("defaults the frame near the end and keeps the successor blocked until it is bound", () => {
    const pending = continuityChoice({
      strategy: "previous_selected_take_exit_frame",
      predecessorTakeSelected: true,
      durationSeconds: 2,
      editedTimeSeconds: null,
      boundAssetId: null,
      dependentLabels: ["Scene 1 Cut 3"],
      busy: null,
    });
    expect(pending.timeSeconds).toBeCloseTo(1.9);
    expect(pending.bindEnabled).toBe(true);
    expect(pending.successorBlocked).toBe(true);
    expect(pending.confirmBeforeReplace).toBe(true);

    const missingTake = continuityChoice({
      strategy: "previous_selected_take_exit_frame",
      predecessorTakeSelected: false,
      durationSeconds: 2,
      editedTimeSeconds: null,
      boundAssetId: null,
      dependentLabels: [],
      busy: null,
    });
    expect(missingTake.bindEnabled).toBe(false);
    expect(missingTake.bindReason).toContain("Select a take");
    expect(missingTake.successorBlocked).toBe(true);

    const bound = continuityChoice({
      strategy: "previous_selected_take_exit_frame",
      predecessorTakeSelected: true,
      durationSeconds: 2,
      editedTimeSeconds: 1.25,
      boundAssetId: "frame_1",
      dependentLabels: [],
      busy: null,
    });
    expect(bound.timeSeconds).toBe(1.25);
    expect(bound.successorBlocked).toBe(false);
    expect(bound.confirmBeforeReplace).toBe(true);
  });
});

describe("phase 4 alternate angle", () => {
  it("offers a new shot on take review, separate from marking a clip", () => {
    const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: true,
      state: null,
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async (_label, cmd, args) => {
        calls.push({ cmd, args });
        return true;
      },
      blockerNames: () => [],
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      continuityDependents: () => [],
      reviewExportPath: () => null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; status?: string }> = [];
    const view = renderTakeReviewStage(
      scene({
        id: 1,
        cuts: [
          cut({
            cut: 1,
            title: "Rain on the Last Train",
            takes: [{ id: "take_1", path: "Scenes/Scene1/Cut1/a.mp4", status: "selected", notes: null }],
          }),
        ],
      }),
      (intent) => intents.push(intent),
    );
    const create = [...view.querySelectorAll("button")].find((button) => button.textContent === "New alternate angle");
    const mark = [...view.querySelectorAll("button")].find((button) => button.textContent === "Mark alternate");
    expect(create).toBeTruthy();
    expect(mark).toBeTruthy();
    expect(view.textContent).toContain("New alternate angle adds a shot");
    create?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    mark?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(calls).toEqual([]);
    expect(intents).toEqual([
      { type: "alternate" },
      { type: "set-status", cut: 1, takeId: "take_1", status: "alternate" },
    ]);

    const sameScene = scene({
      id: 1,
      cuts: [
        cut({
          cut: 1,
          title: "Rain on the Last Train",
          takes: [{ id: "take_1", path: "Scenes/Scene1/Cut1/a.mp4", status: "selected", notes: null }],
        }),
      ],
    });
    for (const view of [
      renderProductionNotesStage(sameScene, () => undefined),
      renderRenderStage(sameScene, () => undefined),
    ]) {
      const createOnVideo = [...view.querySelectorAll("button")].find(
        (button) => button.textContent === "New alternate angle",
      );
      const markOnVideo = [...view.querySelectorAll("button")].find(
        (button) => button.textContent === "Mark alternate",
      );
      expect(createOnVideo).toBeTruthy();
      expect(markOnVideo).toBeTruthy();
      expect(view.textContent).toContain("New alternate angle adds a shot");
    }
  });
});

describe("phase 4 presentation and job truth", () => {
  it("shows unknown elapsed time and unknown cost instead of zero", () => {
    expect(elapsedLabel({ status: "completed" })).toBe("unknown");
    const model = buildJobsModel({
      jobs: [
        {
          id: "job-render",
          kind: "render",
          status: "completed",
          output_path: "Scenes/Scene1/Cut1/take.mp4",
          scope: { kind: "shot", id: "shot-1" },
        } as JobRecord,
      ],
      scenes: [scene({ id: 1, cuts: [cut({ cut: 1, stable_id: "shot-1" })] })],
      assets: [],
      filter: "all",
    });
    expect(model.rows[0]?.elapsed).toBe("unknown");
    expect(model.rows[0]?.created).toBe("unknown");
    expect(model.rows[0]?.started).toBe("unknown");
    expect(model.rows[0]?.completed).toBe("unknown");
    expect(model.rows[0]?.scope).toBe("shot shot-1");
    expect(model.rows[0]?.progress).toBe("unknown");
    expect(model.rows[0]?.retry).toBe("unknown");
    const view = renderJobsView(model, true, "all", () => {});
    const text = view.textContent ?? "";
    expect(text).toContain("Created unknown");
    expect(text).toContain("Started unknown");
    expect(text).toContain("Completed unknown");
    expect(text).toContain("Scope shot shot-1");
    expect(text).toContain("Progress unknown");
    expect(text).toContain("Retry unknown");
    expect(model.rows[0]?.outputPath).toBe("Scenes/Scene1/Cut1/take.mp4");
    expect(model.rows[0]?.cut).toBe(1);
    expect(model.rows[0]?.stage).toBe("take_review");
  });

  it("shows the recorded start time and live progress instead of unknown", () => {
    const model = buildJobsModel({
      jobs: [
        {
          id: "job-render",
          kind: "render",
          status: "completed",
          created_at: "2026-01-01T00:00:00Z",
          started_at: "2026-01-01T00:01:00Z",
          completed_at: "2026-01-01T00:01:30Z",
          progress: "completed",
          scope: { kind: "shot", id: "shot-1" },
          output_path: "Scenes/Scene1/Cut1/take.mp4",
        } as JobRecord,
      ],
      scenes: [scene({ id: 1, cuts: [cut({ cut: 1, stable_id: "shot-1" })] })],
      assets: [],
      filter: "all",
    });
    expect(model.rows[0]?.created).toBe("2026-01-01T00:00:00Z");
    expect(model.rows[0]?.started).toBe("2026-01-01T00:01:00Z");
    expect(model.rows[0]?.completed).toBe("2026-01-01T00:01:30Z");
    expect(model.rows[0]?.progress).toBe("completed");
    // Elapsed prefers the real start; with neither timestamp it reports unknown.
    expect(elapsedLabel({ status: "completed" })).toBe("unknown");
    expect(
      elapsedLabel({
        status: "completed",
        created_at: "2026-01-01T00:00:00Z",
        started_at: "2026-01-01T00:01:00Z",
        completed_at: "2026-01-01T00:01:30Z",
      }),
    ).toBe("30s");
    const view = renderJobsView(model, true, "all", () => {});
    const text = view.textContent ?? "";
    expect(text).toContain("Started 2026-01-01T00:01:00Z");
    expect(text).toContain("Progress completed");
    expect(text).not.toContain("Started unknown");
  });

  it("links an assembly job to the review export", () => {
    const model = buildJobsModel({
      jobs: [
        {
          id: "job-assemble",
          kind: "assemble",
          status: "completed",
          output_path: "Final/Scene1_master.mp4",
          scope: { kind: "scene", id: "scene-1" },
        } as JobRecord,
      ],
      scenes: [scene({ id: 1 })],
      assets: [],
      filter: "completed",
    });
    expect(model.rows[0]?.outputKind).toBe("review");
    expect(model.rows[0]?.actionLabel).toBe("Open review export");
    expect(model.counts.completed).toBe(1);
  });

  it("keeps Director scope on the stable shot when the display number changes", () => {
    const selection: DirectorSelection = {
      sceneId: 1,
      sceneStableId: "scene-1",
      cut: 4,
      shotStableId: "shot-stable",
      assetId: null,
      lookSlotId: null,
      requirementId: null,
      workArea: "produce",
      stage: "take_review",
    };
    const reordered = { ...selection, cut: 2 };
    expect(reordered.shotStableId).toBe("shot-stable");
    const hidden = visibleDirectorSelection(reordered, new Set(["shot:shot-stable"]));
    expect(hidden.shotStableId).toBeNull();
    expect(hidden.sceneStableId).toBe("scene-1");
    const state = {
      scenes: [scene({ id: 1, stable_id: "scene-1", title: "The last train" })],
    } as ProjectState;
    const context = describeDirectorContext(state, selection, "Take Review");
    expect(context.headline).toContain("Scene 1");
    expect(context.headline).toContain("Take Review");
    expect(context.scopeText).toContain("shot-stable");
  });

  it("updates the visible primary when a look version is approved", () => {
    const base = scene({
      id: 1,
      look_board: {
        slots: [{ id: "slot-mood", role: "mood_reference", asset_ids: ["asset-a", "asset-b"], approved_asset_id: null }],
      } as SceneRecord["look_board"],
    });
    const pending = buildWorkspace({ scene: base, cut: null, stage: "scene_look", lookSlotId: "slot-mood", busy: null });
    expect(pending.primary).toBe("No approved look");
    expect(pending.approval).toBe("pending");
    expect(pending.actions[0]?.enabled).toBe(true);
    const approved = buildWorkspace({
      scene: scene({
        id: 1,
        look_board: {
          slots: [{ id: "slot-mood", role: "mood_reference", asset_ids: ["asset-a", "asset-b"], approved_asset_id: "asset-b" }],
        } as SceneRecord["look_board"],
      }),
      cut: null,
      stage: "scene_look",
      lookSlotId: "slot-mood",
      busy: null,
    });
    expect(approved.primary).toBe("Approved asset-b");
    expect(approved.approval).toBe("approved");
    expect(approved.stableId).toBe("slot-mood");
  });
});

describe("phase 5 shell selectors", () => {
  it("never reports Finish complete while a required selected take is missing", () => {
    const model = buildFinishModel({
      scene: scene({
        id: 1,
        cuts: [cut({ cut: 1, takes: [{ id: "take-1", path: "take.mp4", status: "pending" } as CutRecord["takes"][number]], selected_take: null })],
        post: { versions: [{ id: "v1", label: "v1", assembly_path: "Final/Scene1_master.mp4", stage: "assembly" }] } as SceneRecord["post"],
      }),
      jobs: [],
      deliverables: [],
      busyReason: null,
      state: null,
    });
    expect(model.headline).not.toBe("Complete");
    expect(model.items.some((item) => item.state === "complete")).toBe(false);
    expect(model.canAssemble).toBe(false);
    expect(model.assembleReason).toContain("Cut 1");
    expect(model.reviewExportPath).toBe("Final/Scene1_master.mp4");
  });

  it("selects a scene by stable identity", () => {
    const state = {
      scenes: [
        scene({ id: 2, stable_id: "scene-b", title: "Later" }),
        scene({ id: 1, stable_id: "scene-a", title: "The last train" }),
      ],
      requirements: [],
      characters: [],
    } as unknown as ProjectState;
    const summaries = buildSceneSummaries({ state, reports: [], assets: [], resolvePath: () => null });
    expect(summaries.map((item) => item.key)).toEqual(["scene-b", "scene-a"]);
    const list = buildSceneList({
      scenes: state.scenes,
      reports: [],
      assets: [],
      selectedKey: "scene-a",
      blockerCounts: new Map(),
      resolvePath: () => null,
    });
    expect(list.find((item) => item.active)?.key).toBe("scene-a");
    expect(list.find((item) => item.active)?.title).toBe("The last train");
  });

  it("counts blockers and leaves unknown references unknown", () => {
    const summary = buildProjectSummary({
      state: {
        project_name: "Lite_Challenge",
        profile: { recipe: NARRATIVE_RECIPE, deliverables: [] },
        requirements: [
          { id: "req-1", name: "Red lantern", kind: "prop", status: "missing", scene_ids: [1] },
          { id: "req-2", name: "Unscouted room", kind: "location", status: "missing", scene_ids: [1] },
        ],
        scenes: [],
      } as unknown as ProjectState,
      report: { recipe: { id: NARRATIVE_RECIPE, label: "Narrative dialogue", stages: [] }, warnings: [], scenes: [] } as never,
      workArea: "plan",
    });
    expect(summary.recipeLabel).toBe("Narrative dialogue");
    expect(summary.blockers).not.toContain("Red lantern");
    expect(summary.blockers).toContain("Unscouted room");
    const scenes = buildSceneSummaries({
      state: { scenes: [scene({ id: 1 })], requirements: [], characters: [] } as unknown as ProjectState,
      reports: [],
      assets: [],
      resolvePath: () => null,
    });
    expect(scenes[0]?.references).toBe("unknown");
  });
});

describe("phase 6 scene intent", () => {
  it("saves and approves from the view, and keeps a blocked draft from firing", () => {
    clearIntentDraft();
    const intents: Array<{ type: string; intent?: { narrative_purpose?: string | null } }> = [];
    const view = renderSceneIntentView(
      scene({
        id: 1,
        intent: { narrative_purpose: "Leave the station" },
        intent_approved: false,
      }),
      buildIntentModel({
        scene: scene({ id: 1, intent: { narrative_purpose: "Leave the station" } }),
        busy: null,
        error: null,
        chatReady: false,
        blocked: [],
      }),
      (intent) => intents.push(intent),
    );
    const draft = [...view.querySelectorAll("button")].find((button) => button.textContent === "Draft from script");
    const save = [...view.querySelectorAll("button")].find((button) => button.textContent === "Save draft");
    const approve = [...view.querySelectorAll("button")].find((button) => button.textContent === "Approve intent");
    expect(draft?.disabled).toBe(true);
    expect(draft?.title).toContain("chat model");
    expect(save?.disabled).toBe(false);
    expect(approve?.disabled).toBe(false);
    const purpose = [...view.querySelectorAll("textarea")][0];
    expect(purpose).toBeTruthy();
    if (purpose) {
      purpose.value = "Catch the last train";
      purpose.dispatchEvent(new Event("input"));
    }
    draft?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    save?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    approve?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.map((item) => item.type)).toEqual(["save", "approve"]);
    expect(intents[0]?.intent?.narrative_purpose).toBe("Catch the last train");
    expect(view.innerHTML).not.toContain("workflow_save_intent");
    expect(view.innerHTML).not.toContain("invoke");
  });
});

describe("phase 6 cut sketches", () => {
  it("emits sketch intents and leaves the host command to the desk", () => {
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: false,
      state: null,
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async () => {
        throw new Error("sketch view called the host");
      },
      requireChat: () => null,
      rerender: () => undefined,
      setError: () => undefined,
      rememberScene: () => undefined,
      recipeRequires: () => false,
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      ask: async () => false,
      pickFile: async () => null,
      continuityDependents: () => [],
      reviewExportPath: () => null,
      activeRecipe: () => null,
      requirementKindLabel: () => "",
      requirementBlocks: () => false,
      blockerNames: () => [],
      hooks: null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; strategy?: string; maxCuts?: number; shotId?: string; sketch?: { title: string | null } }> = [];
    const view = renderSketchStage(
      scene({
        id: 1,
        intent: { narrative_purpose: "Leave the station" },
        cuts: [cut({ cut: 1, title: "Window", sketch: { title: "Window", visual_prompt: "Rain" } })],
      }),
      undefined,
      (intent) => intents.push(intent),
      () => undefined,
    );
    const draft = [...view.querySelectorAll("button")].find((button) => button.textContent === "Draft sketches");
    const save = [...view.querySelectorAll("button")].find((button) => button.textContent === "Save sketch");
    const remove = [...view.querySelectorAll("button")].find((button) => button.textContent === "Remove cut");
    expect(remove?.disabled).toBe(false);
    remove?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(draft?.disabled).toBe(true);
    expect(draft?.title).toContain("chat model");
    draft?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    const strategy = view.querySelector("select");
    expect(strategy).toBeTruthy();
    if (strategy) {
      strategy.value = "performance_heavy";
      strategy.dispatchEvent(new Event("change"));
    }
    const limit = view.querySelector<HTMLSelectElement>('select[aria-label="Maximum cuts in this scene"]');
    expect(limit?.value).toBe("12");
    if (limit) {
      limit.value = "2";
      limit.dispatchEvent(new Event("change"));
    }
    const titleLabel = [...view.querySelectorAll("label")].find((label) => label.querySelector("span")?.textContent === "Title");
    const title = titleLabel?.querySelector("input");
    if (title instanceof HTMLInputElement) {
      title.value = "Rain on the glass";
      title.dispatchEvent(new Event("input"));
    }
    save?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.map((item) => item.type)).toEqual(["remove-cut", "set-strategy", "set-max-cuts", "save-sketch"]);
    expect(intents[0]?.shotId).toBe("shot-1");
    expect(intents[1]?.strategy).toBe("performance_heavy");
    expect(intents[2]?.maxCuts).toBe(2);
    expect(intents[3]?.sketch?.title).toBe("Rain on the glass");
  });
});

describe("phase 6 reference images", () => {
  it("saves a scene reference description without invoking the host", () => {
    const intents: Array<{ type: string; description?: string; category?: string; slotId?: string | null }> = [];
    const view = renderReferenceImagesStage(scene({ id: 1, cuts: [] }), undefined, (intent) => intents.push(intent));
    const description = view.querySelector<HTMLTextAreaElement>('textarea[aria-label="New reference description"]');
    expect(description).not.toBeNull();
    description!.value = "Station rain";
    const save = [...view.querySelectorAll("button")].find((button) => button.textContent === "Add description");
    expect(save).toBeDefined();
    save!.click();
    expect(intents).toEqual([{ type: "save-description", slotId: null, description: "Station rain", category: "user_defined" }]);
    expect(view.innerHTML).not.toContain("invoke");
  });
});

describe("phase 6 cut sketch reference image controls", () => {
  it("emits source and prompt intents and leaves the host command to the desk", () => {
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: false,
      state: { assets: [] },
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async () => {
        throw new Error("reference view called the host");
      },
      requireChat: () => null,
      rerender: () => undefined,
      setError: () => undefined,
      rememberScene: () => undefined,
      recipeRequires: () => false,
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      ask: async () => false,
      pickFile: async () => null,
      continuityDependents: () => [],
      reviewExportPath: () => null,
      activeRecipe: () => null,
      requirementKindLabel: () => "",
      requirementBlocks: () => false,
      blockerNames: () => [],
      hooks: null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; strategy?: string; prompt?: string }> = [];
    const view = renderSketchStage(
      scene({
        id: 1,
        cuts: [cut({ cut: 1, title: "Window", start_frame_prompt: "Wet glass" })],
      }),
      undefined,
      () => undefined,
      (intent) => intents.push(intent),
    );
    const draft = [...view.querySelectorAll("button")].find((button) => button.textContent === "Draft image prompt");
    const savePrompt = [...view.querySelectorAll("button")].find((button) => button.textContent === "Save prompt");
    const saveSource = [...view.querySelectorAll("button")].find((button) => button.textContent === "Save source strategy");
    expect(draft?.disabled).toBe(true);
    expect(draft?.title).toContain("chat model");
    draft?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    const prompt = [...view.querySelectorAll("label")].find((label) => label.textContent?.includes("Start-frame prompt"))?.querySelector("textarea");
    if (prompt instanceof HTMLTextAreaElement) prompt.value = "Station rain";
    savePrompt?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    saveSource?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.map((item) => item.type)).toEqual(["save-prompt", "set-source"]);
    expect(intents[0]?.prompt).toBe("Station rain");
    expect(intents[1]?.strategy).toBe("preproduction_start_frame");
    expect(view.innerHTML).not.toContain("workflow_set_visual_source");
    expect(view.innerHTML).not.toContain("invoke");

    const host = getSurfaceHost();
    host.chatReady = true;
    const renderDraft = () => {
      const next = renderSketchStage(scene({ id: 1, cuts: [cut({ cut: 1 })] }), undefined, () => undefined, (intent) => intents.push(intent));
      return [...next.querySelectorAll("button")].find((button) => button.textContent === "Draft image prompt")!;
    };
    const readyDraft = renderDraft();
    expect(readyDraft.disabled).toBe(false);
    readyDraft.click();
    expect(intents.at(-1)?.type).toBe("draft-prompt");
    host.busy = "Saving";
    const busyDraft = renderDraft();
    expect(busyDraft.disabled).toBe(true);
    expect(busyDraft.title).toBe("Saving");
    busyDraft.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.filter((intent) => intent.type === "draft-prompt")).toHaveLength(1);
  });
});

describe("phase 6 production notes", () => {
  it("emits prompt intents and leaves the host command to the desk", () => {
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: false,
      state: null,
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async () => {
        throw new Error("notes view called the host");
      },
      requireChat: () => null,
      rerender: () => undefined,
      setError: () => undefined,
      rememberScene: () => undefined,
      recipeRequires: () => false,
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      ask: async () => false,
      pickFile: async () => null,
      continuityDependents: () => [],
      reviewExportPath: () => null,
      activeRecipe: () => null,
      requirementKindLabel: () => "",
      requirementBlocks: () => false,
      blockerNames: () => [],
      hooks: null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; prompt?: string }> = [];
    const view = renderProductionNotesStage(
      scene({
        id: 1,
        cuts: [
          cut({
            cut: 1,
            title: "Window",
            production_note_path: null,
            sketch: { title: "Window", visual_prompt: "Rain" },
            prompt: null,
          }),
        ],
      }),
      (intent) => intents.push(intent),
    );
    const build = [...view.querySelectorAll("button")].find(
      (button) => button.textContent === "Build all production video prompts",
    );
    const save = [...view.querySelectorAll("button")].find((button) => button.textContent === "Save production prompt");
    const alternate = [...view.querySelectorAll("button")].find((button) => button.textContent === "New alternate angle");
    expect(build?.disabled).toBe(true);
    expect(save?.disabled).toBe(false);
    expect(build?.title).toContain("chat model");
    build?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    const prompt = view.querySelector("textarea");
    if (prompt instanceof HTMLTextAreaElement) prompt.value = "The train leaves in the rain";
    save?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    alternate?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.map((item) => item.type)).toEqual(["save-prompt", "alternate"]);
    expect(intents[0]?.prompt).toBe("The train leaves in the rain");
    expect(view.innerHTML).not.toContain("workflow_sync_production_notes");
    expect(view.innerHTML).not.toContain("invoke");
  });

  it("emits a render request from the queue without calling the host", () => {
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: true,
      state: null,
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async () => {
        throw new Error("render view called the host");
      },
      requireChat: () => null,
      rerender: () => undefined,
      setError: () => undefined,
      rememberScene: () => undefined,
      recipeRequires: () => false,
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      ask: async () => false,
      pickFile: async () => null,
      continuityDependents: () => [],
      reviewExportPath: () => null,
      activeRecipe: () => null,
      requirementKindLabel: () => "",
      requirementBlocks: () => false,
      blockerNames: () => [],
      hooks: null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; cut?: number }> = [];
    const view = renderRenderStage(
      scene({
        id: 1,
        cuts: [
          cut({
            cut: 1,
            title: "Window",
            production_note_path: "Scenes/Scene1/Cut1/prod.md",
            prompt: "Rain on the glass",
            visual_source: { strategy: "no_start_frame" },
          }),
        ],
      }),
      (intent) => intents.push(intent),
    );
    const render = [...view.querySelectorAll("button")].find((button) => button.textContent === "Render Cut 1 new take");
    expect(render?.disabled).toBe(false);
    render?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toEqual([{ type: "render", cut: 1 }]);
    expect(view.innerHTML).not.toContain("workflow_prepare_render");
  });
});

describe("phase 6 assemble", () => {
  it("emits assembly intents and leaves the host command to the desk", () => {
    bindSurfaceHost({
      busy: null,
      error: null,
      chatReady: true,
      state: { profile: { deliverables: [] }, assets: [] },
      report: null,
      selectedCut: null,
      selectedSceneId: 1,
      selectedRequirementId: null,
      run: async () => {
        throw new Error("assemble view called the host");
      },
      requireChat: () => null,
      rerender: () => undefined,
      setError: () => undefined,
      rememberScene: () => undefined,
      recipeRequires: () => false,
      resolvePath: (path) => path ?? null,
      mediaUrl: async (path: string) => path,
      ask: async () => false,
      pickFile: async () => null,
      continuityDependents: () => [],
      reviewExportPath: () => null,
      activeRecipe: () => null,
      requirementKindLabel: () => "",
      requirementBlocks: () => false,
      blockerNames: () => [],
      hooks: null,
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; path?: string; note?: string }> = [];
    const view = renderAssembleStage(
      scene({
        id: 1,
        cuts: [
          cut({
            cut: 1,
            selected_take: "take_1",
            takes: [{ id: "take_1", path: "Scenes/Scene1/Cut1/a.mp4", status: "selected", notes: null }],
          }),
        ],
      }),
      (intent) => intents.push(intent),
    );
    const prepare = [...view.querySelectorAll("button")].find((button) => button.textContent === "Prepare Scene 1 master");
    const editorial = view.querySelector<HTMLSelectElement>('select[aria-label="Editorial path"]');
    const note = view.querySelector<HTMLInputElement>('input[aria-label="External NLE note"]');
    const record = [...view.querySelectorAll("button")].find((button) => button.textContent === "Record editorial path");
    expect(prepare?.disabled).toBe(false);
    prepare?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    if (editorial) {
      editorial.value = "external_nle";
      editorial.dispatchEvent(new Event("change"));
    }
    if (note) note.value = "Cut in the NLE";
    record?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents.map((item) => item.type)).toEqual(["prepare", "set-editorial"]);
    expect(intents[1]?.path).toBe("external_nle");
    expect(intents[1]?.note).toBe("Cut in the NLE");
    expect(view.innerHTML).not.toContain("workflow_prepare_assemble");
  });
});

function commandHost(): SurfaceHost {
  return {
    busy: null,
    error: null,
    chatReady: true,
    state: null,
    report: null,
    selectedCut: null,
    selectedSceneId: 1,
    selectedRequirementId: null,
    run: async () => {
      throw new Error("view called the host");
    },
    requireChat: () => null,
    rerender: () => undefined,
    setError: () => undefined,
    rememberScene: () => undefined,
    recipeRequires: () => false,
    resolvePath: (path) => path ?? null,
    mediaUrl: async (path: string) => path,
    ask: async () => false,
    pickFile: async () => null,
    continuityDependents: () => [],
    reviewExportPath: () => null,
    activeRecipe: () => ({ id: "narrative_dialogue", label: "Narrative dialogue" }),
    requirementKindLabel: (kind: string) => kind,
    requirementBlocks: () => false,
    blockerNames: () => [],
    hooks: null,
  } as unknown as SurfaceHost;
}

describe("phase 6 profile and characters", () => {
  it("emits profile and requirement intents and leaves the host command to the desk", () => {
    resetSurfaceSession();
    setProfileOpen(true);
    bindSurfaceHost({
      ...commandHost(),
      state: {
        project_name: "Lite",
        scenes: [],
        profile: {
          recipe: "narrative_dialogue",
          deliverables: [{ id: "d1", name: "Short" }],
        },
        requirements: [
          { id: "req_1", name: "Station", kind: "location", status: "missing", scene_ids: [1] },
        ],
      },
    } as unknown as SurfaceHost);
    const intents: Array<{ type: string; name?: string; kind?: string; status?: string; profile?: { deliverables?: Array<{ name: string }> } }> = [];
    const view = renderProjectProfile((intent) => intents.push(intent));
    const deliverable = view.querySelector<HTMLInputElement>('input[name="name"]');
    expect(deliverable).toBeTruthy();
    if (deliverable) deliverable.value = "Festival cut";
    view.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    const status = view.querySelector<HTMLSelectElement>('select[aria-label="Station status"]');
    if (status) {
      status.value = "approved";
      status.dispatchEvent(new Event("change"));
    }
    const addName = view.querySelector<HTMLInputElement>('input[aria-label="Requirement name"]');
    const addKind = view.querySelector<HTMLSelectElement>('select[aria-label="Requirement kind"]');
    if (addName) addName.value = "Lantern";
    if (addKind) addKind.value = "prop";
    view.querySelector(".workflow-requirement-add")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(intents.map((item) => item.type)).toEqual(["save", "set-requirement", "add-requirement"]);
    expect(intents[0]?.profile?.deliverables?.[0]?.name).toBe("Festival cut");
    expect(intents[1]?.status).toBe("approved");
    expect(intents[2]?.name).toBe("Lantern");
    expect(intents[2]?.kind).toBe("prop");
    expect(view.innerHTML).not.toContain("workflow_save_profile");
    expect(view.innerHTML).not.toContain("invoke");
  });

  it("emits a character save and leaves the host command to the desk", () => {
    resetSurfaceSession();
    bindSurfaceHost(commandHost());
    const intents: Array<{ type: string; name?: string; characterId?: string | null }> = [];
    const onIntent = (intent: { type: string; name?: string; characterId?: string | null }) => intents.push(intent);
    let view = renderCharactersStage(onIntent);
    const open = [...view.querySelectorAll("button")].find((button) => button.textContent === "New character");
    open?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    view = renderCharactersStage(onIntent);
    const nameLabel = [...view.querySelectorAll("label")].find((label) => label.querySelector("span")?.textContent === "Character name");
    const name = nameLabel?.querySelector("input");
    if (name instanceof HTMLInputElement) {
      name.value = "Mara";
      name.dispatchEvent(new Event("input"));
    }
    const create = [...view.querySelectorAll("button")].find((button) => button.textContent === "Create character");
    expect(create?.disabled).toBe(false);
    create?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toHaveLength(1);
    expect(intents[0]?.type).toBe("save");
    expect(intents[0]?.name).toBe("Mara");
    expect(intents[0]?.characterId).toBeNull();
    expect(view.innerHTML).not.toContain("workflow_save_character");
  });
});

describe("phase 6 surface view models", () => {
  const blocked = ["Location"];
  const emptyScene = scene({ id: 1 });

  it("gives every production surface an empty, loading, error, or blocked presentation", () => {
    const intent = buildIntentModel({ scene: emptyScene, busy: null, error: null, chatReady: false, blocked });
    expect(intent.tone).toBe("blocked");
    expect(intent.actions.find((item) => item.id === "draft")?.enabled).toBe(false);
    expect(intent.actions.find((item) => item.id === "draft")?.reason).toContain("chat model");

    const loading = buildSketchModel({ scene: emptyScene, report: undefined, busy: "Saving sketches", error: null, chatReady: true, blocked: [] });
    expect(loading.tone).toBe("loading");
    expect(loading.actions.every((item) => item.enabled === false)).toBe(true);

    const failed = buildNotesModel({ scene: emptyScene, busy: null, error: "The note could not be saved.", blocked: [], chatReady: true });
    expect(failed.tone).toBe("error");
    expect(failed.error).toContain("could not be saved");

    const references = buildReferenceModel({ scene: emptyScene, report: undefined, busy: null, error: null, blocked: [], chatReady: true });
    expect(references.tone === "empty" || references.emptyTitle || references.banner).toBeTruthy();

    const queue = buildRenderQueueModel({ scene: emptyScene, busy: null, error: null, blocked: [] });
    expect(queue.emptyTitle || queue.tone === "empty" || queue.banner).toBeTruthy();

    const review = buildTakeReviewModel({ scene: emptyScene, busy: null, error: null, blocked: [] });
    expect(review.emptyTitle).toContain("No takes");

    const assemble = buildAssembleModel({ scene: emptyScene, busy: null, error: null, blocked: [], reviewPath: null });
    expect(assemble.actions.find((item) => item.id === "open-review")?.enabled).toBe(false);

    const profile = buildCharactersModel({ characters: [], creating: false, busy: null, error: null });
    expect(profile.tone === "empty" || profile.emptyTitle).toBeTruthy();
  });

  it("disables the control a blocked action names", () => {
    const view = document.createElement("div");
    const button = document.createElement("button");
    button.textContent = "Draft from script";
    view.append(button);
    applySurfaceActions(view, [
      { id: "draft", label: "Draft from script", enabled: false, reason: "Pick a chat model in the dropdown first." },
    ]);
    expect(button.disabled).toBe(true);
    expect(button.title).toContain("chat model");
  });
});

describe("phase 7 recipe data", () => {
  const narrative = desk([
    { id: "script", label: "Script", workArea: "plan" },
    { id: "characters", label: "Characters", workArea: "plan", available: false, disabledReason: "Waiting on Script." },
    { id: "scene_intent", label: "Scene Intent", workArea: "plan" },
    { id: "render", label: "Render", workArea: "produce" },
    { id: "assemble", label: "Edit", workArea: "finish" },
  ]);

  it("follows host navigation over a local stage list, including a stage with no editor", () => {
    const color: StageId = EXAMPLE_RECIPE_STAGE;
    expect(stageHasEditor(color)).toBe(false);
    expect(stageHasEditor("script")).toBe(true);
    const fromRecipe = stagesFromRecipe({
      navigation: [
        { id: "script", label: "Track", work_area: "plan", status: "present", available: true, depends_on: [] },
        { id: color, label: "Color script", work_area: "produce", status: "missing", available: false, disabled_reason: "Waiting on Track.", depends_on: ["script"] },
        { id: "assemble", label: "Edit", work_area: "finish", status: "missing", available: true, depends_on: [color] },
      ],
      recipeStages: [
        { id: "characters", label: "Characters", work_area: "plan", required_roles: ["cast", "mouth_policy"] },
        { id: "assemble", label: "Assemble", work_area: "finish", required_roles: [] },
        { id: "script", label: "Script", work_area: "plan", required_roles: [] },
      ],
      scene: null,
    });
    expect(fromRecipe.map((item) => item.id)).toEqual(["script", color, "assemble"]);
    expect(fromRecipe.some((item) => item.id === "characters")).toBe(false);
    expect(fromRecipe.map((item) => item.workArea)).toEqual(["plan", "produce", "finish"]);
    expect(workAreaForStage(color, fromRecipe)).toBe("produce");
    const produce = surfacesForArea({ area: "produce", stages: fromRecipe, scene: null, active: "script" });
    expect(produce.map((item) => item.id)).toEqual([color]);
    expect(produce[0]?.available).toBe(false);
    expect(produce[0]?.disabledReason).toBe("Waiting on Track.");
    const fallback = paintUnknownRecipeStage({
      id: color,
      label: "Color script",
      disabledReason: "Waiting on Track.",
    });
    expect(fallback.dataset.stage).toBe(color);
    expect(fallback.querySelector("h2")?.textContent).toBe("Color script");
    expect(fallback.textContent).toContain("Waiting on Track.");
    expect(stageStatus({ ...stages(), extensions: { [color]: "present" } }, color)).toBe("present");
    const context = describeDirectorContext(
      { scenes: [scene({ id: 1, stable_id: "scene-1", title: "Dawn" })] } as ProjectState,
      {
        sceneId: 1,
        sceneStableId: "scene-1",
        cut: null,
        shotStableId: null,
        assetId: null,
        lookSlotId: null,
        requirementId: null,
        workArea: "produce",
        stage: color,
      },
      "Color script",
    );
    expect(context.headline).toContain("Color script");
  });

  it("omits characters and lip sync when the recipe does", () => {
    const montage = stagesFromRecipe({
      navigation: null,
      recipeStages: [
        { id: "script", label: "Track", work_area: "plan", required_roles: [] },
        { id: "scene_intent", label: "Timing", work_area: "plan", required_roles: [] },
      ],
      scene: null,
    });
    expect(montage.map((item) => item.id)).toEqual(["script", "scene_intent"]);
    expect(montage.some((item) => item.id === "characters")).toBe(false);
    expect(MONTAGE_RECIPE).not.toBe(NARRATIVE_RECIPE);
    const plan = surfacesForArea({ area: "plan", stages: montage, scene: scene({ id: 1 }), active: "script" });
    expect(plan.some((item) => item.id === "characters")).toBe(false);
  });

  it("keeps an unavailable stage in navigation and refuses the click", () => {
    const items = surfacesForArea({ area: "plan", stages: narrative, scene: scene({ id: 1 }), active: "script" });
    const clicks: string[] = [];
    const nav = renderSurfaceNav(items, (id) => clicks.push(id));
    const characters = [...nav.querySelectorAll("button")].find((button) => button.textContent?.includes("Characters"));
    expect(characters).toBeTruthy();
    expect(characters?.disabled).toBe(true);
    expect(characters?.title).toBe("Waiting on Script.");
    characters?.click();
    expect(clicks).toEqual([]);
    const script = [...nav.querySelectorAll("button")].find((button) => button.textContent?.startsWith("Script"));
    script?.click();
    expect(clicks).toEqual(["script"]);
  });
});

describe("ui fixture interactions", () => {
  it("opens the selected scene, follows Director context, and opens job and review outputs", () => {
    const state = {
      scenes: [scene({ id: 1, stable_id: "scene-1", title: "The last train", cuts: [cut({ cut: 1, stable_id: "shot-1" })] })],
      requirements: [],
      characters: [],
    } as unknown as ProjectState;
    const list = renderSceneListView(
      buildSceneList({
        scenes: state.scenes,
        reports: [],
        assets: [],
        selectedKey: "scene-1",
        blockerCounts: new Map(),
        resolvePath: () => null,
      }),
      () => undefined,
    );
    const selected = list.querySelector("button.active");
    expect(selected?.textContent).toContain("The last train");
    expect(selected?.getAttribute("data-scene-key")).toBe("scene-1");

    const context = describeDirectorContext(
      state,
      {
        sceneId: 1,
        sceneStableId: "scene-1",
        cut: 1,
        shotStableId: "shot-1",
        assetId: null,
        lookSlotId: null,
        requirementId: null,
        workArea: "finish",
        stage: "assemble",
      },
      "Edit",
    );
    expect(context.headline).toContain("Edit");
    expect(context.entries.some((entry) => entry.id === "shot-1")).toBe(true);

    const jobs = buildJobsModel({
      jobs: [
        {
          id: "job-assemble",
          kind: "assemble",
          status: "completed",
          output_path: "Final/Scene1_master.mp4",
          scope: { kind: "scene", id: "scene-1" },
        } as JobRecord,
      ],
      scenes: state.scenes,
      assets: [],
      filter: "all",
    });
    const opened: string[] = [];
    const jobsView = renderJobsView(jobs, true, "all", (intent) => {
      if (intent.type === "open") opened.push(intent.jobId);
    });
    const jobButton = jobsView.querySelector("button.studio-job");
    expect(jobButton?.title).toContain("Open review export");
    (jobButton as HTMLButtonElement).click();
    expect(opened).toEqual(["job-assemble"]);

    const finish = buildFinishModel({
      scene: scene({
        id: 1,
        cuts: [cut({ cut: 1, takes: [{ id: "take-1", path: "take.mp4", status: "selected" } as CutRecord["takes"][number]], selected_take: "take-1" })],
        post: { versions: [{ id: "v1", assembly_path: "Final/Scene1_master.mp4" }] } as SceneRecord["post"],
      }),
      jobs: [],
      deliverables: [{ id: "del-1", name: "Vertical" }],
      busyReason: null,
    });
    const finishIntents: string[] = [];
    const finishView = renderFinishView(finish, (intent) => finishIntents.push(intent.type));
    const review = [...finishView.querySelectorAll("button")].find((button) => button.textContent === "Open review export");
    expect(review?.disabled).toBe(false);
    review?.click();
    expect(finishIntents).toContain("open-review");
    expect(finishView.querySelector(".studio-finish-actions")?.querySelectorAll("button").length).toBeGreaterThan(0);
  });

  it("keeps actions available to the keyboard when the layout is narrow", () => {
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    expect(css).toContain("@media (max-width: 900px)");
    expect(css).toContain("@media (max-width: 720px)");
    expect(css).toContain(".studio-surface-nav button:focus-visible");
    expect(css).not.toMatch(/\.studio-finish-actions\s*\{[^}]*display:\s*none/);
    const finish = renderFinishView(
      buildFinishModel({
        scene: scene({
          id: 1,
          cuts: [cut({ cut: 1, takes: [{ id: "take-1", path: "take.mp4", status: "selected" } as CutRecord["takes"][number]], selected_take: "take-1" })],
        }),
        jobs: [],
        deliverables: [],
        busyReason: null,
      }),
      () => undefined,
    );
    for (const button of finish.querySelectorAll("button")) {
      expect(button.tabIndex).toBeGreaterThanOrEqual(0);
      expect(button.getAttribute("tabindex")).not.toBe("-1");
    }
  });

  it("does not let presentation modules call invoke", () => {
    const studio = join(root, "src/studio");
    const files = walk(studio).filter((file) => file.endsWith(".ts") && !file.endsWith("controller.ts"));
    for (const file of files) {
      const source = readFileSync(file, "utf8");
      expect(source, file).not.toMatch(/\binvoke\s*\(/);
    }
  });
});

describe("sceneHeadingTitle", () => {
  it("does not duplicate a stored 'Scene N —' prefix", () => {
    expect(sceneHeadingTitle(1, "Scene 1 — The last train")).toBe("Scene 1 — The last train");
    expect(sceneHeadingTitle(1, "Scene 1: The last train")).toBe("Scene 1 — The last train");
    expect(sceneHeadingTitle(1, "scene 1 - the last train")).toBe("Scene 1 — the last train");
  });
  it("prefixes bare titles and leaves other scenes' numbers alone", () => {
    expect(sceneHeadingTitle(1, "The last train")).toBe("Scene 1 — The last train");
    expect(sceneHeadingTitle(2, "Scene 1 — The last train")).toBe("Scene 2 — Scene 1 — The last train");
    expect(sceneHeadingTitle(3, "")).toBe("Scene 3");
    expect(sceneHeadingTitle(1, "Scene 1")).toBe("Scene 1");
  });
});

function walk(dir: string): string[] {
  const found: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) found.push(...walk(path));
    else found.push(path);
  }
  return found;
}
