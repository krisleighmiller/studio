import { describe, expect, it } from "vitest";
import { assetRoleLabel, buildAssetsModel } from "../src/studio/selectors/assets";
import type { AssetRecord, ProjectState } from "../src/workflowTypes";

function asset(id: string, role: string, sceneId: number, status = "approved"): AssetRecord {
  return { id, path: `media/${id}.mp3`, role, scene_id: sceneId, status } as AssetRecord;
}

function state(assets: AssetRecord[]): ProjectState {
  return {
    version: 3,
    project_name: "x",
    characters: [],
    scenes: [],
    assets,
    jobs: [],
    requirements: [],
    notes: [],
  };
}

const LIBRARY = [
  asset("m", "music", 1),
  asset("d", "dialogue", 1, "draft"),
  asset("vo", "voice_over", 2),
  asset("amb", "ambience", 1),
  asset("fx", "sound_effect", 3),
  asset("loc", "location_reference", 1),
];

describe("assetRoleLabel", () => {
  it("labels audio and look roles", () => {
    expect(assetRoleLabel("music")).toBe("Music");
    expect(assetRoleLabel("voice_over")).toBe("Voice over");
    expect(assetRoleLabel("location_reference")).toBe("Location");
  });

  it("falls back to the raw role for unknown values", () => {
    expect(assetRoleLabel("mystery_role")).toBe("mystery_role");
  });
});

describe("buildAssetsModel", () => {
  it("returns everything under 'all'", () => {
    const model = buildAssetsModel(state(LIBRARY), "all");
    expect(model.rows).toHaveLength(6);
    expect(model.counts.all).toBe(6);
  });

  it("filters to audio roles only", () => {
    const model = buildAssetsModel(state(LIBRARY), "audio");
    // music, dialogue, voice_over, ambience, sound_effect — location_reference excluded
    expect(model.rows.map((row) => row.role).sort()).toEqual([
      "ambience",
      "dialogue",
      "music",
      "sound_effect",
      "voice_over",
    ]);
    expect(model.counts.audio).toBe(5);
  });

  it("filters a display section to its underlying roles", () => {
    const model = buildAssetsModel(state(LIBRARY), "dialogue");
    expect(model.rows.map((row) => row.role).sort()).toEqual(["dialogue", "voice_over"]);
    const effects = buildAssetsModel(state(LIBRARY), "effects");
    expect(effects.rows.map((row) => row.role)).toEqual(["sound_effect"]);
  });

  it("labels rows and sorts by role then scene", () => {
    const model = buildAssetsModel(state(LIBRARY), "all");
    const roles = model.rows.map((row) => row.roleLabel);
    expect(roles).toEqual([...roles].sort());
    expect(model.rows.find((row) => row.id === "d")!.status).toBe("draft");
    expect(model.rows.find((row) => row.id === "m")!.sceneId).toBe(1);
  });
});