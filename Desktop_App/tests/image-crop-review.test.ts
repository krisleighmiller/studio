import { afterEach, expect, it, vi } from "vitest";
import { renderSceneLookView } from "../src/studio/sceneLookView";
import { reviewImageCrop } from "../src/studio/imageCropReview";
import type { SceneLookModel } from "../src/studio/sceneLookModel";

afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks(); });
it("shows a before/after crop, emits only the selected local edit, and never auto-approves", async () => {
  const intents: unknown[] = [];
  const source = { assetId: "approved-source", path: "/project/source.png", statusLabel: "Approved", approved: true, rejected: false };
  const model: SceneLookModel = { title: "Look", counts: "", warnings: [], blockers: [], narrativePurpose: "", notes: [], affectedCuts: [],
    slots: [{ id: "slot", role: "interior_reference", title: "Room", prompt: "", negativePrompt: "", approvalLabel: "Approved" }],
    selectedSlotId: "slot", versions: [source], selectedAssetId: source.assetId, primary: source, identityHint: null,
    failedJobs: [], showPlates: false, plateNote: "", plates: [], canBuildPlates: false, directorAttached: false, busyReason: null, emptySlots: false };
  const view = renderSceneLookView(model, intent => intents.push(intent));
  [...view.querySelectorAll("button")].find(b => b.textContent === "Crop locally")!.click();
  expect(intents).toEqual([{ type: "crop", slotId: "slot", assetId: "approved-source", path: "/project/source.png" }]);
  vi.spyOn(HTMLImageElement.prototype, "decode").mockImplementation(async function(this: HTMLImageElement) {
    Object.defineProperty(this, "naturalWidth", { configurable: true, value: 100 });
    Object.defineProperty(this, "naturalHeight", { configurable: true, value: 80 });
  });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ drawImage: vi.fn() } as unknown as CanvasRenderingContext2D);
  const pending = reviewImageCrop("data:image/png;base64,fixture");
  await vi.waitFor(() => expect(document.querySelector(".image-crop-review")).not.toBeNull());
  const dialog = document.querySelector<HTMLDialogElement>(".image-crop-review")!;
  expect(dialog.textContent).toContain("Before (100 × 80)");
  expect(dialog.textContent).toContain("After (80 × 64)");
  const fields = dialog.querySelectorAll<HTMLInputElement>('input[type="number"]');
  fields[0].value = "20"; fields[0].dispatchEvent(new Event("input", { bubbles: true }));
  expect(dialog.textContent).toContain("After (70 × 64)");
  [...dialog.querySelectorAll("button")].find(b => b.textContent === "Save crop as draft")!.click();
  expect(await pending).toEqual({ x: 20, y: 8, width: 70, height: 64 });
  expect(document.querySelector(".image-crop-review")).toBeNull();
  expect(intents).toHaveLength(1);
});
