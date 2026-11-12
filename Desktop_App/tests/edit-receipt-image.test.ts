import { expect, it } from "vitest";
import { renderEditReceipt } from "../src/studio/components/editReceipt";
import type { CutRecord, ProjectState } from "../src/workflowTypes";

function receiptImage(cut: Partial<CutRecord>, assets: ProjectState["assets"] = []): string {
  const state = {
    version: 1, state_revision: 360, characters: [], scripts: [{ path: "Scripts/film.md" }],
    assets, scenes: [{ id: 1, title: "Scene", cuts: [{
      id: { source_scene: 1, cut: 1 }, package_needs_review: true, ...cut,
    }] }],
  } as unknown as ProjectState;
  return renderEditReceipt(state)!.querySelector("li")!.textContent!;
}

const retained = "Preproduction/Scene1/start_frames/cut_1_v1.png";
const crop = "Preproduction/Scene1/references/interior_2.png";

it("reports the rebound shared image and its identity rather than retained start-frame history", () => {
  const line = receiptImage({ approved_start_frame: retained,
    visual_source: { strategy: "shared_reference", asset_id: "asset_46" } }, [
    { id: "asset_46", path: crop, role: "scene_look", status: "approved" },
  ]);
  expect(line).toContain(`image shared reference asset_46 · ${crop}`);
  expect(line).not.toContain(retained);
});

it("identifies missing and stale active bindings without falling back to inactive history", () => {
  for (const [assets, expected] of [
    [[], "missing asset asset_46"],
    [[{ id: "asset_46", path: crop, role: "scene_look", status: "superseded" }], `stale asset_46 · ${crop}`],
  ] as const) {
    const line = receiptImage({ approved_start_frame: retained,
      visual_source: { strategy: "shared_reference", asset_id: "asset_46" } }, [...assets]);
    expect(line).toContain(expected);
    expect(line).not.toContain(retained);
  }
});

it("distinguishes continuity dependency awaiting a frame from its bound frame", () => {
  const base = { approved_start_frame: retained, visual_source: {
    strategy: "previous_selected_take_exit_frame" as const,
    predecessor: { source_scene: 1, cut: 2 },
  } };
  expect(receiptImage(base)).toContain("image predecessor Scene 1 Cut 2 · awaiting selected exit frame");
  expect(receiptImage(base)).not.toContain(retained);
  expect(receiptImage({ ...base, visual_source: { ...base.visual_source, asset_id: "frame_7" } }, [
    { id: "frame_7", path: "Preproduction/Scene1/continuity_frames/cut_1_v2.png", role: "continuity_frame", status: "approved" },
  ])).toContain("image predecessor Scene 1 Cut 2 · frame_7 · Preproduction/Scene1/continuity_frames/cut_1_v2.png");
});

it("keeps legacy start-frame resolution, and never labels no-start-frame as an image", () => {
  expect(receiptImage({ approved_start_frame: retained })).toContain(`image start frame ${retained}`);
  expect(receiptImage({ approved_start_frame: retained, visual_source: { strategy: "no_start_frame" } }))
    .toContain("image no start frame");
  expect(receiptImage({ approved_start_frame: retained, visual_source: { strategy: "deferred_manual" } }))
    .toContain("image manual source · not bound");
  expect(receiptImage({ approved_start_frame: retained,
    visual_source: { strategy: "preproduction_start_frame", asset_id: "frame_2" } }, [
    { id: "frame_2", path: "Preproduction/Scene1/start_frames/cut_1_v2.png", role: "start_frame", status: "approved" },
  ])).toContain("image start frame frame_2 · Preproduction/Scene1/start_frames/cut_1_v2.png");
});
