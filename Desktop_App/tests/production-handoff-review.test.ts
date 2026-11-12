import { afterEach, expect, it } from "vitest";
import { reviewProductionHandoff, type HandoffPreview } from "../src/studio/surfaces/productionHandoffReview";
import { renderProductionNoteCard } from "../src/studio/surfaces/notesStage";
import { bindSurfaceHost } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { CutRecord } from "../src/workflowTypes";

const preview: HandoffPreview = {
  path: "Scene_Prompts/Scene3/Cut1.txt",
  oldCues: [{ speaker: "MARA", text: "An old long line" }],
  newCues: [{ speaker: "MARA", text: "Short." }],
  oldTiming: { speech_duration_seconds: 4.2, render_duration_seconds: 6 },
  newTiming: { speech_duration_seconds: 1, render_duration_seconds: 3 },
  oldScreenSeconds: 5, newScreenSeconds: 2,
  oldReferences: [{ role: "start_frame", path: "old.png" }],
  newReferences: [{ role: "start_frame", path: "new.png" }],
  prompt: "Keep the authored framing", negativePrompt: "No extra faces", changed: true, revision: "expected",
};
afterEach(() => document.body.replaceChildren());
it("shows an explicit per-cut refresh action and compares cues, timing, references and preserved prompt before applying", async () => {
  bindSurfaceHost({ busy: null, resolvePath: () => null, hooks: null } as unknown as SurfaceHost);
  const cut = { id: { source_scene: 3, cut: 1 }, sketch: {}, production_note_path: preview.path, takes: [] } as unknown as CutRecord;
  const emitted: unknown[] = [];
  const card = renderProductionNoteCard(cut, intent => emitted.push(intent));
  const review = [...card.querySelectorAll("button")].find(button => button.textContent?.includes("Review planning"))!;
  expect(review.disabled).toBe(false);
  review.click();
  expect(emitted).toContainEqual({ type: "review-handoff", cut: 1 });
  const pending = reviewProductionHandoff(preview, 3, 1);
  const dialog = document.querySelector<HTMLDialogElement>("dialog[data-production-handoff-review]")!;
  expect(dialog.textContent).toContain("An old long line");
  expect(dialog.textContent).toContain("Short.");
  expect(dialog.textContent).toContain("4.2 / 5 / 6 s");
  expect(dialog.textContent).toContain("1 / 2 / 3 s");
  expect(dialog.textContent).toContain("old.png");
  expect(dialog.textContent).toContain("new.png");
  expect(dialog.textContent).toContain("Keep the authored framing");
  dialog.querySelectorAll("button")[0].click();
  expect(await pending).toBe(false);
  const accepted = reviewProductionHandoff(preview, 3, 1);
  document.querySelector<HTMLDialogElement>("dialog[data-production-handoff-review]")!.querySelectorAll("button")[1].click();
  expect(await accepted).toBe(true);
});
