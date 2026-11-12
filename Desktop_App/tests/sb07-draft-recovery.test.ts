import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
afterEach(() => { desk.clearWorkflowDesk(); localStorage.clear(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });
async function setup() {
  localStorage.clear();
  const root = document.createElement("div"); document.body.append(root);
  const initial = structuredClone(fixture); initial.state.scenes = []; initial.report.scenes = [];
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "workflow_import") return structuredClone(initial);
    throw new Error(`Forbidden: ${cmd}`);
  });
  const forbidden = vi.fn(() => { throw new Error("No provider or chat permitted"); });
  vi.stubGlobal("fetch", forbidden);
  const ask = vi.fn(async () => true);
  vi.stubGlobal("window", { workbench: { invoke, ask } });
  let project = "/disposable/sb07/A";
  desk.bindWorkflowDesk({ root, projectRoot: () => project, chatModel: () => null,
    onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
    onSelectionChange: vi.fn(), onStatus: vi.fn() });
  const open = async (path = project) => { desk.clearWorkflowDesk(); project = path; await desk.loadWorkflowDesk(project); };
  const click = (label: string) => { const button = [...root.querySelectorAll("button")].find(b => b.textContent === label); expect(button, label).toBeDefined(); button!.click(); };
  const field = (name: string) => root.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
  const edit = (name: string, text: string) => { field(name).value = text; field(name).dispatchEvent(new Event("input")); };
  await open(); click("Create script"); invoke.mockClear();
  return { root, invoke, forbidden, open, click, field, edit, ask };
}
it("recovers exact unsaved Title/Notes/Script after close/reopen, isolates projects, and discards explicitly", async () => {
  const ui = await setup();
  const expected = { title: "  Rain — 未完  ", notes: "  First thought\n\nLast thought ☔\n", script: "Not a complete script.\n  Keep whitespace.\n" };
  for (const [name, value] of Object.entries(expected)) ui.edit(name, value);
  expect(ui.root.textContent).toContain("Recovery copy updated locally. No project file was written.");
  ui.click("Cancel"); ui.click("Create script");
  for (const [name, value] of Object.entries(expected)) expect(ui.field(name).value).toBe(value);
  await ui.open("/disposable/sb07/B"); ui.click("Create script");
  expect(ui.field("notes").value).toBe(""); ui.edit("notes", "Only B");
  await ui.open("/disposable/sb07/A"); ui.click("Create script");
  for (const [name, value] of Object.entries(expected)) expect(ui.field(name).value).toBe(value);
  expect(ui.root.textContent).toContain("Recovered local draft");
  ui.click("Discard draft");
  await vi.waitFor(() => expect(ui.root.querySelector("form")).toBeNull());
  await ui.open(); ui.click("Create script"); expect(ui.field("script").value).toBe("");
  await ui.open("/disposable/sb07/B"); ui.click("Create script"); expect(ui.field("notes").value).toBe("Only B");
  expect(ui.invoke.mock.calls.every(([cmd]) => cmd === "workflow_import")).toBe(true);
  expect(ui.forbidden).not.toHaveBeenCalled();
});

it("retains failed-write edits, preserves the prior durable copy, and retries without losing focus", async () => {
  const ui = await setup(); ui.edit("notes", "Durable notes");
  const before = { ...localStorage };
  const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => { throw new DOMException("Full", "QuotaExceededError"); });
  const field = ui.field("script"); field.focus(); field.setSelectionRange(0, 0);
  ui.edit("script", "Latest unsaved text\n");
  expect(document.activeElement).toBe(field);
  expect(ui.root.querySelector('[data-draft-recovery][role="alert"]')?.textContent).toContain("only in memory");
  expect({ ...localStorage }).toEqual(before);
  ui.click("Cancel"); ui.click("Create script"); expect(ui.field("script").value).toBe("Latest unsaved text\n");
  await ui.open("/disposable/sb07/B"); ui.click("Create script"); expect(ui.field("script").value).toBe("");
  await ui.open("/disposable/sb07/A"); expect(ui.field("script").value).toBe("Latest unsaved text\n");
  write.mockRestore(); ui.click("Retry draft recovery");
  await vi.waitFor(() => expect(ui.root.textContent).toContain("Recovery copy updated locally. No project file was written."));
  await ui.open(); expect(ui.field("script").value).toBe("Latest unsaved text\n");
  expect(ui.invoke.mock.calls.every(([cmd]) => cmd === "workflow_import")).toBe(true);
});

it("declined, failed and stale discard cannot erase either project's draft", async () => {
  const ui = await setup(); ui.edit("notes", "Keep A");
  ui.ask.mockResolvedValueOnce(false); ui.click("Discard draft");
  await vi.waitFor(() => expect(ui.root.querySelector('form')?.getAttribute('aria-busy')).toBe("false"));
  expect(ui.field("notes").value).toBe("Keep A");
  const remove = vi.spyOn(localStorage, "removeItem").mockImplementation(() => { throw new Error("Unavailable"); });
  ui.click("Discard draft");
  await vi.waitFor(() => expect(ui.root.textContent).toContain("Could not clear"));
  expect(ui.field("notes").value).toBe("Keep A"); remove.mockRestore();
  let finish!: (accepted: boolean) => void;
  ui.ask.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })); ui.click("Discard draft");
  await ui.open("/disposable/sb07/B"); ui.click("Create script"); ui.edit("notes", "Keep B");
  finish(true); await new Promise(resolve => setTimeout(resolve, 0));
  expect(ui.field("notes").value).toBe("Keep B");
  await ui.open("/disposable/sb07/A"); expect(ui.field("notes").value).toBe("Keep A");
});

it("a late project load cannot publish another project's recovered draft", async () => {
  const ui = await setup(); ui.edit("notes", "Private A");
  let release!: (result: WorkflowImportResult) => void;
  ui.invoke.mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
  const openingA = ui.open("/disposable/sb07/A");
  await ui.open("/disposable/sb07/B"); ui.click("Create script"); ui.edit("notes", "Private B");
  release(structuredClone(fixture)); await openingA;
  expect(ui.field("notes").value).toBe("Private B");
});
