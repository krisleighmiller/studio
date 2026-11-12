import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
afterEach(() => { desk.clearWorkflowDesk(); localStorage.clear(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it("saves/reopens/reloads exact notes before script, retaining cancellation and failed-write drafts without model requests", async () => {
  localStorage.clear();
  const root = document.createElement("div"); document.body.append(root);
  const empty = structuredClone(fixture); empty.state.scenes = []; empty.report.scenes = [];
  const files = new Map<string, string>();
  let failWrite = false;
  const invoke = vi.fn(async (command: string, args?: any) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      if (failWrite) throw new Error("Fixture write refused");
      files.set(`${args.projectRoot}/Notes.md`, args.text);
      return "Notes.md";
    }
    if (command === "read_text_file") {
      if (!files.has(args.path)) throw new Error("File missing");
      return files.get(args.path);
    }
    throw new Error(`Forbidden: ${command}`);
  });
  const forbidden = vi.fn(() => { throw new Error("No network/chat permitted"); });
  const openDialog = vi.fn(async () => null);
  vi.stubGlobal("fetch", forbidden);
  vi.stubGlobal("window", { workbench: { invoke, openDialog } });
  let project = "/disposable/sb08/A";
  desk.bindWorkflowDesk({ root, projectRoot: () => project, chatModel: () => null,
    onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
    onSelectionChange: vi.fn(), onStatus: vi.fn() });
  const open = async (path = project) => { desk.clearWorkflowDesk(); project = path; await desk.loadWorkflowDesk(project); };
  const click = (label: string) => { const b = [...root.querySelectorAll("button")].find(b => b.textContent === label); expect(b, label).toBeDefined(); b!.click(); };
  const field = (name: string) => root.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
  const edit = (name: string, value: string) => { field(name).value = value; field(name).dispatchEvent(new Event("input")); };
  const settled = () => vi.waitFor(() => expect(root.querySelector("form")?.getAttribute("aria-busy")).toBe("false"));
  const key = "studio.local-script-draft.v1:" + project;
  const notes = "  Notes before any script — 雨\n\nKeep whitespace. ☔\n";
  await open(); click("Create script"); edit("title", "Untitled film"); edit("notes", notes);
  click("Save notes"); await settled();
  expect(root.textContent).toContain("Notes saved to Notes.md");
  expect(root.textContent).toContain("/disposable/sb08/A");
  expect(files.get(`${project}/Notes.md`)).toBe(notes);
  expect(JSON.parse(localStorage.getItem(key)!)).toMatchObject({ notes, script: "", title: "Untitled film" });
  click("Cancel"); click("Create script"); expect(field("notes").value).toBe(notes);
  click("Load notes"); await settled(); expect(field("notes").value).toBe(notes);
  await open(); expect(field("notes").value).toBe(notes);
  // Read the project file, not the distinct recovery copy; reload Notes alone.
  files.set(`${project}/Notes.md`, "Saved elsewhere locally\n");
  click("Reload saved notes"); await settled();
  expect(field("notes").value).toBe("Saved elsewhere locally\n");
  expect(field("title").value).toBe("Untitled film"); expect(field("script").value).toBe("");
  expect(root.textContent).toContain("Notes reloaded");
  const prior = files.get(`${project}/Notes.md`);
  failWrite = true;
  edit("notes", notes + "Latest unsaved\n"); click("Save notes"); await settled();
  expect(root.querySelector('[role="alert"]')?.textContent).toContain("Notes.md was not updated");
  expect(files.get(`${project}/Notes.md`)).toBe(prior);
  click("Cancel"); await open(); expect(field("notes").value).toBe(notes + "Latest unsaved\n");
  failWrite = false; click("Save notes"); await settled();
  expect(files.get(`${project}/Notes.md`)).toBe(notes + "Latest unsaved\n");
  await open("/disposable/sb08/B"); click("Create script"); expect(field("notes").value).toBe("");
  await open("/disposable/sb08/A"); expect(field("notes").value).toBe(notes + "Latest unsaved\n");
  expect(invoke.mock.calls.every(([command]) => ["workflow_import", "save_project_notes", "read_text_file"].includes(command))).toBe(true);
  expect(forbidden).not.toHaveBeenCalled();
});
