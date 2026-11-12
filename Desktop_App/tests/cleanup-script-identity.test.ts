import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";
import type { WorkflowImportResult } from "../src/workflowTypes";

// Captured by the real Rust importer from retained runtime-first/issue1 files/state.
// This is a frontend wiring check, not a substitute for native ownership tests.
const native = JSON.parse(readFileSync("tests/fixtures/fx02-import-result.json", "utf8")) as WorkflowImportResult;
const path = "Scripts/Final authored dialogue.md";
afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it.each(["save", "import", "register"])("FX02 %s selects the second source's allocated scene, not original Scene 1", async action => {
  const container = document.createElement("div"); document.body.append(container);
  const forbidden = vi.fn(() => { throw new Error("No provider/chat request permitted"); });
  const invoke = vi.fn(async (command: string) => {
    if (command === "workflow_import") return structuredClone(native);
    if (command === "save_project_script" || command === "ingest_script_file") return path;
    return forbidden();
  });
  vi.stubGlobal("window", { workbench: { invoke, openDialog: async () => "/external/Final authored dialogue.md" } });
  vi.stubGlobal("fetch", forbidden);
  desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
    onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
    onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  await desk.loadWorkflowDesk("/fixture/project");
  expect(desk.workflowSelection().sceneId).toBe(1);
  const click = (label: string) => {
    const button = [...container.querySelectorAll("button")].find(b => b.textContent === label);
    expect(button).toBeDefined(); button!.click();
  };
  if (action === "save") {
    click("Create script");
    for (const [name, value] of [["title", "Final authored dialogue"], ["script", readFileSync("host/tests/support/fx02/final.md", "utf8")]]) {
      const field = container.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
      field.value = value; field.dispatchEvent(new Event("input", { bubbles: true }));
    }
    click("Save script");
  } else if (action === "import") await desk.handleImportScript();
  else await desk.handleUseAsProjectScript("/fixture/project/Final authored dialogue.md");
  const expected = native.state.scenes.find(s => s.source_path === path)!;
  expect(expected.source_scene_number).toBe(1);
  expect(expected.id).not.toBe(1);
  await vi.waitFor(() => expect(desk.workflowSelection().sceneId).toBe(expected.id));
  expect(getSurfaceHost().state?.scenes.find(s => s.id === 1)?.source_path).toBe("Scripts/Reviewer saved.md");
  expect(container.querySelector(".workflow-script")?.textContent).toContain("Hello independently edited dialogue!");
  expect(container.textContent).not.toContain("No scenes were imported");
  expect(forbidden).not.toHaveBeenCalled();
});
