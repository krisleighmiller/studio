import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";
// This suite owns post-submit script transport; SB21 exercises the real modal.
vi.mock('../src/studio/generationPrompt', async () => ({...await vi.importActual('../src/studio/generationPrompt'), editGenerationPrompt: async (spec: {prompt: string}) => spec.prompt}));

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.unstubAllGlobals(); });
async function setup(model: {apiId: string; modelId: string} | null = { apiId: "local", modelId: "writer" }) {
  // A fresh fixture represents a new project, not the previous test's recovery copy.
  localStorage.clear();
  const root = document.createElement("div"); document.body.append(root);
  const initial = structuredClone(fixture); initial.state.scenes = []; initial.report.scenes = [];
  const invoke = vi.fn(async (cmd: string, _args?: unknown): Promise<unknown> => {
    if (cmd === "workflow_import") return initial;
    if (cmd === "workflow_generate_script") return { script: "Scene 1 — Rain\nMara: Come inside." };
    throw new Error(`Unexpected command ${cmd}`);
  });
  const forbidden = vi.fn(() => { throw new Error("Network forbidden"); });
  vi.stubGlobal("fetch", forbidden);
  const openDialog = vi.fn(async (): Promise<string | null> => null);
  vi.stubGlobal("window", { workbench: { invoke, openDialog, ask: vi.fn(async () => true) } });
  let projectRoot = "/fixture/project";
  desk.bindWorkflowDesk({ root, projectRoot: () => projectRoot, chatModel: () => model,
    onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
    onSelectionChange: vi.fn(), onStatus: vi.fn() });
  await desk.loadWorkflowDesk(projectRoot); invoke.mockClear();
  const click = (label: string) => { const b = [...root.querySelectorAll("button")].find(b => b.textContent === label); expect(b, label).toBeDefined(); b!.click(); };
  click("Create script");
  const field = (name: string) => root.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
  const edit = (name: string, value: string) => { field(name).value = value; field(name).dispatchEvent(new Event("input")); };
  return { root, invoke, openDialog, click, field, edit, switchProject: () => { projectRoot = "/other/project"; desk.clearWorkflowDesk(); } };
}
it("native denial retains the previous error and all three fields", async () => {
  const ui = await setup();
  ui.edit("title", "Retain title"); ui.edit("notes", "Retain notes");
  ui.click("Save script");
  await vi.waitFor(() => expect(ui.root.textContent).toContain("Add script text before saving."));
  ui.invoke.mockResolvedValueOnce({cancelled:true});
  ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.root.querySelector("form")!.getAttribute("aria-busy")).toBe("false"));
  expect(ui.root.textContent).toContain("Add script text before saving.");
  expect(ui.field("title").value).toBe("Retain title"); expect(ui.field("notes").value).toBe("Retain notes"); expect(ui.field("script").value).toBe("");
});
it("generates only on explicit action with exact typed input and returns editable unsaved script", async () => {
  const ui = await setup();
  ui.edit("title", "Rain"); ui.edit("notes", "  Mara shelters a stranger.\n");
  expect(ui.invoke).not.toHaveBeenCalled();
  ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.field("script").value).toContain("Mara: Come inside."));
  expect(ui.invoke.mock.calls).toEqual([["workflow_generate_script", { projectRoot: "/fixture/project", title: "Rain", notes: "  Mara shelters a stranger.\n", apiId: "local", modelId: "writer", replaceExisting: false }]]);
  ui.edit("script", "Scene 1\nMy edited line.");
  expect(ui.field("script").value).toContain("My edited line.");
  expect(ui.invoke).toHaveBeenCalledTimes(1);
});

it("validates missing model and empty notes locally without transport", async () => {
  const ui = await setup(null);
  ui.edit("notes", "Idea"); ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.root.textContent).toContain("Select a text/chat model"));
  expect(ui.invoke).not.toHaveBeenCalled();
  ui.edit("notes", "  "); ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.root.textContent).toContain("Add notes before generating"));
  expect(ui.invoke).not.toHaveBeenCalled();
});
it.each(["provider failed", "invalid screenplay"])("retains manual script and input on %s", async error => {
  const ui = await setup(); ui.edit("notes", "Idea"); ui.edit("title", "Keep"); ui.edit("script", "Scene 7\nManual edits");
  ui.invoke.mockRejectedValueOnce(new Error(error)); ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.root.textContent).toContain(error));
  expect(ui.field("script").value).toBe("Scene 7\nManual edits"); expect(ui.field("notes").value).toBe("Idea"); expect(ui.field("title").value).toBe("Keep");
  expect(ui.invoke.mock.calls[0][1]).toMatchObject({replaceExisting:true});
});
it("file-loaded notes share the same explicit creative action and never rewrite existing Script on load", async () => {
  const ui = await setup(); ui.edit("script", "Scene 3\nKeep manual draft");
  ui.openDialog.mockResolvedValue("/external/notes.txt");
  ui.invoke.mockResolvedValueOnce("  File-loaded idea.\n");
  ui.click("Load notes");
  await vi.waitFor(() => expect(ui.field("notes").value).toBe("  File-loaded idea.\n"));
  expect(ui.field("script").value).toBe("Scene 3\nKeep manual draft");
  expect(ui.invoke.mock.calls).toEqual([["read_text_file",{path:"/external/notes.txt"}]]);
  ui.invoke.mockClear(); ui.click("Generate script from notes");
  await vi.waitFor(() => expect(ui.field("script").value).toContain("Mara: Come inside."));
  expect(ui.invoke.mock.calls[0][1]).toMatchObject({notes:"  File-loaded idea.\n",replaceExisting:true});
  expect(ui.invoke).toHaveBeenCalledTimes(1);
});
it("ignores duplicate clicks and late responses after project/draft replacement", async () => {
  const ui = await setup(); ui.edit("notes", "Old idea");
  let resolve!: (value: unknown) => void;
  ui.invoke.mockImplementationOnce(() => new Promise(r => {resolve=r;}));
  const send = [...ui.root.querySelectorAll("button")].find(b => b.textContent === "Generate script from notes")!;
  send.click(); send.click();
  await vi.waitFor(() => expect(ui.invoke).toHaveBeenCalledTimes(1));
  ui.switchProject();
  // Bind a new draft even with the same root string before the old response arrives.
  const newer = await setup(); newer.edit("notes", "New idea");
  resolve({script:"Scene 1\nMust not appear"});
  await new Promise(r => setTimeout(r,10));
  expect(newer.root.textContent).not.toContain("Must not appear");
  expect(newer.field("script").value).toBe(""); expect(newer.field("notes").value).toBe("New idea");
});
