import type { AnimaticShot, CutRecord } from "../../workflowTypes";
import type { SketchIntent } from "./surfaceIntents";
import { getSurfaceHost } from "./session";
import { actionButton, block } from "./shared";

/** Read-only timing comparison. Metadata comes from the selected local files, not a render request. */
export function renderAnimaticTiming(shot: AnimaticShot, cut: CutRecord | undefined, emit: (intent: SketchIntent) => void): HTMLElement {
  const section = document.createElement("div");
  section.className = "workflow-animatic-timing";
  const host = getSurfaceHost();
  const lines = cut?.dialogue?.length ? cut.dialogue : cut?.sketch?.dialogue ?? [];
  const words = lines.flatMap(line => line.text.split(/\s+/u).filter(word => /[\p{L}\p{N}]/u.test(word))).length;
  section.append(block("workflow-field-hint", words
    ? `Script speech estimate: ${(words * 60 / 165).toFixed(2)}s at 165 words/minute, not a measured performance. Review cut/sketch and production-note dialogue if they differ.`
    : "Script speech estimate: no cut dialogue available. Check the script and production note before retiming."));
  const provenance = shot.auto_duration_seed_seconds != null && Math.abs(shot.auto_duration_seed_seconds - shot.duration_seconds) < 0.001
    ? "auto seed (untouched)" : "origin unknown, not evidence of a manual choice (or explicitly revised)";
  section.append(block("workflow-field-hint", `Editorial screen duration: ${shot.duration_seconds.toFixed(2)}s · ${provenance}. This is independent of speech and rendered source length.`));
  const status = block("workflow-field-hint", "Selected rendered-source limit: loading media metadata… Measured WAV: not established.");
  section.append(status);
  const selected = cut?.takes?.find(take => take.id === cut.selected_take || take.path === cut.selected_take);
  const video = document.createElement("video"); video.preload = "metadata"; video.dataset.timingSource = ""; video.hidden = true;
  const performance = (host.state?.assets ?? []).find(asset => asset.id === cut?.selected_performance_asset_id && /\.wav$/i.test(asset.path));
  // A legacy rendered take may have an unregistered sibling WAV. Probe it for
  // comparison only; same-name bytes do not prove complete dialogue or sync.
  const wavPath = performance?.path ?? (selected?.path.replace(/\.mp4$/i, ".wav") !== selected?.path
    ? selected?.path.replace(/\.mp4$/i, ".wav") : undefined);
  const audio = document.createElement("audio"); audio.preload = "metadata"; audio.dataset.timingPerformance = ""; audio.hidden = true;
  section.append(video, audio);
  let source: number | null = null;
  let measured: number | null = null;
  const refresh = () => {
    const existing = section.querySelector("button[data-bounded-edit]"); existing?.remove();
    const head = shot.trim_in_seconds ?? 0;
    const tail = shot.trim_out_seconds ?? 0;
    const window = shot.duration_seconds - head - tail;
    const limit = source == null ? null : Math.floor((source + 1e-7) * 100) / 100;
    status.textContent = `Measured WAV: ${measured == null ? "unavailable (not inferred from text or video)" : `${measured.toFixed(2)}s (${performance ? "selected WAV" : "matching-name WAV, linkage unverified"}; not proof of the spoken endpoint)`}. Selected rendered-source limit: ${source == null ? "unavailable" : `${source.toFixed(2)}s raw; requested window starts ${head.toFixed(2)}s and runs ${window.toFixed(2)}s`}.`;
    if (source == null || limit == null || limit < 0.4) return;
    if (window <= 0 || head >= source || head + window > source + 1e-7) {
      status.textContent += ` Editorial duration exceeds the selected source. Assembly will refuse this window. No timing has changed.`;
      if (head !== 0 || tail !== 0) {
        status.textContent += " Review the custom trims manually; no automatic edit is offered.";
        return;
      }
      const button = actionButton(`Use bounded ${limit.toFixed(2)}s screen duration`, () => emit({type:"set-duration",shotId:shot.id,durationSeconds:limit}));
      button.dataset.boundedEdit = "";
      button.disabled = Boolean(host.busy);
      section.append(button);
    } else if (measured != null && Math.abs(shot.duration_seconds - measured) > 0.5) {
      status.textContent += " Speech and editorial timing differ; review intentional lead, after-line action and cut dialogue before editing.";
    }
  };
  refresh();
  if (selected) {
    video.addEventListener("loadedmetadata", () => { if (Number.isFinite(video.duration) && video.duration > 0) { source = video.duration; refresh(); } });
    video.addEventListener("error", () => { status.textContent = "Selected rendered-source duration could not be read. No bounded edit offered."; });
    void host.mediaUrl(host.resolvePath(selected.path) ?? selected.path).then(url => { if (section.isConnected && getSurfaceHost().state === host.state) video.src = url; }).catch(() => { if (section.isConnected) status.textContent = "Selected rendered-source duration could not be read. No bounded edit offered."; });
  }
  if (wavPath) {
    audio.addEventListener("loadedmetadata", () => { if (Number.isFinite(audio.duration) && audio.duration > 0) { measured = audio.duration; refresh(); } });
    void host.mediaUrl(host.resolvePath(wavPath) ?? wavPath).then(url => { if (section.isConnected && getSurfaceHost().state === host.state) audio.src = url; }).catch(() => undefined);
  }
  return section;
}
