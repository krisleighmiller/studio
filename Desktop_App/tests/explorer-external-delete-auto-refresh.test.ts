import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_external_delete_auto_refresh", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-07-"));
  const parent = join(root, "Expanded");
  mkdirSync(parent);
  const removed = join(parent, "remove-externally.txt");
  const retained = join(parent, "keep.txt");
  writeFileSync(removed, "disposable");
  writeFileSync(retained, "unrelated");
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
    await vi.waitFor(() => {
      expect(tree.querySelectorAll(`[data-path="${removed}"]`)).toHaveLength(1);
      expect(tree.querySelectorAll(`[data-path="${retained}"]`)).toHaveLength(1);
    });
    const deleter = spawnSync(process.execPath, ["-e", "require('node:fs').unlinkSync(process.argv[1])", removed]);
    expect(deleter.status).toBe(0);
    expect(tree.querySelector(`[data-path="${removed}"]`)).not.toBeNull();
    await vi.waitFor(() => expect(tree.querySelector(`[data-path="${removed}"]`)).toBeNull(), { timeout: 3500 });
    expect(tree.querySelectorAll(`[data-path="${retained}"]`)).toHaveLength(1);
    expect(readFileSync(retained, "utf8")).toBe("unrelated");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 5000);
