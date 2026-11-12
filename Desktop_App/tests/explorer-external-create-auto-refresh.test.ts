import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_external_create_auto_refresh", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-06-"));
  const other = mkdtempSync(join(process.env.TMPDIR!, "w6-06-other-"));
  const parent = join(root, "Expanded");
  mkdirSync(parent);
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const invoke = vi.fn(async (command: string, args: {path: string}) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  try {
    await import("../src/main");
    document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
    const tree = document.querySelector<HTMLElement>("#file-tree")!;
    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${parent}"]`)).not.toBeNull());
    tree.querySelector<HTMLButtonElement>(`[data-path="${parent}"]`)!.click();
    await vi.waitFor(() => expect(invoke.mock.calls.some(([cmd, args]) => cmd === "list_dir" && args.path === parent)).toBe(true));
    const file = join(parent, "created-externally.txt");
    const unrelated = join(other, "not-in-this-project.txt");
    const writer = spawnSync(process.execPath, ["-e", "const fs=require('node:fs'); fs.writeFileSync(process.argv[1], 'external bytes'); fs.writeFileSync(process.argv[2], 'other project')", file, unrelated]);
    expect(writer.status).toBe(0);
    expect(tree.querySelector(`[data-path="${file}"]`)).toBeNull();
    await vi.waitFor(() => expect(tree.querySelectorAll(`[data-path="${file}"]`)).toHaveLength(1), { timeout: 3500 });
    expect(tree.querySelector(`[data-path="${file}"]`)?.textContent).toContain("created-externally.txt");
    expect(tree.querySelector(`[data-path="${unrelated}"]`)).toBeNull();
    expect(invoke.mock.calls.some(([cmd, args]) => cmd === "list_dir" && args.path === other)).toBe(false);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(other, { recursive: true, force: true });
  }
}, 5000);
