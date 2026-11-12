import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); localStorage.clear(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("notes_save_refreshes_actual_explorer", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const projectRoot = mkdtempSync(join(process.env.TMPDIR!, "w3-w6-notes-"));
  const notesPath = join(projectRoot, "Notes.md");
  const notes = "The actual Files tree must show this saved note.\n";
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  fixture.state.scenes = [];
  fixture.report.scenes = [];
  const invoke = vi.fn(async (command: string, args: any) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "save_project_notes") {
      writeFileSync(notesPath, args.text, "utf8");
      return "Notes.md";
    }
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => projectRoot, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  try {
    await import("../src/main");
    document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
    const tree = document.querySelector<HTMLElement>("#file-tree")!;
    await vi.waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "workflow_import")).toBe(true));
    expect(tree.querySelector(`[data-path="${notesPath}"]`)).toBeNull();

    const workflow = document.querySelector<HTMLElement>("#workflow-view")!;
    await vi.waitFor(() => expect([...workflow.querySelectorAll("button")].some(button => button.textContent === "Create script")).toBe(true));
    [...workflow.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "Create script")!.click();
    const notesField = workflow.querySelector<HTMLTextAreaElement>('textarea[name="notes"]')!;
    notesField.value = notes;
    notesField.dispatchEvent(new Event("input"));
    [...workflow.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "Save notes")!.click();

    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${notesPath}"]`)).not.toBeNull());
    document.querySelector<HTMLButtonElement>('.nav-item[data-surface="files"]')!.click();
    expect(document.querySelector("#app")?.classList.contains("surface-files")).toBe(true);
    expect(tree.querySelector(`[data-path="${notesPath}"]`)?.textContent).toContain("Notes.md");
    expect(readFileSync(notesPath, "utf8")).toBe(notes);
  } finally {
    rmSync(projectRoot, { recursive: true, force: true });
  }
});
