import type { AnimaticShot, AssemblyShotSnapshot, CutRecord, SceneRecord } from "../../workflowTypes";

/** Immediate review UI parity check; the host still validates approval authoritatively. */
export function renderedSnapshotCurrent(scenes: SceneRecord[], saved: AssemblyShotSnapshot[] | null | undefined): boolean {
  if (!saved?.length || !scenes.length) return false;
  const ordered = [...scenes].sort((a, b) => a.id - b.id).flatMap((scene): Array<{ shot: AnimaticShot | null; cut: CutRecord | undefined }> => {
    const shots = scene.animatic?.shots;
    if (!scene.cuts.length || (shots?.length && shots.length !== scene.cuts.length)) return [];
    if (new Set(scene.cuts.map(cut => `${cut.id.source_scene}:${cut.id.cut}`)).size !== scene.cuts.length) return [];
    if (shots?.length && new Set(shots.map(shot => `${shot.cut_id.source_scene}:${shot.cut_id.cut}`)).size !== shots.length) return [];
    const cuts = scene.animatic?.shots?.length
      ? scene.animatic.shots.map(shot => ({ shot, cut: scene.cuts.find(cut => cut.id.source_scene === shot.cut_id.source_scene && cut.id.cut === shot.cut_id.cut) }))
      : [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut).map(cut => ({ shot: null, cut }));
    return cuts.length === scene.cuts.length ? cuts : [];
  });
  return ordered.length === saved.length && ordered.every(({ shot, cut }, i) => {
    if (!cut) return false;
    const take = cut.takes?.find(take => take.id === cut.selected_take || take.path === cut.selected_take);
    const record = saved[i];
    if (!take || !record || record.shot_id !== cut.stable_id || record.cut_label !== cut.id.cut || record.take_id !== take.id || record.path !== take.path) return false;
    if (!shot) return record.start_seconds === 0 && record.transition === "cut";
    const head = shot.trim_in_seconds ?? 0;
    const duration = shot.duration_seconds - head - (shot.trim_out_seconds ?? 0);
    return Math.abs(record.start_seconds - head) < 0.000001 && Math.abs(record.duration_seconds - duration) < 0.000001 && record.transition === (shot.transition ?? "cut") && (!shot.trim_source_take_id || shot.trim_source_take_id === take.id);
  });
}
