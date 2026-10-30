import { describe, expect, it } from "vitest";
import { bindSurfaceHost, renderTakeReviewStage } from "../src/studio/surfaces/productionSurfaces";
import type { SurfaceHost } from "../src/studio/surfaces/host";

function makeHost(overrides: Partial<SurfaceHost> = {}): SurfaceHost {
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
    hooks: { onOpenPath: () => {} },
    ...overrides,
  } as unknown as SurfaceHost;
}

describe("S06 take review delete repair", () => {
  it("renders Delete button per take emitting delete-take with cut, takeId, path", () => {
    const intents: Array<{ type: string; cut?: number; takeId?: string; path?: string }> = [];
    bindSurfaceHost(makeHost());
    const view = renderTakeReviewStage(
      {
        id: 1,
        title: "Scene 1",
        stable_id: "s1",
        stages: {"script":"missing","characters":"missing","scene_intent":"missing","scene_look":"missing","cut_sketches":"missing","reference_images":"missing","production_notes":"missing","render":"missing","take_review":"ready","assemble":"missing"} as any,
        cuts: [
          {
            id: { scene: 1, cut: 1 },
            stable_id: "c1",
            title: "Cut 1",
            takes: [{ id: "t1", path: "Scenes/Scene1/Cut1/take.mp4", status: "selected", notes: null }],
            selected_take: null,
          } as any,
        ],
        dialogue: [],
      } as any,
      (intent) => intents.push(intent as any),
    );
    const deleteBtn = [...view.querySelectorAll("button")].find((b) => b.textContent === "Delete");
    expect(deleteBtn).toBeTruthy();
    deleteBtn!.click();
    const emitted = intents.find((i) => i.type === "delete-take");
    expect(emitted).toBeTruthy();
    expect(emitted!.cut).toBe(1);
    expect(emitted!.takeId).toBe("t1");
    expect(emitted!.path).toBe("Scenes/Scene1/Cut1/take.mp4");
  });

  it("disables Delete when busy", () => {
    bindSurfaceHost(makeHost({ busy: "Deleting video take…" }));
    const view = renderTakeReviewStage(
      {
        id: 1,
        title: "S",
        stable_id: "s1",
        stages: {"script":"missing","characters":"missing","scene_intent":"missing","scene_look":"missing","cut_sketches":"missing","reference_images":"missing","production_notes":"missing","render":"missing","take_review":"ready","assemble":"missing"} as any,
        cuts: [
          {
            id: { scene: 1, cut: 1 },
            stable_id: "c1",
            title: "C",
            takes: [{ id: "t2", path: "a.mp4", status: "unreviewed", notes: null }],
            selected_take: null,
          } as any,
        ],
        dialogue: [],
      } as any,
      () => {},
    );
    const deleteBtn = [...view.querySelectorAll("button")].find((b) => b.textContent === "Delete");
    expect(deleteBtn).toBeTruthy();
    expect(deleteBtn!.disabled).toBe(true);
  });

  it("shows empty state with no takes", () => {
    bindSurfaceHost(makeHost());
    const view = renderTakeReviewStage(
      {
        id: 2,
        title: "Empty",
        stable_id: "s2",
        stages: {"script":"missing","characters":"missing","scene_intent":"missing","scene_look":"missing","cut_sketches":"missing","reference_images":"missing","production_notes":"missing","render":"missing","take_review":"ready","assemble":"missing"} as any,
        cuts: [{ id: { scene: 2, cut: 1 }, stable_id: "c1", title: "C", takes: [], selected_take: null } as any],
        dialogue: [],
      } as any,
      () => {},
    );
    expect(view.textContent).toContain("No takes to review");
  });

  it("keeps refresh and alternate distinct and status controls unchanged", () => {
    bindSurfaceHost(makeHost());
    const intents: Array<any> = [];
    const view = renderTakeReviewStage(
      {
        id: 1,
        title: "S",
        stable_id: "s1",
        stages: {"script":"missing","characters":"missing","scene_intent":"missing","scene_look":"missing","cut_sketches":"missing","reference_images":"missing","production_notes":"missing","render":"missing","take_review":"ready","assemble":"missing"} as any,
        cuts: [
          {
            id: { scene: 1, cut: 1 },
            stable_id: "c1",
            title: "C",
            takes: [{ id: "t1", path: "a.mp4", status: "selected", notes: null }],
            selected_take: null,
          } as any,
        ],
        dialogue: [],
      } as any,
      (i) => intents.push(i),
    );
    const refresh = [...view.querySelectorAll("button")].find((b) => b.textContent === "Refresh takes from project");
    const alternate = [...view.querySelectorAll("button")].find((b) => b.textContent === "New alternate angle");
    expect(refresh).toBeTruthy();
    expect(alternate).toBeTruthy();
    refresh!.click();
    expect(intents.some((i) => i.type === "refresh")).toBe(true);
    alternate!.click();
    expect(intents.some((i) => i.type === "alternate")).toBe(true);
    const statusBtns = [...view.querySelectorAll("button")].filter((b) => ["Select", "Mark alternate", "Reject", "Needs edit", "Needs rerender"].includes(b.textContent ?? ""));
    expect(statusBtns.length).toBe(5);
  });

  it("delete button does not appear for alternate status differently", () => {
    bindSurfaceHost(makeHost());
    const view = renderTakeReviewStage(
      {
        id: 1,
        title: "S",
        stable_id: "s1",
        stages: {"script":"missing","characters":"missing","scene_intent":"missing","scene_look":"missing","cut_sketches":"missing","reference_images":"missing","production_notes":"missing","render":"missing","take_review":"ready","assemble":"missing"} as any,
        cuts: [
          {
            id: { scene: 1, cut: 1 },
            stable_id: "c1",
            title: "C",
            takes: [{ id: "t3", path: "b.mp4", status: "alternate", notes: null }],
            selected_take: null,
          } as any,
        ],
        dialogue: [],
      } as any,
      () => {},
    );
    const deleteBtn = [...view.querySelectorAll("button")].find((b) => b.textContent === "Delete");
    expect(deleteBtn).toBeTruthy();
  });
});
