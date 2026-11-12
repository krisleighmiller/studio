import { sceneHeadingTitle } from "../selectors/scenes";

/** Local authoring only: a draft is editable text, not generated dialogue. */
export function buildScriptDraft(notes: string, title: string): string | { validationError: string } {
  if (!notes.trim()) {
    return { validationError: "Add notes or script text before building a draft." };
  }

  // Match host/src/script.rs::heading_re, line-by-line, including its u32 IDs.
  // Already formatted scripts must retain their original headings and bytes.
  const hasScenes = notes.split("\n").some((line) => {
    const heading = /^\s{0,3}#{0,6}\s*scene\s+([0-9]+)\b.*$/i.exec(line.trimEnd());
    return heading !== null && Number(heading[1]) <= 0xffff_ffff;
  });
  if (hasScenes) return notes;

  const singleLineTitle = title.replace(/\s+/g, " ").trim();
  return `${sceneHeadingTitle(1, singleLineTitle)}\n\n${notes}`;
}
