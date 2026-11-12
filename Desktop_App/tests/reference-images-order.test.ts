import { expect, it } from "vitest";
import { surfacesForArea, type DeskStage } from "../src/studio/surfaces";

it("places Reference Images third without reordering the other surfaces", () => {
  const stages: DeskStage[] = ["script", "characters", "scene_intent", "scene_look", "cut_sketches", "reference_images", "production_notes"].map(id => ({
    id, label: id, workArea: "plan", available: true, disabledReason: null,
  }));
  const plan = surfacesForArea({ area: "plan", stages, scene: null, active: "script" });
  expect(plan.map(item => item.id)).toEqual(["script", "characters", "reference_images", "scene_intent", "scene_look", "cut_sketches", "production_notes", "profile", "assets"]);
});
