import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { renderScriptStage } from "../src/studio/surfaces/scriptStage";
import {
  approvedSceneLookAssets,
  cutForShot,
  cutImageAssets,
  dependencyNote,
  editableSketch,
  effectiveVisualSourceStrategy,
  formatSketchDialogue,
  nullable,
  parseSketchDialogue,
  renderBlocker,
  splitList,
  visualSourceHint,
  visualSourceReady,
  visualSourceStatus,
} from "../src/studio/surfaces/surfaceSelectors";
import { buildScriptModel } from "../src/studio/surfaces/surfaceModels";
import type { AssetRecord, CutRecord, SceneRecord } from "../src/workflowTypes";

const root = join(import.meta.dirname, "..");
const surfacesDir = join(root, "src", "studio", "surfaces");

function cut(partial: Record<string, unknown> = {}): CutRecord {
  return {
    id: { source_scene: 1, cut: 1 },
    title: "Cut 1",
    ...partial,
  } as unknown as CutRecord;
}

function asset(partial: Record<string, unknown> = {}): AssetRecord {
  return {
    id: "a1",
    role: "cut_start_frame",
    scene_id: 1,
    cut_id: { source_scene: 1, cut: 1 },
    status: "approved",
    path: "Preproduction/Scene1/cut_1_v1.png",
    ...partial,
  } as unknown as AssetRecord;
}

function scene(partial: Record<string, unknown> = {}): SceneRecord {
  return {
    id: 1,
    title: "Scene 1",
    cuts: [],
    ...partial,
  } as unknown as SceneRecord;
}

describe("surface selectors stay pure", () => {
  it("derives the effective visual-source strategy without host state", () => {
    expect(effectiveVisualSourceStrategy(cut({ visual_source: { strategy: "no_start_frame" } }))).toBe(
      "no_start_frame",
    );
    expect(effectiveVisualSourceStrategy(cut({ approved_start_frame: "a.png" }))).toBe(
      "preproduction_start_frame",
    );
    expect(effectiveVisualSourceStrategy(cut())).toBeNull();
  });

  it("reports render readiness per strategy", () => {
    expect(visualSourceReady(cut({ visual_source: { strategy: "no_start_frame" } }))).toBe(true);
    expect(visualSourceReady(cut({ approved_start_frame: "a.png" }))).toBe(true);
    expect(
      visualSourceReady(cut({ visual_source: { strategy: "preproduction_start_frame" } })),
    ).toBe(false);
    expect(
      visualSourceReady(cut({ visual_source: { strategy: "shared_reference", asset_id: "a1" } })),
    ).toBe(true);
    expect(visualSourceReady(cut({ visual_source: { strategy: "shared_reference" } }))).toBe(false);
  });

  it("orders render blockers before readiness", () => {
    expect(renderBlocker(cut())).toBe("production note missing");
    expect(renderBlocker(cut({ production_note_path: "n.md" }))).toBe(
      "production video prompt missing",
    );
    expect(visualSourceStatus(cut())).toBe("Source strategy needed");
  });

  it("hints the bound source without touching the DOM", () => {
    expect(visualSourceHint(cut(), undefined)).toContain("Choose and save a strategy");
    expect(
      visualSourceHint(cut({ visual_source: { strategy: "shared_reference" } }), asset()),
    ).toBe("Bound source: Preproduction/Scene1/cut_1_v1.png");
  });

  it("filters look and cut assets from explicit inputs, not global state", () => {
    const approved = asset({ id: "look-1" });
    const rejected = asset({ id: "look-2", status: "rejected" });
    const otherScene = asset({ id: "look-1", scene_id: 2 });
    const lookScene = scene({
      look_board: { slots: [{ approved_asset_id: "look-1" }, { approved_asset_id: "look-2" }] },
    });
    expect(approvedSceneLookAssets(lookScene, [approved, rejected, otherScene]).map((a) => a.id)).toEqual([
      "look-1",
    ]);
    const framed = asset({ id: "f1" });
    const wrongCut = asset({ id: "f2", cut_id: { source_scene: 1, cut: 2 } });
    const wrongRole = asset({ id: "f3", role: "voice_sample" });
    expect(cutImageAssets([framed, wrongCut, wrongRole], 1, 1).map((a) => a.id)).toEqual(["f1"]);
  });

  it("matches animatic shots to cuts and labels dependency states", () => {
    const sceneRecord = scene({ cuts: [cut()] });
    expect(
      cutForShot(sceneRecord, { cut_id: { source_scene: 1, cut: 1 } } as never)?.title,
    ).toBe("Cut 1");
    expect(cutForShot(sceneRecord, { cut_id: { source_scene: 1, cut: 9 } } as never)).toBeUndefined();
    expect(dependencyNote("stale")).toContain("stale");
    expect(dependencyNote("keep")).toContain("safe to keep");
    expect(dependencyNote("ok" as never)).toBeNull();
  });

  it("derives editable sketches and round-trips dialogue text", () => {
    const existing = { title: "T" };
    expect(editableSketch(cut({ sketch: existing }))).toBe(existing);
    expect(editableSketch(cut({ speech_mode: "source_video_lipsync" })).shot_role).toBe("performance");
    expect(editableSketch(cut({ speech_mode: "voice_over" })).mouth_policy).toBe("off_camera");
    const dialogue = [
      { speaker: "Pip", heard_as: "lip_sync", text: "Run." },
      { speaker: "", heard_as: null, text: "A bell." },
    ];
    const roundTripped = parseSketchDialogue(
      formatSketchDialogue({ dialogue } as never),
    );
    expect(roundTripped).toEqual(dialogue);
    expect(nullable("  ")).toBeNull();
    expect(nullable(" hi ")).toBe("hi");
    expect(splitList("a, b\nc")).toEqual(["a", "b", "c"]);
  });
});

describe("script surface follows the model pattern", () => {
  it("marks beats present and renders rows", () => {
    const record = scene({ location: "Station", dialogue: [{ speaker: "Pip", text: "Run." }] });
    expect(buildScriptModel({ scene: record }).status).toBe("present");
    const view = renderScriptStage(record);
    expect(view.dataset.status).toBe("present");
    expect(view.textContent).toContain("Location:");
    expect(view.textContent).toContain("Pip:");
  });

  it("falls back to the source excerpt, then to an empty state", () => {
    const excerpt = renderScriptStage(scene({ source_excerpt: "Rain." }));
    expect(excerpt.querySelector("pre.workflow-excerpt")?.textContent).toBe("Rain.");
    const empty = renderScriptStage(scene());
    expect(buildScriptModel({ scene: scene() }).status).toBe("empty");
    expect(empty.querySelector(".empty-state")?.textContent).toContain("No script beats");
  });
});

describe("phase 6 module boundary", () => {
  const stageFiles = readdirSync(surfacesDir).filter(
    (name) => name.endsWith("Stage.ts") && name !== "productionSurfaces.ts",
  );

  it("keeps every stage module focused", () => {
    expect(stageFiles.length).toBeGreaterThanOrEqual(9);
    for (const name of stageFiles) {
      const lines = readFileSync(join(surfacesDir, name), "utf8").split("\n").length;
      expect(lines, name).toBeLessThan(600);
    }
    const facade = readFileSync(join(surfacesDir, "productionSurfaces.ts"), "utf8");
    expect(facade.split("\n").length).toBeLessThan(60);
  });

  it("keeps selectors pure: no DOM, no session, no emits", () => {
    const selectors = readFileSync(join(surfacesDir, "surfaceSelectors.ts"), "utf8");
    expect(selectors).not.toContain("document.");
    expect(selectors).not.toContain("getSurfaceHost");
    expect(selectors).not.toContain("emit(");
  });

  it("keeps stage components off raw host commands", () => {
    for (const name of [...stageFiles, "shared.ts", "session.ts", "surfaceSelectors.ts"]) {
      const text = readFileSync(join(surfacesDir, name), "utf8");
      for (const line of text.split("\n")) {
        if (line.includes('from "../../host"')) {
          expect(line, `${name}: ${line.trim()}`).toContain("import type");
        }
      }
    }
  });
});
