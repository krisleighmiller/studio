import { describe, expect, it } from "vitest";
import { audioSections, buildAudioStatus, describeAudioStatus, parseAudioRoles } from "../src/studio/selectors/audio";
import type { AssetRecord, ProjectState, SceneRecord } from "../src/workflowTypes";

function baseState(): ProjectState {
  return {
    version: 3,
    project_name: "x",
    characters: [],
    scenes: [],
    assets: [],
    jobs: [],
    requirements: [],
    notes: [],
  };
}

function scene(id: number, dialogue: boolean): SceneRecord {
  const cuts = dialogue
    ? [{ dialogue: [{ speaker: "A", text: "hello" }] }]
    : [];
  return { id, cuts } as unknown as SceneRecord;
}

function asset(id: string, role: string, sceneId: number, status: string): AssetRecord {
  return { id, path: `${id}.mp3`, role, status, scene_id: sceneId } as AssetRecord;
}

function withProfile(state: ProjectState, audio_plan: string): ProjectState {
  state.profile = { audio_plan } as unknown as ProjectState["profile"];
  return state;
}

describe("parseAudioRoles", () => {
  it("finds role keywords in a plan", () => {
    const roles = parseAudioRoles("a quiet room tone, light ambience, a music cue, and foley");
    expect(roles).toEqual(expect.arrayContaining(["room_tone", "ambience", "music", "foley"]));
    expect(roles).not.toContain("dialogue");
  });

  it("returns empty for an unrelated plan", () => {
    expect(parseAudioRoles("establishing exterior at dusk")).toEqual([]);
  });
});

describe("buildAudioStatus", () => {
  it("reports required, present, and missing", () => {
    const state = withProfile(baseState(), "music and ambience");
    state.scenes = [scene(1, false)];
    state.assets = [asset("m", "music", 1, "approved")];
    const status = buildAudioStatus(state, 1)!;
    expect(status.required).toEqual(["ambience", "music"]);
    expect(status.missing).toEqual(["ambience"]);
    expect(status.blocksFinish).toBe(true);
  });

  it("derives dialogue from cuts", () => {
    const state = baseState();
    state.scenes = [scene(1, true)];
    const status = buildAudioStatus(state, 1)!;
    expect(status.required).toEqual(["dialogue"]);
    expect(status.missing).toEqual(["dialogue"]);
    expect(status.blocksFinish).toBe(true);
  });

  it("passes when required audio is approved", () => {
    const state = withProfile(baseState(), "music");
    state.scenes = [scene(1, false)];
    state.assets = [asset("m", "music", 1, "approved")];
    const status = buildAudioStatus(state, 1)!;
    expect(status.missing).toEqual([]);
    expect(status.allApproved).toBe(true);
    expect(status.blocksFinish).toBe(false);
  });

  it("blocks on unapproved audio", () => {
    const state = withProfile(baseState(), "music");
    state.scenes = [scene(1, false)];
    state.assets = [asset("m", "music", 1, "draft")];
    const status = buildAudioStatus(state, 1)!;
    expect(status.missing).toEqual([]);
    expect(status.allApproved).toBe(false);
    expect(status.blocksFinish).toBe(true);
  });

  it("returns null for an unknown scene", () => {
    expect(buildAudioStatus(baseState(), 99)).toBeNull();
  });
});

describe("describeAudioStatus", () => {
  it("is null when nothing is required or present", () => {
    const state = baseState();
    state.scenes = [scene(1, false)];
    expect(describeAudioStatus(buildAudioStatus(state, 1)!)).toBeNull();
  });

  it("summarizes missing and unapproved roles", () => {
    const state = withProfile(baseState(), "music and ambience");
    state.scenes = [scene(1, true)];
    state.assets = [asset("m", "music", 1, "draft")];
    const status = buildAudioStatus(state, 1)!;
    expect(describeAudioStatus(status)).toBe(
      "Audio: missing Ambience, Dialogue; unapproved Music",
    );
  });

  it("reports complete when required audio is approved", () => {
    const state = withProfile(baseState(), "music");
    state.scenes = [scene(1, false)];
    state.assets = [asset("m", "music", 1, "approved")];
    expect(describeAudioStatus(buildAudioStatus(state, 1)!)).toBe("Audio complete");
  });
});

describe("audioSections", () => {
  it("groups dialogue, music, ambience, and effects", () => {
    const state = withProfile(baseState(), "music, ambience, and foley");
    state.scenes = [scene(1, true)]; // dialogue required via a cut
    state.assets = [asset("m", "music", 1, "draft")];
    const sections = audioSections(buildAudioStatus(state, 1)!);
    const labels = sections.map((section) => section.label);
    expect(labels).toEqual(["Dialogue", "Music", "Ambience", "Effects"]);

    const dialogue = sections.find((section) => section.id === "dialogue")!;
    expect(dialogue.entries).toHaveLength(1);
    expect(dialogue.entries[0]).toMatchObject({
      label: "Dialogue",
      present: false,
      required: true,
    });

    const music = sections.find((section) => section.id === "music")!;
    expect(music.entries[0]).toMatchObject({ present: true, approved: false, required: true });

    const effects = sections.find((section) => section.id === "effects")!;
    expect(effects.entries).toHaveLength(1);
    expect(effects.entries[0]).toMatchObject({ label: "Foley", present: false, required: true });
  });

  it("omits sections with no required or present roles", () => {
    const state = baseState();
    state.scenes = [scene(1, false)];
    expect(audioSections(buildAudioStatus(state, 1)!)).toEqual([]);
  });
});