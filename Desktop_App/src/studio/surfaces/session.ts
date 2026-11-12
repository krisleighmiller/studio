import type { CharacterProfile } from "../../workflowTypes";
import { clearIntentDraft } from "../sceneIntentView";
import type { SurfaceHost } from "./host";
import type { CharacterReferenceImage } from "./surfaceIntents";

/**
 * Phase 6 surface session: the single owner of mutable surface state.
 *
 * Selectors (surfaceSelectors.ts) stay pure. Components (stage modules) read
 * project data through getSurfaceHost() and keep no module-level project
 * state of their own; transient UI state (drafts, toggles, rough-cut
 * playback) lives here behind small accessors. Effects still flow out as
 * intents handled by workflowDesk.
 */
let host: SurfaceHost;

export const startFrameEditBriefs = new Map<string, string>();
export const characterDrafts = new Map<string, { name: string; profile: CharacterProfile; references?: CharacterReferenceImage[]; savedCharacterId?: string; referenceReselectionPaths?: string[] }>();
let newCharacterOpen = false;
let profileOpen = false;

export type RoughPlaybackState = {
  sceneId: number;
  index: number;
  timer: number;
  audio: HTMLAudioElement | null;
};
let roughPlayback: RoughPlaybackState | null = null;
let performanceAudio: HTMLAudioElement | null = null;

export function auditionPerformance(audio: HTMLAudioElement): void {
  if (performanceAudio !== audio) performanceAudio?.pause();
  performanceAudio = audio;
}
export function stopPerformanceAudio(): void {
  performanceAudio?.pause();
  performanceAudio = null;
}

export function bindSurfaceHost(next: SurfaceHost): void {
  host = next;
}

export function getSurfaceHost(): SurfaceHost {
  return host;
}

export function resetSurfaceSession(): void {
  clearIntentDraft();
  startFrameEditBriefs.clear();
  characterDrafts.clear();
  newCharacterOpen = false;
  profileOpen = false;
  stopRoughCut();
  stopPerformanceAudio();
}

export function setProfileOpen(open: boolean): void {
  profileOpen = open;
}

export function isProfileOpen(): boolean {
  return profileOpen;
}

export function noteCharacterSaved(characterId: string | null): void {
  characterDrafts.delete(characterId ?? "__new__");
  if (!characterId) newCharacterOpen = false;
}

export function isNewCharacterOpen(): boolean {
  return newCharacterOpen;
}

export function setNewCharacterOpen(open: boolean): void {
  newCharacterOpen = open;
}

export function getRoughPlayback(): RoughPlaybackState | null {
  return roughPlayback;
}

export function setRoughPlayback(next: RoughPlaybackState | null): void {
  roughPlayback = next;
}

export function stopRoughCut(): void {
  if (!roughPlayback) return;
  window.clearTimeout(roughPlayback.timer);
  roughPlayback.audio?.pause();
  roughPlayback = null;
}
