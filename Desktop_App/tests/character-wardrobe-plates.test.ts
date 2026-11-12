import { expect, it, vi } from "vitest";
import { renderCharactersStage } from "../src/studio/surfaces/charactersStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";

it("shows each character's plates and sends the explicitly selected scene and wardrobe", () => {
  const character = (id: string) => ({ id, name: id, dir: id, identity_ready: true, voice_ready: false, profile: {} });
  const state = { characters: [character("alice"), character("bob")], scenes: [{ id: 7, title: "Forest", cuts: [], stages: {}, look_board: { slots: [{ id: "w", role: "wardrobe_reference", title: "Coat", approved_asset_id: "wardrobe" }] } }], assets: [
    { id: "wardrobe", path: "coat.png", role: "wardrobe_reference", status: "approved", scene_id: 7 },
    { id: "alice-plate", path: "alice.png", role: "wardrobe_plate", character_id: "alice", scene_id: 7 },
    { id: "bob-plate", path: "bob.png", role: "wardrobe_plate", character_id: "bob", scene_id: 7 },
  ] };
  bindSurfaceHost({ state, busy: null, busyControl: null, report: null, rerender: vi.fn(), resolvePath: (p: string) => p, mediaUrl: async (p: string) => p } as unknown as SurfaceHost);
  const emit = vi.fn();
  const root = renderCharactersStage(emit);
  const sections = root.querySelectorAll<HTMLElement>("[data-wardrobe-character]");
  expect(sections).toHaveLength(2);
  expect(sections[0].querySelectorAll("figure")[0].textContent).toContain("alice.png");
  expect([...sections[0].querySelectorAll("figure")].map(f => f.textContent).join()).not.toContain("bob.png");
  const select = sections[0].querySelector<HTMLSelectElement>('select[aria-label="Approved wardrobe for alice"]')!;
  expect(select.value).toBe("");
  select.value = "7|wardrobe";
  select.dispatchEvent(new Event("change"));
  [...sections[0].querySelectorAll("button")].find(b => b.textContent === "Build wardrobe plates")!.click();
  expect(emit).toHaveBeenCalledWith({ type: "build-wardrobe-plates", characterKey: "alice", sceneId: 7, wardrobeAssetId: "wardrobe" });
  resetSurfaceSession();
});
