import { expect, it } from "vitest";
import { buildSceneLook } from "../src/studio/sceneLookModel";
import { renderSceneLookView } from "../src/studio/sceneLookView";
import type { SceneRecord, AssetRecord, JobRecord } from "../src/workflowTypes";

it("lists only cuts bound to this look slot, retaining their source, note, and pending state", () => {
  const scene = {
    id: 1, title: "Room", cuts: [
      { id: { source_scene: 1, cut: 1 }, title: "Wide", visual_source: { strategy: "shared_reference", asset_id: "old" }, production_note_path: "note1.md" },
      { id: { source_scene: 1, cut: 2 }, visual_source: { strategy: "shared_reference", asset_id: "other" } },
    ], look_board: { slots: [{ id: "room", role: "interior_reference", title: "Room", asset_ids: ["old", "new"], approved_asset_id: "new" }] },
  } as SceneRecord;
  const assets = [
    { id: "old", path: "old.png", status: "superseded", role: "interior_reference" },
    { id: "new", path: "new.png", status: "approved", role: "interior_reference" },
  ] as AssetRecord[];
  const model = buildSceneLook({ scene, report: undefined, assets, jobs: [{ id: "render1", kind: "render", status: "pending", input_asset_ids: ["old"] }] as JobRecord[], characters: [], blockers: [], focusedSlotId: "room", focusedAssetId: null, directorSlotId: null, identityHint: null, busy: null, resolvePath: path => path ?? null });
  expect(model.affectedCuts).toMatchObject([{ label: "Scene 1 Cut 1 · Wide", assetId: "old", notePath: "note1.md", pending: true, stale: true }]);
  const view = renderSceneLookView(model, () => {});
  expect(view.querySelector(".scene-look-affected-cuts")?.textContent).toContain("Needs review/rebind");
  expect(view.querySelector(".scene-look-affected-cuts")?.textContent).toContain("pending video approval must be prepared again");
  expect(view.querySelector(".scene-look-affected-cuts")?.textContent).not.toContain("Cut 2");
});
