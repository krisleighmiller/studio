import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;

afterEach(() => {
  desk.clearWorkflowDesk();
  localStorage.clear();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("notes_save_intent_file_call: routes one click once with the active root and exact Notes text", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = "/disposable/w3-07";
  const notes = "  Exact Notes text — café, 雨, 🎬\n\nTrailing spaces stay.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") return "Notes.md";
    throw new Error(`Unexpected command: ${command}`);
  });
  const forbidden = vi.fn(() => { throw new Error("Unexpected external effect"); });
  vi.stubGlobal("window", { workbench: { invoke, openDialog: forbidden } });
  desk.bindWorkflowDesk({
    root,
    projectRoot: () => projectRoot,
    onOpenPath: forbidden,
    onAddToChat: forbidden,
    onOpenVoice: forbidden,
    onComposeChat: forbidden,
    onSelectionChange: vi.fn(),
    chatModel: () => null,
    onStatus: vi.fn(),
  });

  await desk.loadWorkflowDesk(projectRoot);
  const click = (label: string) => {
    const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
    expect(button, label).toBeDefined();
    button!.click();
  };
  click("Create script");
  const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
  notesField.value = notes;
  notesField.dispatchEvent(new Event("input"));

  click("Save notes");
  await vi.waitFor(() => {
    expect(invoke.mock.calls.filter(([command]) => command === "save_project_notes")).toHaveLength(1);
  });

  expect(invoke.mock.calls.filter(([command]) => command === "save_project_notes")).toEqual([
    ["save_project_notes", { projectRoot, text: notes }],
  ]);
  expect(forbidden).not.toHaveBeenCalled();
});

it("notes_save_no_model_call: saves with no model and no provider, generation, or approval command", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = "/disposable/w3-08";
  const notes = "Provider-free notes save.\n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") return "Notes.md";
    throw new Error(`Forbidden provider/generation/approval command: ${command}`);
  });
  const forbidden = vi.fn(() => { throw new Error("Forbidden external effect"); });
  vi.stubGlobal("window", { workbench: { invoke, openDialog: forbidden } });
  desk.bindWorkflowDesk({
    root,
    projectRoot: () => projectRoot,
    onOpenPath: forbidden,
    onAddToChat: forbidden,
    onOpenVoice: forbidden,
    onComposeChat: forbidden,
    onSelectionChange: vi.fn(),
    chatModel: () => null,
    onStatus: vi.fn(),
  });

  await desk.loadWorkflowDesk(projectRoot);
  const click = (label: string) => {
    const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
    expect(button, label).toBeDefined();
    button!.click();
  };
  click("Create script");
  const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
  notesField.value = notes;
  notesField.dispatchEvent(new Event("input"));

  click("Save notes");
  await vi.waitFor(() => {
    expect(invoke.mock.calls.filter(([command]) => command === "save_project_notes")).toHaveLength(1);
  });

  expect(invoke.mock.calls).toEqual([
    ["workflow_import", { projectRoot, persist: true }],
    ["save_project_notes", { projectRoot, text: notes }],
  ]);
  expect(forbidden).not.toHaveBeenCalled();
});

it("notes_save_no_script_registration: saves Notes.md without creating or registering a script", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-09-"));
  const notes = "Notes stay notes — café, 雨, 🎬\n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      writeFileSync(join(projectRoot, "Notes.md"), String(args?.text), "utf8");
      return "Notes.md";
    }
    throw new Error(`Forbidden script registration command: ${command}`);
  });
  const forbidden = vi.fn(() => { throw new Error("Forbidden external effect"); });
  vi.stubGlobal("window", { workbench: { invoke, openDialog: forbidden } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: forbidden,
      onAddToChat: forbidden,
      onOpenVoice: forbidden,
      onComposeChat: forbidden,
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    const scriptField = root.querySelector<HTMLTextAreaElement>('textarea[name="script"]')!;
    expect(scriptField.value).toBe("");
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));

    click("Save notes");
    await vi.waitFor(() => expect(readFileSync(join(projectRoot, "Notes.md"), "utf8")).toBe(notes));

    expect(existsSync(join(projectRoot, "Scripts"))).toBe(false);
    expect(desk.workflowSelection().sceneId).toBeNull();
    expect(invoke.mock.calls).toEqual([
      ["workflow_import", { projectRoot, persist: true }],
      ["save_project_notes", { projectRoot, text: notes }],
    ]);
    expect(forbidden).not.toHaveBeenCalled();
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_save_success_path: reports the returned project file separately from local recovery", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = "/disposable/w3-10";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") return "Notes.md";
    throw new Error(`Unexpected command: ${command}`);
  });
  const onStatus = vi.fn();
  vi.stubGlobal("window", { workbench: { invoke } });
  desk.bindWorkflowDesk({
    root,
    projectRoot: () => projectRoot,
    onOpenPath: vi.fn(),
    onAddToChat: vi.fn(),
    onOpenVoice: vi.fn(),
    onComposeChat: vi.fn(),
    onSelectionChange: vi.fn(),
    chatModel: () => null,
    onStatus,
  });

  await desk.loadWorkflowDesk(projectRoot);
  const click = (label: string) => {
    const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
    expect(button, label).toBeDefined();
    button!.click();
  };
  click("Create script");
  const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
  notesField.value = "Visible save-path status.\n";
  notesField.dispatchEvent(new Event("input"));

  click("Save notes");
  await vi.waitFor(() => expect(onStatus).toHaveBeenCalledWith("Notes saved to Notes.md"));

  const recovery = root.querySelector<HTMLElement>("[data-draft-recovery]")?.textContent ?? "";
  expect(recovery).toContain("Notes saved to Notes.md");
  expect(recovery).toContain("Local recovery is also current");
  expect(recovery).not.toContain("No project file was written");
  expect(onStatus).not.toHaveBeenCalledWith(expect.stringContaining("local"));
});

it("notes_save_files_refresh: publishes Notes.md to the open Files tree after settlement", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-11-"));
  const notes = "Publish this exact file to Files.\n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const events: string[] = [];
  let filesTree: string[] = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      writeFileSync(join(projectRoot, "Notes.md"), String(args?.text), "utf8");
      events.push("notes-settled");
      return "Notes.md";
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
      onProjectFilesChanged: (isCurrent) => {
        expect(isCurrent()).toBe(true);
        filesTree = readdirSync(projectRoot);
        events.push("files-refreshed");
      },
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));

    click("Save notes");
    await vi.waitFor(() => expect(filesTree).toContain("Notes.md"));

    expect(events).toEqual(["notes-settled", "files-refreshed"]);
    expect(readFileSync(join(projectRoot, "Notes.md"), "utf8")).toBe(notes);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_reload_reads_file: replaces Notes with Notes.md instead of local recovery", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-12-"));
  const recoveryNotes = "Text A from local recovery.\n";
  const fileNotes = "Text B from Notes.md — café, 雨, 🎬\n\nExact disk text.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  localStorage.setItem(`studio.local-script-draft.v1:${projectRoot}`, JSON.stringify({
    version: 1,
    projectRoot,
    title: "Recovery title",
    notes: recoveryNotes,
    script: "Recovery script",
  }));
  writeFileSync(join(projectRoot, "Notes.md"), fileNotes, "utf8");
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "read_text_file") return readFileSync(String(args?.path), "utf8");
    throw new Error(`Unexpected command: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    expect(notesField().value).toBe(recoveryNotes);
    const reload = [...root.querySelectorAll("button")].find(node => node.textContent === "Reload saved notes");
    expect(reload).toBeDefined();
    reload!.click();
    await vi.waitFor(() => expect(notesField().value).toBe(fileNotes));

    expect(notesField().value).not.toBe(recoveryNotes);
    expect(invoke.mock.calls.filter(([command]) => command === "read_text_file")).toEqual([
      ["read_text_file", { path: join(projectRoot, "Notes.md") }],
    ]);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_reload_preserves_title: leaves the unsaved Title byte-for-byte unchanged", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-13-"));
  const unsavedTitle = "  Unsaved title — café, 雨, 🎬  ";
  const fileNotes = "Different Notes.md text.\n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  writeFileSync(join(projectRoot, "Notes.md"), fileNotes, "utf8");
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "read_text_file") return readFileSync(String(args?.path), "utf8");
    throw new Error(`Unexpected command: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const titleField = () => root.querySelector<HTMLInputElement>('input[name="title"]')!;
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    titleField().value = unsavedTitle;
    titleField().dispatchEvent(new Event("input"));

    click("Reload saved notes");
    await vi.waitFor(() => expect(notesField().value).toBe(fileNotes));

    expect(titleField().value).toBe(unsavedTitle);
    expect(invoke.mock.calls.filter(([command]) => command === "read_text_file")).toEqual([
      ["read_text_file", { path: join(projectRoot, "Notes.md") }],
    ]);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_reload_preserves_script: leaves unsaved Script unchanged and unregistered", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-14-"));
  const unsavedScript = "  INT. CAFÉ — NIGHT\n\n雨 falls outside. 🎬  \n";
  const fileNotes = "Different Notes.md text.\n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  writeFileSync(join(projectRoot, "Notes.md"), fileNotes, "utf8");
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "read_text_file") return readFileSync(String(args?.path), "utf8");
    throw new Error(`Unexpected command: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    const scriptField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="script"]')!;
    scriptField().value = unsavedScript;
    scriptField().dispatchEvent(new Event("input"));

    click("Reload saved notes");
    await vi.waitFor(() => expect(notesField().value).toBe(fileNotes));

    expect(scriptField().value).toBe(unsavedScript);
    expect(desk.workflowSelection().sceneId).toBeNull();
    expect(existsSync(join(projectRoot, "Scripts"))).toBe(false);
    expect(invoke.mock.calls).toEqual([
      ["workflow_import", { projectRoot, persist: true }],
      ["read_text_file", { path: join(projectRoot, "Notes.md") }],
    ]);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_reload_read_failure: keeps newer editable Notes and says Notes.md was not loaded", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-15-"));
  const notesPath = join(projectRoot, "Notes.md");
  const newerNotes = "  Newer editable Notes — café, 雨, 🎬\n\nKeep this exact text.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  writeFileSync(notesPath, "Older saved notes.\n", "utf8");
  chmodSync(notesPath, 0o000);
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "read_text_file") {
      expect(String(args?.path)).toBe(notesPath);
      expect(statSync(notesPath).mode & 0o777).toBe(0);
      throw new Error("Permission denied while reading Notes.md");
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField().value = newerNotes;
    notesField().dispatchEvent(new Event("input"));

    click("Reload saved notes");
    await vi.waitFor(() => expect(root.querySelector('[role="alert"]')?.textContent)
      .toContain("Notes.md was not loaded"));

    expect(notesField().value).toBe(newerNotes);
    expect(root.querySelector('[role="alert"]')?.textContent).toContain("Your current Notes were kept");
    expect(invoke.mock.calls.filter(([command]) => command === "read_text_file")).toEqual([
      ["read_text_file", { path: notesPath }],
    ]);
  } finally {
    if (existsSync(notesPath)) chmodSync(notesPath, 0o600);
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_recovery_not_file_success: edits recovery without writing Notes.md or reporting file-save success", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-16-"));
  const notes = "  Recovery-only Notes — café, 雨, 🎬\n\nNo file save yet.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string) => {
    if (command === "workflow_import") return structuredClone(empty);
    throw new Error(`Unexpected command: ${command}`);
  });
  const onStatus = vi.fn();
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus,
    });

    await desk.loadWorkflowDesk(projectRoot);
    const create = [...root.querySelectorAll("button")].find(node => node.textContent === "Create script");
    expect(create).toBeDefined();
    create!.click();
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));

    const recovery = JSON.parse(localStorage.getItem(`studio.local-script-draft.v1:${projectRoot}`)!);
    const recoveryStatus = root.querySelector<HTMLElement>("[data-draft-recovery]")?.textContent ?? "";
    expect(recovery.notes).toBe(notes);
    expect(existsSync(join(projectRoot, "Notes.md"))).toBe(false);
    expect(invoke.mock.calls).toEqual([["workflow_import", { projectRoot, persist: true }]]);
    expect(recoveryStatus).toBe("Recovery copy updated locally. No project file was written. Cancel keeps this recovery copy.");
    expect(recoveryStatus).not.toContain("Notes saved");
    expect(onStatus).not.toHaveBeenCalled();
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_file_failure_keeps_recovery: reopens exact Notes and reports that Notes.md was not updated", async () => {
  const root = document.createElement("div");
  const footer = document.createElement("div");
  document.body.append(root, footer);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-17-"));
  const notesPath = join(projectRoot, "Notes.md");
  const tempPath = join(projectRoot, "Notes.md.tmp");
  const priorFile = "  First saved Notes — café, 雨, 🎬\n\nThis stays on disk.  \n";
  const latestNotes = "  Latest recovered Notes — café, 雨, 🎬\n\nKeep trailing spaces.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      if (existsSync(tempPath) && statSync(tempPath).isDirectory()) {
        throw new Error("Could not write Notes.md temporary file: Is a directory (os error 21)");
      }
      writeFileSync(notesPath, String(args?.text), "utf8");
      return "Notes.md";
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  const onStatus = vi.fn((left: string, right = "") => {
    footer.textContent = [left, right].filter(Boolean).join(" ");
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus,
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField().value = priorFile;
    notesField().dispatchEvent(new Event("input"));
    click("Save notes");
    await vi.waitFor(() => expect(footer.textContent).toBe("Notes saved to Notes.md"));
    expect(readFileSync(notesPath, "utf8")).toBe(priorFile);

    notesField().value = latestNotes;
    notesField().dispatchEvent(new Event("input"));
    mkdirSync(tempPath);

    click("Save notes");
    await vi.waitFor(() => expect(root.querySelector('[role="alert"]')?.textContent)
      .toContain("Notes.md was not updated"));
    expect(readFileSync(notesPath, "utf8")).toBe(priorFile);
    expect(footer.textContent).toContain("Notes.md was not updated");
    expect(footer.textContent).not.toContain("Notes saved");

    click("Cancel");
    click("Create script");
    expect(notesField().value).toBe(latestNotes);
    expect(JSON.parse(localStorage.getItem(`studio.local-script-draft.v1:${projectRoot}`)!).notes).toBe(latestNotes);
    expect(root.textContent).not.toContain("Notes saved");
    expect(footer.textContent).toContain("Notes.md was not updated");
    expect(footer.textContent).not.toContain("Notes saved");
    expect(invoke.mock.calls.filter(([command]) => command === "save_project_notes")).toEqual([
      ["save_project_notes", { projectRoot, text: priorFile }],
      ["save_project_notes", { projectRoot, text: latestNotes }],
    ]);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_file_project_isolation: settles the save only for its captured project", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const disposableRoot = mkdtempSync(join(tmpdir(), "studio-w3-18-"));
  const projectA = join(disposableRoot, "project-a");
  const projectB = join(disposableRoot, "project-b");
  mkdirSync(projectA);
  mkdirSync(projectB);
  const notes = "  Project A Notes — café, 雨, 🎬\n\nStay isolated.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  let activeRoot = projectA;
  let releaseSave!: () => void;
  const saveGate = new Promise<void>(resolve => { releaseSave = resolve; });
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      await saveGate;
      writeFileSync(join(String(args?.projectRoot), "Notes.md"), String(args?.text), "utf8");
      return "Notes.md";
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  const onStatus = vi.fn();
  const onProjectFilesChanged = vi.fn();
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => activeRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus,
      onProjectFilesChanged,
    });

    await desk.loadWorkflowDesk(projectA);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));
    click("Save notes");
    await vi.waitFor(() => expect(invoke.mock.calls.filter(([command]) => command === "save_project_notes"))
      .toEqual([["save_project_notes", { projectRoot: projectA, text: notes }]]));

    activeRoot = projectB;
    await desk.loadWorkflowDesk(projectB);
    releaseSave();
    await vi.waitFor(() => expect(readFileSync(join(projectA, "Notes.md"), "utf8")).toBe(notes));

    expect(existsSync(join(projectB, "Notes.md"))).toBe(false);
    expect(onStatus).not.toHaveBeenCalledWith(expect.stringContaining("Notes saved"));
    expect(onProjectFilesChanged).not.toHaveBeenCalled();
    expect(root.textContent).not.toContain("Notes saved");
  } finally {
    releaseSave();
    rmSync(disposableRoot, { recursive: true, force: true });
  }
});

it("notes_load_requires_explicit_save: loads external Notes without changing project Notes.md", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const disposableRoot = mkdtempSync(join(tmpdir(), "studio-w3-19-"));
  const projectRoot = join(disposableRoot, "project");
  const externalPath = join(disposableRoot, "external-notes.md");
  const notesPath = join(projectRoot, "Notes.md");
  const priorFile = "Prior project Notes stay unchanged.\n";
  const loadedNotes = "  External Notes — café, 雨, 🎬\n\nAcquisition only.  \n";
  mkdirSync(projectRoot);
  writeFileSync(notesPath, priorFile, "utf8");
  writeFileSync(externalPath, loadedNotes, "utf8");
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "read_text_file") return readFileSync(String(args?.path), "utf8");
    throw new Error(`Unexpected command: ${command}`);
  });
  const openDialog = vi.fn(async () => externalPath);
  const onStatus = vi.fn();
  vi.stubGlobal("window", { workbench: { invoke, openDialog } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus,
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    click("Load notes");
    const notesField = () => root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    await vi.waitFor(() => expect(notesField().value).toBe(loadedNotes));

    expect(openDialog).toHaveBeenCalledOnce();
    expect(invoke.mock.calls).toEqual([
      ["workflow_import", { projectRoot, persist: true }],
      ["read_text_file", { path: externalPath }],
    ]);
    expect(readFileSync(notesPath, "utf8")).toBe(priorFile);
    expect(JSON.parse(localStorage.getItem(`studio.local-script-draft.v1:${projectRoot}`)!).notes).toBe(loadedNotes);
    expect(onStatus).not.toHaveBeenCalledWith(expect.stringContaining("Notes saved"));
  } finally {
    rmSync(disposableRoot, { recursive: true, force: true });
  }
});

it("notes_saved_file_no_implicit_generate: saves Notes.md without generating before Generate is clicked", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-20-"));
  const notesPath = join(projectRoot, "Notes.md");
  const notes = "  Saved Notes with a fake model — café, 雨, 🎬\n\nGeneration stays explicit.  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      writeFileSync(notesPath, String(args?.text), "utf8");
      return "Notes.md";
    }
    throw new Error(`Unexpected generation or provider command before Generate click: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => ({ apiId: "fake-api", modelId: "fake-model" }),
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));
    expect([...root.querySelectorAll("button")].some(node => node.textContent === "Generate script from notes")).toBe(true);

    click("Save notes");
    await vi.waitFor(() => expect(readFileSync(notesPath, "utf8")).toBe(notes));

    expect(invoke.mock.calls).toEqual([
      ["workflow_import", { projectRoot, persist: true }],
      ["save_project_notes", { projectRoot, text: notes }],
    ]);
    expect(invoke.mock.calls.some(([command]) => command === "workflow_generate_script")).toBe(false);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});

it("notes_saved_file_no_implicit_script_save: leaves editable Script unsaved and unregistered after Save notes", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const projectRoot = mkdtempSync(join(tmpdir(), "studio-w3-21-"));
  const notesPath = join(projectRoot, "Notes.md");
  const notes = "  Notes save independently — café, 雨, 🎬\n\nScript stays editable only.  \n";
  const script = "  INT. CAFÉ — NIGHT\n\nAn unsaved scene remains in the editor. 🎬  \n";
  const empty = structuredClone(fixture);
  empty.state.scenes = [];
  empty.report.scenes = [];
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "workflow_import") return structuredClone(empty);
    if (command === "save_project_notes") {
      writeFileSync(notesPath, String(args?.text), "utf8");
      return "Notes.md";
    }
    throw new Error(`Unexpected script-save or registration command before Save script: ${command}`);
  });
  vi.stubGlobal("window", { workbench: { invoke } });

  try {
    desk.bindWorkflowDesk({
      root,
      projectRoot: () => projectRoot,
      onOpenPath: vi.fn(),
      onAddToChat: vi.fn(),
      onOpenVoice: vi.fn(),
      onComposeChat: vi.fn(),
      onSelectionChange: vi.fn(),
      chatModel: () => null,
      onStatus: vi.fn(),
    });

    await desk.loadWorkflowDesk(projectRoot);
    const click = (label: string) => {
      const button = [...root.querySelectorAll("button")].find(node => node.textContent === label);
      expect(button, label).toBeDefined();
      button!.click();
    };
    click("Create script");
    const notesField = root.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    const scriptField = root.querySelector<HTMLTextAreaElement>('textarea[name="script"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));
    scriptField.value = script;
    scriptField.dispatchEvent(new Event("input"));

    click("Save notes");
    await vi.waitFor(() => expect(readFileSync(notesPath, "utf8")).toBe(notes));

    expect(scriptField.value).toBe(script);
    expect(existsSync(join(projectRoot, "Scripts"))).toBe(false);
    expect(desk.workflowSelection().sceneId).toBeNull();
    expect(invoke.mock.calls).toEqual([
      ["workflow_import", { projectRoot, persist: true }],
      ["save_project_notes", { projectRoot, text: notes }],
    ]);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});
