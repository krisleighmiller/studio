import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { renderJobsView } from "../src/studio/jobsView";
import { buildJobsModel } from "../src/studio/jobsModel";
import { renderSceneLookView } from "../src/studio/sceneLookView";
import type { SceneLookModel } from "../src/studio/sceneLookModel";
import { renderSurfaceNav } from "../src/studio/surfaceNav";

const root = join(import.meta.dirname, "..");

/** Extracts the full `@media (<query>) { ... }` block (brace-matched) from CSS text. */
function mediaBlock(css: string, query: string): string {
  const start = css.indexOf(`@media (${query})`);
  expect(start, `@media (${query})`).toBeGreaterThanOrEqual(0);
  const open = css.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth++;
    else if (css[i] === "}") {
      depth--;
      if (depth === 0) return css.slice(start, i + 1);
    }
  }
  throw new Error(`Unterminated @media (${query})`);
}

function lookModel(partial: Partial<SceneLookModel> = {}): SceneLookModel {
  return {
    title: "Scene 1 — The Last Train",
    counts: "3 slots · 1 approved",
    warnings: ["Low light"],
    blockers: ["Guard", "Lantern"],
    narrativePurpose: "Leave the station.",
    notes: [],
    slots: [{ id: "s1", role: "interior_reference", title: "Interior", prompt: "Warm", negativePrompt: "", approvalLabel: "Approved" }],
    selectedSlotId: "s1",
    versions: [{ assetId: "a1", path: "Preproduction/Scene1/references/interior_1.png", statusLabel: "approved", approved: true, rejected: false }],
    selectedAssetId: "a1",
    primary: { assetId: "a1", path: "Preproduction/Scene1/references/interior_1.png", statusLabel: "approved", approved: true, rejected: false },
    identityHint: null,
    failedJobs: [{ id: "j1", message: "Look failed" }],
    showPlates: false,
    plateNote: "",
    plates: [],
    canBuildPlates: false,
    emptySlots: false,
    busyReason: null,
    directorAttached: false,
    ...partial,
  } as SceneLookModel;
}

describe("visual hierarchy refactor", () => {
  it("puts the hero image immediately below the header with one blocker summary", () => {
    const view = renderSceneLookView(lookModel(), () => undefined);
    const children = [...view.children].map((el) => el.className);
    const headerIndex = children.findIndex((name) => name.includes("scene-look-header"));
    const blockerIndex = children.findIndex((name) => name.includes("scene-look-blockers"));
    const heroIndex = children.findIndex((name) => name.includes("scene-look-hero"));
    expect(headerIndex).toBe(0);
    expect(blockerIndex).toBeGreaterThan(headerIndex);
    expect(heroIndex).toBeGreaterThanOrEqual(0);
    expect(heroIndex - headerIndex).toBeLessThanOrEqual(2);
    expect(view.querySelectorAll(".scene-look-blockers").length).toBe(1);
    expect(view.querySelectorAll(".scene-look-hero .scene-look-stage").length).toBe(1);
    const hero = view.querySelector(".scene-look-hero");
    const prompt = view.querySelector(".scene-look-disclosure");
    expect(hero && prompt && hero.compareDocumentPosition(prompt) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps jobs closed by default with counts and failure visible", () => {
    const model = buildJobsModel({
      jobs: [
        { id: "a", kind: "render", status: "pending" },
        { id: "b", kind: "scene_look", status: "failed" },
      ],
      scenes: [],
      assets: [],
      filter: "all",
    });
    const closed = renderJobsView(model, false, "all", () => undefined);
    expect(closed.classList.contains("jobs-closed")).toBe(true);
    expect(closed.textContent).toContain("active");
    expect(closed.textContent).toContain("failed");
    expect(closed.querySelector(".workflow-jobs-body")).toBeNull();
    const open = renderJobsView(model, true, "all", () => undefined);
    expect(open.classList.contains("jobs-open")).toBe(true);
    expect(open.querySelector(".workflow-jobs-body")).not.toBeNull();
  });

  it("keeps stage navigation on one line with dots instead of pills", () => {
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    expect(css).toContain(".studio-surface-nav");
    expect(css).toContain("white-space: nowrap");
    expect(css).toContain("overflow-x: auto");
    expect(css).not.toContain(".studio-surface-nav button {\n  display: inline-flex;\n  align-items: center;\n  gap: 6px;\n  padding: 4px 8px;\n  border: 1px solid var(--border);\n  border-radius: 999px;");
    const nav = renderSurfaceNav(
      [
        { id: "script", label: "Script", status: "present", statusLabel: "", active: true, available: true, disabledReason: null },
        { id: "characters", label: "Characters", status: "missing", statusLabel: "", active: false, available: false, disabledReason: "Waiting on Script." },
      ],
      () => undefined,
    );
    expect(nav.querySelectorAll(".studio-surface-dot").length).toBe(2);
    const blocked = [...nav.querySelectorAll("button")].find((b) => b.textContent?.includes("Characters"));
    expect(blocked?.disabled).toBe(true);
    expect(blocked?.title).toBe("Waiting on Script.");
  });

  it("reserves red for failure and keeps blocked amber", () => {
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    expect(css).toContain("--warning:");
    expect(css).toContain("--danger:");
    expect(css).toContain(".workflow-scene.warning");
    expect(css).not.toContain(".workflow-scene.warning {\n  box-shadow: inset 3px 0 0 var(--danger);");
  });

  it("combines Director and Finish into one tabbed inspector", () => {
    const html = readFileSync(join(root, "index.html"), "utf8");
    expect(html).toContain("inspector-tabs");
    expect(html).toContain("inspector-tab-director");
    expect(html).toContain("inspector-tab-finish");
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    expect(css).toContain(".inspector-tab.active");
    expect(css).toContain(".chat-panel.inspector-director");
  });

  it("renders Lite_Challenge Scene 1 hero order when the fixture is present", () => {
    const candidate = "/home/kris/Content_Creation/Projects/Lite_Challenge/Preproduction/workflow_state.json";
    if (!existsSync(candidate)) {
      expect(true).toBe(true);
      return;
    }
    const raw = readFileSync(candidate, "utf8");
    const data = JSON.parse(raw);
    const scene = (data.scenes ?? []).find((item: { id: number }) => item.id === 1) ?? data.scenes?.[0];
    expect(scene).toBeTruthy();
    const view = renderSceneLookView(
      lookModel({
        title: scene.title ?? "Scene 1",
        blockers: [],
        failedJobs: [],
        warnings: [],
      }),
      () => undefined,
    );
    const order = [...view.children].map((el) => el.className);
    expect(order[0]).toContain("scene-look-header");
    expect(order.slice(0, 3).some((name) => name.includes("scene-look-hero"))).toBe(true);
    // Styled fixture for the live browser smoke test. Written to the OS temp
    // dir (never the repo) with the real stylesheet inlined so the fragment
    // renders as styled layout, not an unstyled fragment.
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    const out = join(tmpdir(), "visual-hierarchy-lite-challenge.html");
    writeFileSync(
      out,
      `<!doctype html><meta charset="utf8"><title>Lite_Challenge visual check</title><style>${css}</style><body>${view.outerHTML}</body>`,
    );
  });

  it("keeps the jobs drawer below the workspace at narrow widths", () => {
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    const narrow900 = mediaBlock(css, "max-width: 900px");
    expect(narrow900).toContain('"jobs jobs"');
    expect(narrow900).not.toContain('"jobs main"');
    const narrow720 = mediaBlock(css, "max-width: 720px");
    const scenesAt = narrow720.indexOf('"scenes"');
    const mainAt = narrow720.indexOf('"main"');
    const jobsAt = narrow720.indexOf('"jobs"');
    expect(scenesAt).toBeGreaterThanOrEqual(0);
    expect(mainAt).toBeGreaterThan(scenesAt);
    expect(jobsAt).toBeGreaterThan(mainAt);
  });

  it("lets the finish tab fill the inspector instead of capping its height", () => {
    const css = readFileSync(join(root, "src/styles.css"), "utf8");
    expect(css).not.toMatch(/\.studio-finish-mount\s*\{[^}]*max-height/);
    expect(css).toContain(".chat-panel.inspector-finish .studio-finish-mount");
  });
});
