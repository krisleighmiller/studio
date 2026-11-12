import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.unstubAllGlobals(); });

it.each([true, false])("keeps navigation usable and preserves a draft across Characters and Script (empty: %s)", async empty => {
  const result = structuredClone(fixture);
  if (empty) { result.state.scenes = []; result.report.scenes = []; }
  const forbidden = vi.fn(() => { throw new Error("Unexpected side effect"); });
  const invoke = vi.fn(async (cmd: string) => { if (cmd !== "workflow_import") return forbidden(); return result; });
  vi.stubGlobal("window", { workbench: { invoke } });
  vi.stubGlobal("fetch", forbidden);
  const root = document.createElement("div"); document.body.append(root);
  desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
    onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
    onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  await desk.loadWorkflowDesk("/fixture/project"); invoke.mockClear();
  const main = () => root.querySelector<HTMLElement>(".workflow-main")!;
  const click = (label: string, scope: ParentNode = main()) => {
    const button = [...scope.querySelectorAll("button")].find(b => (b.querySelector("span:not([class])")?.textContent ?? b.textContent)?.trim() === label);
    expect(button, `${label} must be reachable`).toBeDefined(); expect(button!.disabled).toBe(false); button!.click();
  };
  // Labels alone are insufficient: these clicks must actually switch the content.
  click("Characters"); expect(main().textContent).toContain("New character");
  click("Script");
  click("Create script", root);
  for (const [name, value] of [["title", "Keep title"], ["notes", "A missing train."], ["script", "Scene 1 — Platform\nKeep my edit."]]) {
    const field = main().querySelector<HTMLInputElement>(`[name='${name}']`)!;
    field.value = value; field.dispatchEvent(new Event("input", { bubbles: true }));
  }
  click("Characters"); expect(main().querySelector(".workflow-script-draft")).toBeNull();
  expect(main().textContent).toContain("New character");
  click("Create script", root); // Sidebar resumes rather than erases the draft.
  expect(main().querySelector<HTMLInputElement>("[name='title']")!.value).toBe("Keep title");
  click("Characters");
  click("Script");
  expect(main().querySelector<HTMLInputElement>("[name='title']")!.value).toBe("Keep title");
  expect(main().querySelector<HTMLTextAreaElement>("[name='notes']")!.value).toBe("A missing train.");
  expect(main().querySelector<HTMLTextAreaElement>("[name='script']")!.value).toContain("Keep my edit.");
  if (empty) {
    click("Scene Intent"); expect(main().querySelector(".workflow-script-draft")).toBeNull();
    expect(main().querySelector(".empty-state")?.textContent).toBe("Create or import a script in Script first, then select a scene to use this surface.");
    click("Profile"); expect(main().querySelector(".workflow-script-draft")).toBeNull();
    click("Assets"); expect(main().querySelector(".workflow-script-draft")).toBeNull();
    click("Script");
  }
  click("Cancel"); expect(main().querySelector(".workflow-script-draft")).toBeNull();
  click("Characters"); expect(main().textContent).toContain("New character");
  expect(invoke).not.toHaveBeenCalled(); expect(forbidden).not.toHaveBeenCalled();
});
