/**
 * Phase 4 close-out + bounded Redo visual task tests (2026-10-15).
 *
 * Three groups:
 *  - Review-export opening through the real app path (plan §8.1 done-when c):
 *    the Assemble surface must hand the PROJECT-RESOLVED path to the app's
 *    file-open hook, not just emit an intent.
 *  - Scene Look composition (UI Redo plan): approval pill, hero title and
 *    purpose, slot tabs, and alternate action bars must be visible WITHOUT
 *    opening any disclosure, and alternate tiles must carry working actions.
 *  - Director scope parity: the frontend context lines must be byte-identical
 *    to the host's scope.rs `describe` format, including the asset-role and
 *    look-slot display tables (mirrored verbatim from
 *    host/src/workflow/state.rs AssetRole::display_label).
 */
import { describe, expect, it, vi } from "vitest";
import { bindSurfaceHost, renderAssembleStage, resetSurfaceSession } from "../src/studio/surfaces/productionSurfaces";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import { renderSceneLookView } from "../src/studio/sceneLookView";
import type { SceneLookModel } from "../src/studio/sceneLookModel";
import { describeDirectorContext } from "../src/studio/selectors/directorContext";
import { bindWorkflowDesk, loadWorkflowDesk, toggleJobsDrawer, jobsDrawerOpen } from "../src/workflowDesk";
import type { CutRecord, ProjectState, SceneRecord, StageMap, WorkflowImportResult } from "../src/workflowTypes";

// The desk's only host seam is src/host; FA-4 replaces it so the full desk can
// load a canned project and the job-row click runs the real intent routing.
const hostMock = vi.hoisted(() => ({
  invoke: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
  ask: vi.fn(async () => false),
  openDialog: vi.fn(async () => null),
  toMediaUrlAsync: vi.fn(async (path: string) => path),
}));
vi.mock("../src/host", () => ({
  invoke: hostMock.invoke,
  ask: hostMock.ask,
  openDialog: hostMock.openDialog,
  toMediaUrlAsync: hostMock.toMediaUrlAsync,
}));

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

function assembleHost(overrides: Record<string, unknown> = {}): SurfaceHost {
  return {
    busy: null,
    error: null,
    chatReady: true,
    state: null,
    report: null,
    selectedCut: null,
    selectedSceneId: 1,
    selectedRequirementId: null,
    run: async () => true,
    blockerNames: () => [],
    resolvePath: (path: string | null) => path ?? null,
    mediaUrl: async (path: string) => path,
    continuityDependents: () => [],
    reviewExportPath: () => null,
    hooks: null,
    ...overrides,
  } as unknown as SurfaceHost;
}

const oneTakeScene = () =>
  scene({
    id: 1,
    cuts: [
      cut({
        cut: 1,
        selected_take: "take_1",
        takes: [{ id: "take_1", path: "Scenes/Scene1/Cut1/a.mp4", status: "selected", notes: null }],
      }),
    ],
  });

describe("phase 4 review-export open path", () => {
  it("delivers the project-resolved review export path to the app file-open hook", () => {
    resetSurfaceSession();
    const opened: string[] = [];
    bindSurfaceHost(
      assembleHost({
        resolvePath: (path: string | null) => (path ? `/projects/Lite/${path}` : null),
        reviewExportPath: () => "Final/Scene1_master.mp4",
        hooks: { onOpenPath: (path: string) => opened.push(path) },
      }),
    );
    const view = renderAssembleStage(oneTakeScene(), () => undefined);
    const open = [...view.querySelectorAll("button")].find((button) => button.textContent === "Open review export");
    expect(open).toBeTruthy();
    expect(open?.disabled).toBe(false);
    open?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(opened).toEqual(["/projects/Lite/Final/Scene1_master.mp4"]);
  });

  it("keeps the open button disabled when no review export exists yet", () => {
    resetSurfaceSession();
    const opened: string[] = [];
    bindSurfaceHost(
      assembleHost({
        reviewExportPath: () => null,
        hooks: { onOpenPath: (path: string) => opened.push(path) },
      }),
    );
    const view = renderAssembleStage(oneTakeScene(), () => undefined);
    const open = [...view.querySelectorAll("button")].find((button) => button.textContent === "Open review export");
    expect(open?.disabled).toBe(true);
    open?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(opened).toEqual([]);
    expect(open?.title).toContain("No review export yet");
  });
});

function lookModel(partial: Partial<SceneLookModel> = {}): SceneLookModel {
  return {
    title: "Scene 1 — The Last Train",
    counts: "2 slots · 1 approved",
    affectedCuts: [],
    warnings: [],
    blockers: [],
    narrativePurpose: "Kath leaves on the 11:40, unnoticed.",
    notes: [],
    slots: [
      { id: "s1", role: "interior_reference", title: "Train interior", prompt: "Warm", negativePrompt: "", approvalLabel: "Approved" },
      { id: "s2", role: "exterior_reference", title: "", prompt: "", negativePrompt: "", approvalLabel: "Needs approval" },
    ],
    selectedSlotId: "s1",
    versions: [
      { assetId: "a1", path: "Preproduction/Scene1/references/interior_1.png", statusLabel: "approved", approved: true, rejected: false },
      { assetId: "a2", path: "Preproduction/Scene1/references/interior_2.png", statusLabel: "pending", approved: false, rejected: false },
    ],
    selectedAssetId: "a1",
    primary: { assetId: "a1", path: "Preproduction/Scene1/references/interior_1.png", statusLabel: "approved", approved: true, rejected: false },
    identityHint: null,
    failedJobs: [],
    showPlates: false,
    plateNote: "",
    plates: [],
    canBuildPlates: false,
    emptySlots: false,
    busyReason: null,
    directorAttached: false,
    ...partial,
  } as unknown as SceneLookModel;
}

/** True when the element is inside a closed or open <details> (a disclosure). */
function insideDisclosure(el: Element | null): boolean {
  return Boolean(el?.closest("details"));
}

describe("Scene Look composition (UI Redo plan)", () => {
  it("shows the approval pill, artifact title, and purpose without opening any disclosure", () => {
    const view = renderSceneLookView(lookModel(), () => undefined);
    const pill = view.querySelector(".scene-look-pill");
    expect(pill).not.toBeNull();
    expect(pill?.textContent).toBe("✓ Approved");
    expect(insideDisclosure(pill)).toBe(false);

    const heroTitle = view.querySelector(".scene-look-hero-title");
    expect(heroTitle?.textContent).toBe("Train interior");
    expect(insideDisclosure(heroTitle)).toBe(false);

    const purpose = view.querySelector(".scene-look-hero-purpose");
    expect(purpose?.textContent).toBe("Kath leaves on the 11:40, unnoticed.");
    expect(insideDisclosure(purpose)).toBe(false);
  });

  it("keeps the look-slot tabs persistent, not collapsed", () => {
    const intents: Array<{ type: string; slotId?: string }> = [];
    const view = renderSceneLookView(lookModel(), (intent) => intents.push(intent as never));
    const tabs = [...view.querySelectorAll(".scene-look-slot")];
    expect(tabs.length).toBe(2);
    expect(tabs.some((tab) => tab.textContent === "Train interior")).toBe(true);
    // Unlabeled slot falls back to the role display name.
    expect(tabs.some((tab) => tab.textContent === "Exterior")).toBe(true);
    expect(insideDisclosure(tabs[0] ?? null)).toBe(false);

    const second = tabs[1];
    second?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toEqual([{ type: "select-slot", slotId: "s2" }]);
  });

  it("gives every alternate its own action bar (approve/open) on the tile", () => {
    const intents: Array<Record<string, unknown>> = [];
    const view = renderSceneLookView(lookModel(), (intent) => intents.push(intent as never));
    const tiles = view.querySelectorAll(".scene-look-alt-tile");
    expect(tiles.length).toBe(2);

    const [first, second] = [...tiles];
    const firstApprove = [...(first?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Approved");
    expect(firstApprove).toBeTruthy();
    expect(firstApprove?.disabled).toBe(true);

    const secondApprove = [...(second?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Approve");
    expect(secondApprove).toBeTruthy();
    secondApprove?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toContainEqual({ type: "approve", slotId: "s1", assetId: "a2" });

    const secondOpen = [...(second?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Open");
    secondOpen?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toContainEqual({ type: "open", path: "Preproduction/Scene1/references/interior_2.png" });

    // Selecting a tile still moves the primary preview.
    const picker = second?.querySelector("button.scene-look-alt");
    picker?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(intents).toContainEqual({ type: "select-version", assetId: "a2" });
  });

  it("disables alternate actions while a job is running, with the reason on the button", () => {
    const view = renderSceneLookView(lookModel({ busyReason: "Still generating another look." }), () => undefined);
    const approve = [...view.querySelectorAll(".scene-look-alt-tile button")].find((b) => b.textContent === "Approve");
    expect(approve?.disabled).toBe(true);
    expect(approve?.title).toBe("Still generating another look.");
  });

  it("keeps the pending pill visually distinct from the approved pill", () => {
    const view = renderSceneLookView(
      lookModel({
        selectedSlotId: "s2",
        slots: [
          { id: "s1", role: "interior_reference", title: "Train interior", prompt: "", negativePrompt: "", approvalLabel: "Approved" },
          { id: "s2", role: "exterior_reference", title: "", prompt: "", negativePrompt: "", approvalLabel: "Needs approval" },
        ],
        primary: { assetId: "a2", path: "p.png", statusLabel: "pending", approved: false, rejected: false },
        selectedAssetId: "a2",
      }),
      () => undefined,
    );
    const pill = view.querySelector(".scene-look-pill");
    expect(pill?.classList.contains("is-pending")).toBe(true);
    expect(pill?.textContent).toBe("Needs approval");
  });
});

/** Verbatim mirror of host/src/workflow/state.rs AssetRole::display_label. */
const HOST_DISPLAY_LABEL: Array<[string, string]> = [
  ["interior_reference", "Interior"],
  ["exterior_reference", "Exterior"],
  ["location_reference", "Location"],
  ["wardrobe_reference", "Wardrobe"],
  ["wardrobe_plate", "Wardrobe plate"],
  ["prop_reference", "Prop"],
  ["mood_reference", "Mood"],
  ["lighting_reference", "Lighting"],
  ["composition_reference", "Composition"],
  ["group_reference", "Group"],
  ["dialogue", "Dialogue"],
  ["voice_over", "Voice over"],
  ["music", "Music"],
  ["ambience", "Ambience"],
  ["room_tone", "Room tone"],
  ["sound_effect", "Sound effect"],
  ["foley", "Foley"],
  ["source_audio", "Source audio"],
  ["scratch_audio", "Scratch audio"],
  ["mix", "Mix"],
];

function fullState(): ProjectState {
  const trainScene = {
    id: 1,
    stable_id: "scene_1",
    title: "Last Train",
    cuts: [
      {
        id: { scene: 1, cut: 2 },
        stable_id: "shot_2",
        title: "Platform",
        takes: [],
        selected_take: null,
      },
    ],
    look_board: {
      slots: [
        { id: "slot_1", role: "exterior_reference", title: "Platform night", prompt: "", negative_prompt: "" },
        { id: "slot_2", role: "foley", title: "", prompt: "", negative_prompt: "" },
        { id: "slot_3", role: "wardrobe_reference", title: "", prompt: "", negative_prompt: "" },
        { id: "slot_4", role: "interior_reference", title: "Train interior", prompt: "", negative_prompt: "" },
      ],
    },
  };
  return {
    project_name: "Lite",
    scenes: [trainScene],
    assets: [
      { id: "asset_1", role: "interior_reference", status: "present", path: "a.png" },
      { id: "asset_2", role: "voice_sample", status: "approved", path: null },
    ],
    requirements: [
      { id: "req_1", name: "Lantern", kind: "prop", status: "present", scene_ids: [1] },
      { id: "req_2", name: "Guard", kind: "cast", status: "missing", scene_ids: [1] },
    ],
    jobs: [],
  } as unknown as ProjectState;
}

const ALL_SELECTED = {
  sceneId: 1,
  sceneStableId: "scene_1",
  cut: 2,
  shotStableId: "shot_2",
  assetId: "asset_1",
  lookSlotId: "slot_1",
  requirementId: "req_1",
  workArea: "produce",
  stage: "scene_look",
} as const;

describe("director scope parity with the host", () => {
  it("rebuilds the exact host scope.rs lines for the all-six selection", () => {
    const context = describeDirectorContext(fullState(), ALL_SELECTED as never, null);
    expect(context.scopeText).toBe(
      [
        "Workflow scope:",
        "- work area: produce",
        "- scene: scene_1 — Scene 1 Last Train",
        "- shot: shot_2 — Cut 2 Platform",
        "- asset: asset_1 — interior_reference",
        "- look slot: slot_1 — Platform night",
        "- requirement: req_1 — Lantern (present)",
      ].join("\n"),
    );
  });

  it("keeps unresolved ids saying 'unresolved', same as the host", () => {
    const context = describeDirectorContext(
      fullState(),
      { ...ALL_SELECTED, sceneStableId: "scene_9", assetId: "asset_9", lookSlotId: "slot_9", requirementId: "req_9", shotStableId: "shot_9" } as never,
      null,
    );
    expect(context.scopeText).toContain("- scene: scene_9 — unresolved");
    expect(context.scopeText).toContain("- shot: shot_9 — unresolved");
    expect(context.scopeText).toContain("- asset: asset_9 — unresolved");
    expect(context.scopeText).toContain("- look slot: slot_9 — unresolved");
    expect(context.scopeText).toContain("- requirement: req_9 — unresolved");
  });

  it("falls back to the display label for untitled look slots and blank titles for scenes", () => {
    const context = describeDirectorContext(fullState(), { ...ALL_SELECTED, lookSlotId: "slot_2" } as never, null);
    expect(context.scopeText).toContain("- look slot: slot_2 — Foley");

    const untitled = describeDirectorContext(
      { ...fullState(), scenes: [{ ...fullState().scenes[0], title: "", stable_id: "scene_1" }] } as never,
      ALL_SELECTED as never,
      null,
    );
    expect(untitled.scopeText).toContain("- scene: scene_1 — Scene 1");
  });

  it("maps every host display_label role to the same frontend name", () => {
    const state = fullState();
    for (const [role, display] of HOST_DISPLAY_LABEL) {
      const slotState = {
        ...state,
        scenes: state.scenes.map((item) => ({
          ...item,
          look_board: { slots: [{ id: "slot_x", role, title: "", prompt: "", negative_prompt: "" }] },
        })),
      } as unknown as ProjectState;
      const context = describeDirectorContext(slotState, { ...ALL_SELECTED, lookSlotId: "slot_x" } as never, null);
      const entry = context.entries.find((item) => item.kind === "lookSlot");
      expect(entry?.label, `role ${role}`).toBe(`look slot: slot_x — ${display}`);
    }
  });
});

describe("jobs output link (FA-4)", () => {
  it("routes a completed assemble job to the file-open hook with the project-resolved path", async () => {
    hostMock.invoke.mockImplementation(async (command: unknown) => {
      if (command !== "workflow_import") return {};
      return {
        state: {
          version: 1,
          project_name: "Lite",
          characters: [],
          scenes: [],
          assets: [],
          requirements: [],
          notes: [],
          budget: null,
          jobs: [
            {
              id: "job-9",
              kind: "assemble",
              status: "completed",
              output_path: "Final/Scene1_master.mp4",
              created_at: "2026-10-15T10:00:00Z",
              started_at: "2026-10-15T10:00:20Z",
              completed_at: "2026-10-15T10:01:40Z",
              progress: "Review export ready",
              scope: null,
              cost_minor: null,
              estimated_cost_minor: null,
            },
          ],
        } as unknown as ProjectState,
        report: { summary: "1 scene.", scenes: [] },
        summary: "1 scene.",
      } as unknown as WorkflowImportResult;
    });
    const container = document.createElement("div");
    document.body.append(container);
    const opened: string[] = [];
    bindWorkflowDesk({
      root: container,
      projectRoot: () => "/projects/Lite",
      onOpenPath: (path) => opened.push(path),
      onAddToChat: () => undefined,
      onOpenVoice: () => undefined,
      onComposeChat: () => undefined,
      onSelectionChange: () => undefined,
      chatModel: () => null,
      onStatus: () => undefined,
    });
    await loadWorkflowDesk("/projects/Lite");
    // Drawer state is module-level; make opening this test order-independent.
    if (!jobsDrawerOpen()) toggleJobsDrawer();

    const row = Array.from(container.querySelectorAll<HTMLElement>(".studio-job-row")).find((item) =>
      item.textContent?.includes("Review export ready"),
    );
    expect(row).toBeTruthy();
    // Truthful lifecycle metadata reaches the row: real start, real progress,
    // and an elapsed time computed from actual timestamps — not "unknown".
    expect(row?.textContent).toContain("Progress Review export ready");
    expect(row?.textContent).not.toContain("time unknown");
    row?.querySelector<HTMLButtonElement>("button.studio-job")?.click();
    expect(opened).toEqual(["/projects/Lite/Final/Scene1_master.mp4"]);
    container.remove();
    hostMock.invoke.mockReset();
  });

  it("a completed job without an elapsed window says time unknown, not a fake duration", async () => {
    hostMock.invoke.mockImplementation(async (command: unknown) => {
      if (command !== "workflow_import") return {};
      return {
        state: {
          version: 1,
          project_name: "Lite",
          characters: [],
          scenes: [],
          assets: [],
          requirements: [],
          notes: [],
          budget: null,
          jobs: [
            {
              id: "job-10",
              kind: "assemble",
              status: "completed",
              output_path: "Final/Scene2_master.mp4",
              created_at: null,
              started_at: null,
              completed_at: null,
              progress: "Review export ready",
              scope: null,
              cost_minor: null,
              estimated_cost_minor: null,
            },
          ],
        } as unknown as ProjectState,
        report: { summary: "1 scene.", scenes: [] },
        summary: "1 scene.",
      } as unknown as WorkflowImportResult;
    });
    const container = document.createElement("div");
    document.body.append(container);
    const opened: string[] = [];
    bindWorkflowDesk({
      root: container,
      projectRoot: () => "/projects/Lite",
      onOpenPath: (path) => opened.push(path),
      onAddToChat: () => undefined,
      onOpenVoice: () => undefined,
      onComposeChat: () => undefined,
      onSelectionChange: () => undefined,
      chatModel: () => null,
      onStatus: () => undefined,
    });
    await loadWorkflowDesk("/projects/Lite");
    // Drawer state is module-level; make opening this test order-independent.
    if (!jobsDrawerOpen()) toggleJobsDrawer();

    const row = Array.from(container.querySelectorAll<HTMLElement>(".studio-job-row")).find((item) =>
      item.textContent?.includes("Review export ready"),
    );
    expect(row).toBeTruthy();
    expect(row?.textContent).toContain("time unknown");
    // The truthful metadata does not cost the open affordance.
    row?.querySelector<HTMLButtonElement>("button.studio-job")?.click();
    expect(opened).toEqual(["/projects/Lite/Final/Scene2_master.mp4"]);
    container.remove();
    hostMock.invoke.mockReset();
  });
});
