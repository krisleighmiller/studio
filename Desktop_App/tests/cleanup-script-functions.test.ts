import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildScriptDraft } from "../src/studio/surfaces/scriptDraft";
import { loadScriptNotes } from "../src/studio/surfaces/scriptNotes";
import * as notesContract from "../src/studio/surfaces/scriptNotes";
import * as draftContract from "../src/studio/surfaces/scriptDraft";
import * as filesContract from "../src/studio/scriptFiles";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";
import type { WorkflowImportResult } from "../src/workflowTypes";

// Native JSON-RPC workflow_import capture; see evidence/sc05-generate-fixture.py.
const importedScriptFixture = JSON.parse(readFileSync(resolve("tests/fixtures/sc05-import-result.json"), "utf8")) as WorkflowImportResult;

const { refreshScriptSelection } = desk;

import { renderScriptDraftForm, type ScriptDraft } from "../src/studio/surfaces/scriptDraftForm";
import type { ScriptDraftIntent } from "../src/studio/surfaces/surfaceIntents";

describe("SC10 Plan script entry", () => {
  it("keeps the normal selection prompt when scenes exist and reuses the selected scene viewer", () => {
    const project = structuredClone(importedScriptFixture.state);
    const prompt = desk.renderPlanScriptEntry(project, undefined, null);
    expect(prompt.textContent).toBe("Select a scene.");
    expect(prompt.querySelector("button")).toBeNull();
    const scene = project.scenes.find(scene => scene.source_path === "Scripts/imported.md")!;
    const viewer = desk.renderPlanScriptEntry(project, scene, null);
    expect(viewer.matches(".workflow-script")).toBe(true);
    expect(viewer.querySelector(".workflow-script-entry")).toBeNull();
    expect(viewer.textContent?.trim().length).toBeGreaterThan(0);
  });

  it("renders the zero-project state without an impossible selection instruction", () => {
    const entry = desk.renderPlanScriptEntry(null, undefined, null);
    expect(entry.matches(".workflow-script-entry")).toBe(true);
    expect([...entry.querySelectorAll("button")].map(button => button.textContent)).toEqual(["Create script", "Import script", "Use existing project file"]);
    expect(entry.textContent).not.toMatch(/select a scene/i);
  });
  afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it.each(["zero-project", "root-only", "no-headings"])("offers actionable recovery in the Script panel for %s", async scenario => {
    const container = document.createElement("div");
    document.body.append(container);
    const initial = structuredClone(importedScriptFixture);
    initial.state.scenes = [];
    initial.report.scenes = [];
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat request"); });
    const openDialog = vi.fn(async () => scenario === "root-only" ? "/fixture/project/root.md" : "/external/script.txt");
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "ingest_script_file") return "Scripts/imported.md";
      if (cmd === "workflow_import") return structuredClone(importedScriptFixture);
      return forbidden();
    }).mockResolvedValueOnce(initial);
    vi.stubGlobal("window", { workbench: { invoke, openDialog } });
    vi.stubGlobal("fetch", forbidden);
    vi.stubGlobal("XMLHttpRequest", forbidden);
    vi.stubGlobal("WebSocket", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
    await desk.loadWorkflowDesk("/fixture/project");
    const main = () => container.querySelector<HTMLElement>(".workflow-main")!;
    const click = (label: string) => {
      const button = [...main().querySelectorAll("button")].find(button => button.textContent === label);
      expect(button, `${label} must be in the current panel`).toBeDefined();
      button!.click();
    };
    for (const label of ["Create script", "Import script", "Use existing project file"]) {
      expect([...main().querySelectorAll("button")].some(button => button.textContent === label)).toBe(true);
    }
    for (const guidance of [".md", ".markdown", ".txt", "Scene N"]) expect(main().textContent).toContain(guidance);
    expect(main().textContent).not.toMatch(/select a scene/i);
    click("Create script");
    expect(main().querySelector('textarea[name="script"]')).not.toBeNull();
    click("Cancel");
    invoke.mockClear();
    click(scenario === "root-only" ? "Use existing project file" : "Import script");
    await vi.waitFor(() => expect(container.querySelector(".workflow-script")).not.toBeNull());
    expect(container.querySelector(".workflow-script-entry")).toBeNull();
    expect(invoke.mock.calls).toEqual([
      ["ingest_script_file", { projectRoot: "/fixture/project", sourcePath: scenario === "root-only" ? "/fixture/project/root.md" : "/external/script.txt", sourceKind: scenario === "root-only" ? "existing-project-file" : "external-import" }],
      ["workflow_import", { projectRoot: "/fixture/project", persist: true }],
    ]);
    expect(forbidden).not.toHaveBeenCalled();
  });
});

describe("SC09 existing project script", () => {
  afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("registers a root-only script through the actual Plan chooser in existing-project mode", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat request"); });
    const openDialog = vi.fn(async () => "/fixture/project/root-script.md");
    const initial = structuredClone(importedScriptFixture);
    initial.state.scenes = [];
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "ingest_script_file") return "Scripts/imported.md";
      if (cmd === "workflow_import") return structuredClone(importedScriptFixture);
      return forbidden();
    }).mockResolvedValueOnce(initial);
    vi.stubGlobal("window", { workbench: { invoke, openDialog } });
    vi.stubGlobal("fetch", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
    await desk.loadWorkflowDesk("/fixture/project");
    invoke.mockClear();
    const button = [...container.querySelectorAll("button")].find(b => b.textContent === "Use existing project file");
    expect(button, "Plan existing-project entry must be reachable with zero scenes").toBeDefined();
    button!.click();
    await vi.waitFor(() => expect(desk.workflowSelection().sceneId).toBe(importedScriptFixture.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id));
    expect(invoke.mock.calls).toEqual([
      ["ingest_script_file", { projectRoot: "/fixture/project", sourcePath: "/fixture/project/root-script.md", sourceKind: "existing-project-file" }],
      ["workflow_import", { projectRoot: "/fixture/project", persist: true }],
    ]);
    expect(forbidden).not.toHaveBeenCalled();
  });
});

describe("SC08 Import script action", () => {
  async function setup(empty = false) {
    const container = document.createElement("div");
    document.body.append(container);
    const calls: string[] = [];
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat/editor request"); });
    const openDialog = vi.fn(async (): Promise<string | string[] | null> => {
      calls.push("picker");
      return "/external/Imported.MD";
    });
    const invoke = vi.fn(async (cmd: string): Promise<string | WorkflowImportResult> => {
      calls.push(cmd);
      if (cmd === "ingest_script_file") return "Scripts/imported.md";
      if (cmd === "workflow_import") return structuredClone(importedScriptFixture);
      return forbidden();
    });
    const initial = structuredClone(importedScriptFixture);
    if (empty) initial.state.scenes = [];
    invoke.mockResolvedValueOnce(initial);
    vi.stubGlobal("window", { workbench: { invoke, openDialog } });
    vi.stubGlobal("fetch", forbidden);
    vi.stubGlobal("XMLHttpRequest", forbidden);
    vi.stubGlobal("WebSocket", forbidden);
    let projectRoot = "/fixture/project";
    const status = vi.fn();
    desk.bindWorkflowDesk({ root: container, projectRoot: () => projectRoot, onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: status });
    await desk.loadWorkflowDesk(projectRoot);
    invoke.mockClear();
    calls.length = 0;
    const button = () => [...container.querySelectorAll("button")].find(button => button.textContent === "Import script");
    const click = () => {
      expect(button(), "reachable Import script action, including an empty project").toBeDefined();
      button()!.click();
    };
    return { container, calls, forbidden, invoke, openDialog, click, button, status, changeRoot: () => { projectRoot = "/other/project"; } };
  }

  afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("cancels without mutation, selection changes, or success feedback", async () => {
    const ui = await setup(true);
    ui.openDialog.mockResolvedValue(null);
    const selection = desk.workflowSelection();
    const state = structuredClone(getSurfaceHost().state);
    ui.click();
    await vi.waitFor(() => expect(ui.button()!.disabled).toBe(false));
    expect(ui.invoke).not.toHaveBeenCalled();
    expect(desk.workflowSelection()).toEqual(selection);
    expect(getSurfaceHost().state).toEqual(state);
    expect(ui.status).not.toHaveBeenCalled();
    expect(await desk.handleImportScript()).toBe("cancelled");
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it.each(["ingest", "picker", "unsupported", "multiple"])("shows %s failure without rescan or false success", async outcome => {
    const ui = await setup();
    const selection = desk.workflowSelection();
    const state = structuredClone(getSurfaceHost().state);
    if (outcome === "ingest") ui.invoke.mockRejectedValueOnce(new Error("EACCES: denied"));
    if (outcome === "picker") ui.openDialog.mockRejectedValueOnce(new Error("Dialog unavailable"));
    if (outcome === "unsupported") ui.openDialog.mockResolvedValue("/external/script.pdf");
    if (outcome === "multiple") ui.openDialog.mockResolvedValue(["/one.md", "/two.txt"]);
    ui.click();
    await vi.waitFor(() => expect(ui.button()!.disabled).toBe(false));
    expect(ui.container.textContent).toContain("Could not import script");
    expect(ui.status).toHaveBeenCalledWith("Production", expect.stringContaining("Could not import script"));
    expect(ui.invoke.mock.calls.map(([cmd]) => cmd)).toEqual(outcome === "ingest" ? ["ingest_script_file"] : []);
    expect(desk.workflowSelection()).toEqual(selection);
    expect(getSurfaceHost().state).toEqual(state);
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it("waits for ingestion and ignores duplicate clicks before refreshing", async () => {
    const ui = await setup(true);
    let finish!: (path: string) => void;
    ui.invoke.mockImplementationOnce(async cmd => {
      ui.calls.push(cmd);
      return new Promise<string>(resolve => { finish = resolve; });
    });
    const original = ui.button()!;
    original.click();
    original.click();
    await vi.waitFor(() => expect(ui.invoke).toHaveBeenCalledTimes(1));
    expect(ui.calls).toEqual(["picker", "ingest_script_file"]);
    expect(ui.button()!.disabled).toBe(true);
    expect(ui.openDialog).toHaveBeenCalledOnce();
    expect(desk.workflowSelection().sceneId).toBeNull();
    finish("Scripts/imported.md");
    await vi.waitFor(() => expect(ui.button()!.disabled).toBe(false));
    expect(ui.calls).toEqual(["picker", "ingest_script_file", "workflow_import"]);
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it.each(["picker", "ingest"])("stops after project switch during %s", async phase => {
    const ui = await setup(true);
    let finish!: (path: string) => void;
    if (phase === "picker") ui.openDialog.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    else ui.invoke.mockImplementationOnce(() => new Promise<string>(resolve => { finish = resolve; }));
    const pending = desk.handleImportScript();
    await vi.waitFor(() => expect(finish).toBeTypeOf("function"));
    ui.changeRoot();
    finish(phase === "picker" ? "/external/Imported.MD" : "Scripts/imported.md");
    expect(await pending).toEqual({ importFeedback: expect.stringContaining("project changed") });
    expect(ui.invoke.mock.calls.map(([cmd]) => cmd)).toEqual(phase === "picker" ? [] : ["ingest_script_file"]);
    expect(ui.status).not.toHaveBeenCalled();
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it.each([false, true])("picks then ingests in external mode then refreshes/focuses (zero scenes: %s)", async empty => {
    const ui = await setup(empty);
    ui.click();
    await vi.waitFor(() => expect(ui.calls).toEqual(["picker", "ingest_script_file", "workflow_import"]));
    expect(ui.openDialog).toHaveBeenCalledExactlyOnceWith({ title: "Import Script", directory: false, multiple: false,
      filters: [{ name: "Scripts", extensions: ["md", "markdown", "txt"] }] });
    expect(ui.invoke).toHaveBeenNthCalledWith(1, "ingest_script_file", {
      projectRoot: "/fixture/project", sourcePath: "/external/Imported.MD", sourceKind: "external-import",
    });
    expect(ui.invoke).toHaveBeenNthCalledWith(2, "workflow_import", { projectRoot: "/fixture/project", persist: true });
    await vi.waitFor(() => expect(desk.workflowSelection().sceneId).toBe(importedScriptFixture.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id));
    expect(ui.forbidden).not.toHaveBeenCalled();
  });
});

describe("SC07 script draft events", () => {
  async function setup(empty = false) {
    const container = document.createElement("div");
    document.body.append(container);
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat request"); });
    const calls: string[] = [];
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd !== "workflow_import") return forbidden();
      calls.push("refresh");
      return structuredClone(importedScriptFixture);
    });
    const initial = structuredClone(importedScriptFixture);
    if (empty) initial.state.scenes = [];
    invoke.mockResolvedValueOnce(initial);
    vi.stubGlobal("window", { workbench: { invoke } });
    vi.stubGlobal("fetch", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
    await desk.loadWorkflowDesk("/fixture/project");
    invoke.mockClear();
    calls.length = 0;
    const click = (label: string) => [...container.querySelectorAll("button")].find(button => button.textContent === label)!.click();
    const field = (name: string) => container.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
    const edit = (name: string, value: string) => { field(name).value = value; field(name).dispatchEvent(new Event("input", { bubbles: true })); };
    click("Create script");
    return { container, calls, invoke, forbidden, click, field, edit };
  }

  it("loads notes only and preserves the editable title and existing script", async () => {
    const ui = await setup(true);
    ui.edit("title", "Arrival");
    ui.edit("script", "Scene 9\nKeep manual script.");
    const load = vi.spyOn(notesContract, "loadScriptNotes").mockImplementation(async () => {
      ui.calls.push("load");
      return { sourcePath: "/notes.txt", text: "  Rain.\nExact notes" };
    });
    const build = vi.spyOn(draftContract, "buildScriptDraft");
    ui.click("Load notes");
    await vi.waitFor(() => expect(ui.container.querySelector("form")!.getAttribute("aria-busy")).toBe("false"));
    expect(load).toHaveBeenCalledOnce();
    expect(build).not.toHaveBeenCalled();
    expect(ui.field("script").value).toBe("Scene 9\nKeep manual script.");
    expect(ui.field("notes").value).toBe("  Rain.\nExact notes");
    expect(ui.invoke).not.toHaveBeenCalled();
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it.each(["cancelled", "read-error", "blank-notes"])("retains the edited draft after load %s without project mutation", async outcome => {
    const ui = await setup();
    ui.edit("script", "Scene 1\nKeep this edit");
    vi.spyOn(notesContract, "loadScriptNotes").mockResolvedValue(outcome === "cancelled" ? "cancelled" : outcome === "read-error" ? { readError: "Read denied" } : { sourcePath: "/empty.txt", text: "  " });
    const build = vi.spyOn(draftContract, "buildScriptDraft");
    ui.click("Load notes");
    await vi.waitFor(() => expect(ui.container.querySelector("form")!.getAttribute("aria-busy")).toBe("false"));
    expect(ui.field("script").value).toBe("Scene 1\nKeep this edit");
    expect(build).not.toHaveBeenCalled();
    if (outcome === "read-error") expect(ui.container.querySelector("[role='alert']")!.textContent).not.toBe("");
    expect(ui.invoke).not.toHaveBeenCalled();
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it.each(["blank", "invalid", "save-error"])("rejects %s without refresh and retains current edits for retry", async outcome => {
    const ui = await setup();
    ui.edit("title", "Edited title");
    ui.edit("notes", "Edited notes");
    const text = outcome === "blank" ? " \n " : outcome === "invalid" ? "Not a scene" : "Scene 1\nValid content";
    ui.edit("script", text);
    const save = vi.spyOn(filesContract, "saveProjectScript").mockResolvedValue({ saveError: outcome === "invalid" ? "No Scene headings" : "Disk full" });
    ui.click("Save script");
    await vi.waitFor(() => expect(ui.container.querySelector("form")!.getAttribute("aria-busy")).toBe("false"));
    expect(ui.field("script").value).toBe(text);
    expect(ui.field("notes").value).toBe("Edited notes");
    expect(ui.field("title").value).toBe("Edited title");
    expect(ui.container.querySelector("[role='alert']")!.textContent).toContain(outcome === "blank" ? "Add script text" : outcome === "invalid" ? "No Scene headings" : "Disk full");
    if (outcome === "blank") expect(save).not.toHaveBeenCalled();
    else expect(save).toHaveBeenCalledWith("/fixture/project", "Edited title.md", text);
    expect(ui.invoke).not.toHaveBeenCalled();
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  it("saves current two-scene text once on double-click, then refreshes and selects its source", async () => {
    const ui = await setup(true);
    ui.edit("title", "imported");
    const text = "Scene 1 — Arrival\nRain.\n\nScene 2 — Departure\nSun.\n";
    // Final text has not emitted input: save must use the rendered event snapshot.
    ui.field("script").value = text;
    let finish!: (path: string) => void;
    const save = vi.spyOn(filesContract, "saveProjectScript").mockImplementation(() => {
      ui.calls.push("save");
      return new Promise(resolve => { finish = resolve; });
    });
    const oldSave = [...ui.container.querySelectorAll("button")].find(button => button.textContent === "Save script")!;
    oldSave.click();
    oldSave.click();
    ui.click("Save script");
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("/fixture/project", "imported.md", text);
    expect(ui.invoke).not.toHaveBeenCalled();
    expect(ui.field("script").value).toBe(text);
    finish("Scripts/imported.md");
    await vi.waitFor(() => expect(ui.container.querySelector(".workflow-script-draft")).toBeNull());
    expect(ui.calls).toEqual(["save", "refresh"]);
    expect(ui.invoke).toHaveBeenCalledExactlyOnceWith("workflow_import", { projectRoot: "/fixture/project", persist: true });
    expect(desk.workflowSelection().sceneId).toBe(importedScriptFixture.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id);
    expect(getSurfaceHost().state!.scenes.filter(scene => scene.source_path === "Scripts/imported.md")).toHaveLength(2);
    expect(ui.forbidden).not.toHaveBeenCalled();
  });

  afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("opens the local form and cancels without project mutation or chat", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat call"); });
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd !== "workflow_import") return forbidden();
      return structuredClone(importedScriptFixture);
    });
    vi.stubGlobal("window", { workbench: { invoke } });
    vi.stubGlobal("fetch", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
    await desk.loadWorkflowDesk("/fixture/project");
    invoke.mockClear();
    const selection = desk.workflowSelection();
    const create = [...container.querySelectorAll("button")].find(button => button.textContent === "Create script");
    expect(create, "reachable local script authoring action").toBeDefined();
    create!.click();
    expect(container.querySelector(".workflow-script-draft")).not.toBeNull();
    [...container.querySelectorAll("button")].find(button => button.textContent === "Cancel")!.click();
    expect(container.querySelector(".workflow-script-draft")).toBeNull();
    expect(desk.workflowSelection()).toEqual(selection);
    expect(invoke).not.toHaveBeenCalled();
    expect(forbidden).not.toHaveBeenCalled();
  });
});

describe("SC06 renderScriptDraftForm", () => {
  afterEach(() => { document.body.innerHTML = ""; vi.unstubAllGlobals(); });

  it("preserves all edited fields across synchronous session rerenders and emits fresh save payloads only", () => {
    const forbidden = vi.fn(() => { throw new Error("No host/network calls in renderer"); });
    vi.stubGlobal("fetch", forbidden);
    const invoke = forbidden;
    vi.stubGlobal("workbench", { invoke });
    {
      let draft: ScriptDraft = { notes: "Notes", title: "Title", script: "Scene 1\nInitial" };
      const events: ScriptDraftIntent[] = [];
      const root = document.createElement("div");
      document.body.append(root);
      const emit = (event: ScriptDraftIntent) => {
        events.push(event);
        if (event.type === "change") {
          draft = { ...event.draft };
          root.replaceChildren(renderScriptDraftForm(draft, emit));
        }
      };
      root.append(renderScriptDraftForm(draft, emit));
      for (const [name, value] of [["title", "My title"], ["notes", "  notes\nwith detail"], ["script", "Scene 1\nFirst edit"], ["script", "Scene 1\nSecond edit\n\nScene 2\nMore."]] as const) {
        const input = root.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[name='${name}']`)!;
        input.value = value;
        input.dispatchEvent(new Event("input", { bubbles: true }));
        expect(events.at(-1)).toEqual({ type: "change", draft: { ...draft, [name]: value } });
        for (const field of ["notes", "title", "script"] as const) {
          expect(root.querySelector<HTMLInputElement>(`[name='${field}']`)!.value).toBe(draft[field]);
        }
      }
      const buttons = [...root.querySelectorAll("button")];
      expect(buttons.map(button => button.textContent)).toEqual(["Load notes", "Save notes", "Reload saved notes", "Generate script from notes", "Save script", "Retry draft recovery", "Discard draft", "Cancel"]);
      buttons[0].click();
      expect(events.at(-1)).toEqual({ type: "load-notes" });
      // Save must read live DOM values even if the final edit has not emitted input yet.
      root.querySelector<HTMLTextAreaElement>("[name='script']")!.value += "\nFinal keystroke";
      const expected = { ...draft, script: draft.script + "\nFinal keystroke" };
      buttons.find(button => button.textContent === "Save script")!.click();
      expect(events.at(-1)).toEqual({ type: "save", draft: expected });
      buttons.find(button => button.textContent === "Cancel")!.click();
      expect(events.at(-1)).toEqual({ type: "cancel" });
      expect(events).toHaveLength(7);
      expect(invoke).not.toHaveBeenCalled();
      expect(forbidden).not.toHaveBeenCalled();
    }
  });

  it("renders accessible fields, safe validation text and a local-authoring explanation", () => {
    const draft = { notes: "<b>notes</b>", title: "Title", script: "Scene 1\nScript", validationError: "Invalid script: <img src=x>" };
    const emit = vi.fn();
    const form = renderScriptDraftForm(draft, emit);
    document.body.append(form);
    expect(form.getAttribute("aria-label")).toBe("Local script draft");
    for (const name of ["title", "notes", "script"]) {
      const input = form.querySelector<HTMLInputElement>(`[name='${name}']`)!;
      expect(input.closest("label")?.querySelector("span")?.textContent).toBe(name[0].toUpperCase() + name.slice(1));
    }
    const feedback = form.querySelector<HTMLElement>("[role='alert']")!;
    expect(feedback.hidden).toBe(false);
    expect(feedback.textContent).toBe(draft.validationError);
    expect(form.querySelector("img, b")).toBeNull();
    expect(form.textContent).toContain("Local authoring");
    expect(form.textContent).toContain("asks for native approval to send Title and Notes");
    const clean = renderScriptDraftForm({ ...draft, validationError: undefined }, emit);
    expect(clean.querySelector<HTMLElement>("[role='alert']")!.hidden).toBe(true);
    expect(emit).not.toHaveBeenCalled();
    const submit = new Event("submit", { cancelable: true });
    form.dispatchEvent(submit);
    expect(submit.defaultPrevented).toBe(true);
  });

  it("notes_file_contract_copy names the project file and separates local recovery", () => {
    const form = renderScriptDraftForm(
      { notes: "Notes", title: "Title", script: "" },
      vi.fn(),
      "/projects/example",
    );
    const help = form.querySelector<HTMLElement>("[data-notes-file-contract]")!;
    expect(help.textContent).toBe(
      "Autosave is local recovery only for /projects/example. Save notes writes <project root>/Notes.md. Reload saved notes reads that file.",
    );
    expect(help.textContent).not.toMatch(/script|model/i);
  });

  it("keeps pending draft text visible while disabling controls without emitting effects", () => {
    const emit = vi.fn();
    const form = renderScriptDraftForm({ notes: "Keep notes", title: "Keep title", script: "Keep script", busy: true }, emit);
    expect(form.getAttribute("aria-busy")).toBe("true");
    for (const input of form.querySelectorAll<HTMLInputElement>("input, textarea, button")) {
      expect(input.disabled).toBe(true);
      input.click();
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    expect(form.querySelector<HTMLTextAreaElement>("[name='script']")!.value).toBe("Keep script");
    expect(emit).not.toHaveBeenCalled();
  });

  it("emits exact current text on successive edits for state-driven rerenders", () => {
    const draft = { notes: "Original notes", title: "Original title", script: "Scene 1\nOriginal script" };
    const emit = vi.fn();
    const form = renderScriptDraftForm(draft, emit);
    const notes = form.querySelector("textarea")!;
    notes.value = "First edit\n  keep spaces";
    notes.dispatchEvent(new Event("input", { bubbles: true }));
    expect(emit).toHaveBeenLastCalledWith({ type: "change", draft: { ...draft, notes: notes.value } });
    notes.value += "\nSecond edit";
    notes.dispatchEvent(new Event("input", { bubbles: true }));
    expect(emit).toHaveBeenLastCalledWith({ type: "change", draft: { ...draft, notes: notes.value } });
    expect(draft.notes).toBe("Original notes");
  });
});

describe("SC05 refreshScriptSelection", () => {
  afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML = ""; vi.unstubAllGlobals(); });

  it("publishes the actual import state before focusing the first source-matched scene", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const expected = importedScriptFixture.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!;
    const selections: desk.WorkflowSelection[] = [];
    const initial = structuredClone(importedScriptFixture);
    initial.state.scenes = initial.state.scenes.filter(scene => scene.source_path === "Scripts/earlier.md");
    const onSelectionChange = vi.fn((selection: desk.WorkflowSelection) => {
      expect(getSurfaceHost().state).toEqual(selection.sceneId === 1 ? initial.state : importedScriptFixture.state);
      expect(container.querySelector(".workflow-desk")?.getAttribute("data-scene-id")).toBe(String(selection.sceneId));
      selections.push(selection);
    });
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat/file operation"); });
    const invoke = vi.fn(async (cmd: string, args: unknown) => {
      expect(cmd).toBe("workflow_import");
      expect(args).toEqual({ projectRoot: "/fixture/project", persist: true });
      return structuredClone(importedScriptFixture);
    });
    invoke.mockResolvedValueOnce(initial);
    vi.stubGlobal("window", { workbench: { invoke } });
    vi.stubGlobal("fetch", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange, chatModel: () => null, onStatus: vi.fn() });
    await desk.loadWorkflowDesk("/fixture/project");
    expect(desk.workflowSelection().sceneId).toBe(1);
    selections.length = 0;
    invoke.mockClear();
    const result = await refreshScriptSelection("Scripts/imported.md");
    expect(desk.workflowSelection().sceneId).toBe(expected.id);
    expect(result).toEqual(expected);
    expect(selections.map(selection => selection.sceneId)).toEqual([expected.id]);
    expect(desk.workflowSelection().path).toBe("/fixture/project/Scripts/imported.md");
    expect(container.querySelector("button[data-scene-key].active")?.textContent).toContain(expected.title);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(forbidden).not.toHaveBeenCalled();
  });
  it.each(["unmatched source", "empty import", "import error"])("keeps %s truthful with visible actionable feedback", async (outcome) => {
    const container = document.createElement("div");
    document.body.append(container);
    const onStatus = vi.fn();
    const onSelectionChange = vi.fn();
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/chat/file operation"); });
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd !== "workflow_import") return forbidden();
      return structuredClone(importedScriptFixture);
    });
    vi.stubGlobal("window", { workbench: { invoke } });
    vi.stubGlobal("fetch", forbidden);
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: forbidden,
      onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange, chatModel: () => null, onStatus });
    await desk.loadWorkflowDesk("/fixture/project");
    expect(desk.workflowSelection().sceneId).toBe(1); // unrelated prior selection
    onSelectionChange.mockClear();
    const returned = structuredClone(importedScriptFixture);
    returned.note = "Import succeeded"; // must NOT leak into a failed source-specific outcome
    returned.state.scenes = outcome === "empty import" ? [] : returned.state.scenes.filter(scene => scene.source_path !== "Scripts/imported.md");
    if (outcome === "import error") invoke.mockRejectedValueOnce(new Error("EACCES: script unreadable"));
    else invoke.mockResolvedValueOnce(returned);
    const result = await refreshScriptSelection("Scripts/imported.md");
    expect(result).toHaveProperty("importFeedback");
    if (!("importFeedback" in result)) throw new Error("Unexpected imported scene");
    expect(result.importFeedback).toMatch(/Scripts\/imported.md/);
    expect(result.importFeedback).toMatch(/check.*(refresh|try again)/i);
    if (outcome === "import error") expect(result.importFeedback).toContain("EACCES");
    else expect(getSurfaceHost().state).toEqual(returned.state);
    expect(container.textContent).toContain(result.importFeedback);
    expect(container.textContent).not.toContain("Import succeeded");
    expect(onStatus).toHaveBeenLastCalledWith("Production", result.importFeedback);
    expect(getSurfaceHost().error).toBe(result.importFeedback);
    expect(desk.workflowSelection().sceneId).toBeNull();
    expect(onSelectionChange.mock.calls.every(([selection]) => selection.sceneId === null)).toBe(true);
    expect(forbidden).not.toHaveBeenCalled();
  });

  it("shows an initial import failure without claiming a scene exists", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const invoke = vi.fn(async () => { throw new Error("EACCES"); });
    vi.stubGlobal("window", { workbench: { invoke } });
    desk.bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/project", onOpenPath: vi.fn(),
      onAddToChat: vi.fn(), onOpenVoice: vi.fn(), onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
    expect(await refreshScriptSelection("Scripts/imported.md")).toEqual({ importFeedback: expect.stringContaining("EACCES") });
    expect(container.querySelector(".workflow-error")?.textContent).toContain("try again");
    expect(desk.workflowSelection().sceneId).toBeNull();
    expect(getSurfaceHost().state).toBeNull();
  });

  it("does not publish an import completed after a project switch", async () => {
    const container = document.createElement("div");
    let projectRoot = "/fixture/project";
    let resolveImport!: (value: WorkflowImportResult) => void;
    const invoke = vi.fn(() => new Promise<WorkflowImportResult>(resolve => { resolveImport = resolve; }));
    const onStatus = vi.fn(), onSelectionChange = vi.fn();
    vi.stubGlobal("window", { workbench: { invoke } });
    desk.bindWorkflowDesk({ root: container, projectRoot: () => projectRoot, onOpenPath: vi.fn(),
      onAddToChat: vi.fn(), onOpenVoice: vi.fn(), onComposeChat: vi.fn(),
      onSelectionChange, chatModel: () => null, onStatus });
    const pending = refreshScriptSelection("Scripts/imported.md");
    projectRoot = "/fixture/other";
    desk.clearWorkflowDesk();
    resolveImport(structuredClone(importedScriptFixture));
    expect(await pending).toEqual({ importFeedback: expect.stringContaining("project changed") });
    expect(getSurfaceHost().state).toBeNull();
    expect(onSelectionChange).not.toHaveBeenCalled();
    expect(onStatus).not.toHaveBeenCalled();
  });
});

// Exercise the production host.ts adapters, not a replacement notes loader.
describe("SC02 loadScriptNotes", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("returns exact selected text using the native picker/read boundary only", async () => {
    const text = "\ufeff  Rain.\r\n\r\nPip — waits.  \n";
    const openDialog = vi.fn(async () => "/notes/Rain.md");
    const invoke = vi.fn(async (cmd: string, args: unknown) => {
      expect(cmd).toBe("read_text_file");
      expect(args).toEqual({ path: "/notes/Rain.md" });
      return text;
    });
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider request"); });
    vi.stubGlobal("window", { workbench: { openDialog, invoke } });
    vi.stubGlobal("fetch", forbidden);
    expect(await loadScriptNotes()).toEqual({ sourcePath: "/notes/Rain.md", text });
    expect(openDialog).toHaveBeenCalledExactlyOnceWith({
      title: "Load Script Notes", directory: false, multiple: false,
      filters: [{ name: "Script notes", extensions: ["md", "markdown", "txt"] }],
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(forbidden).not.toHaveBeenCalled();
  });

  it.each(["md", "markdown", "txt", "MD"])("reads .%s fixture bytes without writes or refresh/model calls", async (extension) => {
    const directory = mkdtempSync(resolve(tmpdir(), "studio-sc02-"));
    const sourcePath = `${directory}/notes.${extension}`;
    const text = extension === "txt" ? "" : `\ufeff${extension}\r\n  exact — text  \n`;
    writeFileSync(sourcePath, text);
    const before = readFileSync(sourcePath);
    const forbidden = vi.fn(() => { throw new Error("Unexpected host/provider operation"); });
    const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd !== "read_text_file" || args?.path !== sourcePath) return forbidden();
      return readFileSync(sourcePath, "utf8");
    });
    vi.stubGlobal("window", { workbench: new Proxy({ openDialog: async () => sourcePath, invoke }, {
      get(target, key) { return key in target ? target[key as keyof typeof target] : forbidden; },
    }) });
    vi.stubGlobal("fetch", forbidden);
    vi.stubGlobal("XMLHttpRequest", forbidden);
    vi.stubGlobal("WebSocket", forbidden);
    try {
      expect(await loadScriptNotes()).toEqual({ sourcePath, text });
      expect(invoke).toHaveBeenCalledExactlyOnceWith("read_text_file", { path: sourcePath });
      expect(readFileSync(sourcePath)).toEqual(before);
      expect(forbidden).not.toHaveBeenCalled();
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("returns cancellation without reading or creating a successful empty draft", async () => {
    const invoke = vi.fn(() => { throw new Error("Unexpected read/write/refresh/model call"); });
    vi.stubGlobal("window", { workbench: { openDialog: async () => null, invoke } });
    expect(await loadScriptNotes()).toBe("cancelled");
    expect(invoke).not.toHaveBeenCalled();
  });

  it.each(["notes.pdf", "notes.md.exe", "notes", "folder.md/notes"])("rejects unsupported input %s before any read", async (name) => {
    const invoke = vi.fn(async () => "must not read");
    vi.stubGlobal("window", { workbench: { openDialog: async () => `/notes/${name}`, invoke } });
    expect(await loadScriptNotes()).toEqual({ readError: expect.stringMatching(/choose.*\.md.*\.markdown.*\.txt/i) });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns an actionable read error without changing existing source bytes", async () => {
    const directory = mkdtempSync(resolve(tmpdir(), "studio-sc02-"));
    const sourcePath = `${directory}/notes.md`;
    writeFileSync(sourcePath, "Preserve me.\r\n");
    const before = readFileSync(sourcePath);
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd !== "read_text_file") throw new Error(`Unexpected command ${cmd}`);
      throw new Error("EACCES: permission denied");
    });
    vi.stubGlobal("window", { workbench: { openDialog: async () => sourcePath, invoke } });
    try {
      expect(await loadScriptNotes()).toEqual({ readError: expect.stringContaining("permission denied") });
      expect(await loadScriptNotes()).toEqual({ readError: expect.stringContaining(sourcePath) });
      expect(readFileSync(sourcePath)).toEqual(before);
      expect(invoke.mock.calls.every(([cmd]) => cmd === "read_text_file")).toBe(true);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("returns picker failures as actionable errors, not cancellation", async () => {
    const invoke = vi.fn();
    vi.stubGlobal("window", { workbench: { openDialog: async () => { throw new Error("Dialog unavailable"); }, invoke } });
    expect(await loadScriptNotes()).toEqual({ readError: expect.stringMatching(/try again.*Dialog unavailable/i) });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("refuses a multiple-selection response rather than reading an arbitrary file", async () => {
    const invoke = vi.fn();
    vi.stubGlobal("window", { workbench: { openDialog: async () => ["/a.md", "/b.txt"], invoke } });
    expect(await loadScriptNotes()).toEqual({ readError: "Select exactly one notes file." });
    expect(invoke).not.toHaveBeenCalled();
  });
});

// Parse scene spans using the host script.rs heading grammar. This tests the
// wire format without introducing a host call into the pure authoring boundary.
function sceneSections(text: string) {
  const headings = [...text.matchAll(/^\s{0,3}#{0,6}\s*scene\s+(\d+)\b[^\n]*$/gim)];
  return headings.map((heading, index) => ({
    id: Number(heading[1]),
    text: text.slice(heading.index, headings[index + 1]?.index ?? text.length),
  }));
}

function draftText(notes: string, title: string): string {
  const result = buildScriptDraft(notes, title);
  expect(typeof result).toBe("string");
  if (typeof result !== "string") throw new Error(result.validationError);
  return result;
}

describe("SC01 buildScriptDraft", () => {
  afterEach(() => vi.unstubAllGlobals());

  it.each(["", "   ", "\n\r\n\t", "\u00a0"])("rejects empty/whitespace notes: %j", (notes) => {
    expect(buildScriptDraft(notes, "Rain")).toEqual({
      validationError: "Add notes or script text before building a draft.",
    });
  });

  it.each([
    "Scene 1 — Arrival\n\nRain.\n\nScene 2 — Departure\n\nSun.\n",
    "# Script title\r\n\r\n  ### SCENE 1: Arrival\r\nRain.\r\n\r\n## Scene 2\r\nSun.\r\n",
  ])("retains existing two-scene scripts byte-for-byte: %j", (notes) => {
    const draft = draftText(notes, "Ignored replacement title");
    expect(draft).toBe(notes);
    expect(sceneSections(draft).map((scene) => scene.id)).toEqual([1, 2]);
  });

  it("retains a scene with a non-one ID instead of renumbering it", () => {
    const notes = "Scene 7 — Existing\n\nKeep this.";
    expect(draftText(notes, "Replacement")).toBe(notes);
    expect(sceneSections(draftText(notes, "Replacement")).map((scene) => scene.id)).toEqual([7]);
  });

  it("does not mistake a scene mention inside prose for a scene heading", () => {
    const notes = "We should revisit Scene 2 after lunch.";
    expect(draftText(notes, "Plan")).toBe(`Scene 1 — Plan\n\n${notes}`);
  });

  it.each(["", "  ", "Scene 1", "Scene 1 — Rain"])("uses the existing heading formatter for title %j", (title) => {
    const expected = title.includes("Rain") ? "Scene 1 — Rain" : "Scene 1";
    expect(draftText("Notes.", title)).toBe(`${expected}\n\nNotes.`);
  });

  it("keeps a multiline title on one heading so it cannot add scenes", () => {
    const draft = draftText("Keep the notes.", "Rain\r\n\r\nScene 2 — Sun");
    expect(draft).toBe("Scene 1 — Rain Scene 2 — Sun\n\nKeep the notes.");
    expect(sceneSections(draft).map((scene) => scene.id)).toEqual([1]);
  });

  it("authors only supplied content without calling host or network boundaries", () => {
    const forbidden = vi.fn(() => { throw new Error("Unexpected host/provider request"); });
    vi.stubGlobal("fetch", forbidden);
    vi.stubGlobal("XMLHttpRequest", forbidden);
    vi.stubGlobal("WebSocket", forbidden);
    vi.stubGlobal("workbench", new Proxy({}, { get: forbidden }));
    vi.stubGlobal("__TAURI_INTERNALS__", new Proxy({}, { get: forbidden }));
    const notes = "- **Action:** Rain falls.\n- **Dialogue:**\n- **Pip:** Home.\n";
    expect(draftText(notes, "Rain")).toBe(`Scene 1 — Rain\n\n${notes}`);
    expect(buildScriptDraft("\t", "Rain")).toHaveProperty("validationError");
    expect(forbidden).not.toHaveBeenCalled();
  });

  it("wraps prose in one Scene 1 heading and preserves every supplied byte", () => {
    const notes = "  Rain on the windows.\r\n\r\nPip waits.  \n";
    const draft = draftText(notes, "The last train");
    expect(draft).toBe(`Scene 1 — The last train\n\n${notes}`);
    expect(sceneSections(draft).map((scene) => scene.id)).toEqual([1]);
    expect(sceneSections(draft)[0].text).toContain(notes);
  });
});
