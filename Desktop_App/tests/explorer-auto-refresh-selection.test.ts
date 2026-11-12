import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_auto_refresh_selection", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-11-"));
  const parent = join(root, "Expanded");
  mkdirSync(parent);
  const selected = join(parent, "original.txt");
  const added = join(parent, "created-externally.txt");
  writeFileSync(selected, "original bytes");
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const invoke = vi.fn(async (command: string, args: {path: string}) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "classify_file") return "text";
    if (command === "read_text_file") return readFileSync(args.path, "utf8");
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
    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${selected}"]`)).not.toBeNull());
    tree.querySelector<HTMLButtonElement>(`[data-path="${selected}"]`)!.click();
    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${selected}"].selected`)).not.toBeNull());
    expect(tree.querySelector(`[data-path="${added}"]`)).toBeNull();
    const beforeRefresh = tree.querySelector(`[data-path="${selected}"]`);
    const writer = spawnSync(process.execPath, ["-e", "require('node:fs').writeFileSync(process.argv[1], 'external bytes')", added]);
    expect(writer.status).toBe(0);
    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${added}"]`)).not.toBeNull(), { timeout: 3500 });
    expect(tree.querySelector(`[data-path="${selected}"]`)).not.toBe(beforeRefresh);
    expect(Array.from(tree.querySelectorAll(".tree-item.selected"), row => row.getAttribute("data-path"))).toEqual([selected]);
    expect(tree.querySelector(`[data-path="${added}"].selected`)).toBeNull();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 5000);
