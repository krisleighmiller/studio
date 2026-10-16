# UI_Redo.jpg — VISUAL ACCEPTANCE SPEC

Source mock: `/home/kris/Content_Creation/Studio/Dev_Notes/V2/UI_Redo.jpg` — 2816×1536 px (16:9), hand-drawn pencil+marker wireframe on warm-white paper. All coordinates below are fractions of canvas size: x 0=left, y 0=top. Colors are measured pixel means, given as approximations. Method note: the vision endpoint could not return reliable image reads, so every coordinate/color below comes from multi-pass tesseract OCR with bounding boxes plus pixel geometry/color probes; where the pencil text is unrecoverable it is explicitly flagged rather than guessed.

## 1. Overall layout geometry — five columns

- **Nav rail** — x 0.000–0.055, full height. Warm-gray fill (242,237,232), no visible border. Top: circular logo glyph at (0.024, 0.118), ~0.02 wide. Below, a vertical icon+label stack, pitch ≈0.095, labels ~11px: Home (0.024,0.174), Studio (0.024,0.269 — ACTIVE, solid dark-green icon block (49,86,72)), Files (0.024,0.365), [unreadable glyph (0.020,0.428)], Assets (0.024,0.463), [unreadable glyph (0.019,0.518)], Voices (0.024,0.559). Lower rail empty.
- **Top bar** — y 0.000–0.095, spans full width, two text lines, same paper background (no chrome strip, no divider).
  - Line 1 (y 0.010–0.030): project title "Production Studio — My Project" left, x 0.027–0.173, ~18px; right side: "Jobs" (0.860–0.883, y0.013) and a settings-gear icon (0.961–0.970, y 0.010–0.026, dark ink (28,27,38)).
  - Line 2 (y 0.067–0.090): "≡" collapse icon at (0.020,0.081) (sits over the rail column), then workspace dropdown "Studio ▾" (0.081–0.135, y0.080, ~17px with caret), then a larger word read as "93"/"Plan" at (0.175–0.208, y0.073) with glyph box 42–76px tall — ambiguous (tab? heading? count?). See §8.
- **Left panel** — x 0.055–0.28, full height below top bar. Contains, top→bottom: "Scenes" header (0.082,0.212, ~17px); a 6-row scene list, y 0.24–0.60, pitch ≈0.065; uppercase micro-label "PLAN" (0.170–0.197, 0.620, ~12px caps) near the bottom-center of the upper section; a full-width horizontal divider at y ≈0.668; then the Jobs zone: "Jobs" header (0.076,0.762) + 4 job rows y 0.82–0.98 (§3).
- **Center workspace (Scene Look)** — x 0.28–0.66 for main content + alternates strip x 0.662–0.756; y 0.10–1.0. Warm-white panel (249,245,233).
- **Right panel** — x 0.757–1.000: Director zone top (header at 0.760,0.212), full-width horizontal divider y ≈0.672, uppercase micro-label "FINISH" (0.853–0.890, 0.620) just above the divider (mirrors "PLAN"), Finish rows y 0.71–0.93 (§5).

The two side panels are each split into an upper workflow section and lower utility section by one full-width divider at the same height (y≈0.67), and each carries its stage label ("PLAN" left, "FINISH" right) at the bottom of its upper section.

## 2. Scene Look composition

- **Header row (y≈0.198–0.240)**: leftmost small word at (0.289,0.215), 19px box, read by three OCR passes as "Details" (§8); immediately right, chip tokens "Scene" (0.344) + "Cemetery" (0.358–0.397) — i.e. a scene chip "Scene 4 · Cemetery"; word "Inventory" at (0.448,0.212) (possible second tab); far right an **approval pill**: check glyph + "Approved" (0.603–0.642, 0.198–0.240), olive/warm text ink (141,133,59) on light pill fill (240,232,216). Approval state therefore sits header-right, same baseline as the section header.
- **Primary image**: the single largest element — dark block x 0.300–0.642 × y 0.358–0.721 (≈34% canvas width × 36% height; ≈78% of center-panel width; ~1.65:1 landscape). Interior content mostly dark navy-gray (37,50,58); a light vertical streak at x 0.338–0.356 is image content, not UI.
- **In-image overlay**: small light "LOCATION ▾" label at the image's top-left corner (0.316–0.435, 0.365–0.393) — an overlay dropdown on the artifact itself.
- **Caption block (below image, on light panel)**: large title "Cemetery entrance" (0.297–0.420, y 0.730–0.770, glyph 36–44px ≈ 24–28px type — the largest legible text in the mock); below it label "Narrative purpose" (0.311–0.362, 0.847) followed by two-to-three pencil squiggle lines (to x≈0.42, y 0.85–0.885) standing in for purpose/body copy. Center-left column below y≈0.90 is empty.
- **Alternates**: presented as a **vertical strip** (not a grid, not a horizontal row) at the workspace's right edge, x 0.662–0.756. Header "Alternates" (0.666–0.733, 0.325). Four square-ish thumbnails (~0.09 wide) stacked from y≈0.36 to ≈0.68, each followed by a caption line and a dark action bar:
  - T1 caption (0.662–0.740, 0.494) — "Alternate — rain, silhouette, no face" (partially legible); action bar (0.663–0.754, 0.505–0.529).
  - T2 caption (0.663–0.742, 0.545) — "Alternate — empty track, wide"; bar (0.663–0.745, 0.558–0.581).
  - T3 caption (0.662–0.768, 0.608) — "The Platform at night"; bar (0.663–0.752, 0.622–0.646).
  - Bar 4 (0.661–0.760, 0.685–0.707) below a fourth/final thumbnail area.
  - A light glyph cluster "4 ▸▸/›" overlays T1's top edge (0.678–0.755, 0.386) — possible pager/scroll control; ambiguous (§8).
- **Action buttons**: the bars are solid dark-green pills (45,82,67) with a lighter green interior band (149,178,133); the light button text inside them is pencil-thin and **not OCR-recoverable**. The redesign must therefore define per-alternate button labels (candidates from the plan doc: Use / Regenerate / Reject / Approve) — the mock only fixes their placement: one full-width dark action bar directly under each alternate caption.
- **Scene context**: scene identity lives in the header chip ("Scene 4 · Cemetery"), the approved pill, the image caption, and the Narrative-purpose placeholder; there is no separate dependencies field visible in the mock (§8).

## 3. Jobs presentation

- Location: left panel bottom (below the y≈0.668 divider); a second "Jobs" entry sits top-bar right (0.860,0.013) as a shortcut/link.
- Density: 4 rows, pitch ≈0.048 (~74 px/row) — the densest region of the mock after the scene list.
- Row anatomy (measured): `[icon circle x0.076–0.088, gray (47,49,49)] [name, title-case, from x0.092] [status word at x≈0.13–0.15] [elapsed "(2m)"/"(5m)" right-aligned at x≈0.18–0.20]`.
  - Row 1 (0.824): icon, "Render:", "Complete", "(2m)".
  - Row 2 (0.872): icon (reads "»"), "Asset Sync", "Pending", no time.
  - Row 3 (0.920): icon, "Sound Mix:", "Complete", "(5m)".
  - Row 4 (0.967): duplicate of row 3 — mock artifact.
- Status is conveyed by the words Complete/Pending and the row icons; no saturated status colors were detected in the Jobs zone (§7/§8).

## 4. Director presentation

- Right panel top (x 0.757–1.0, y 0.10–0.66). Header "Director" (0.760,0.212, ~24px).
- Scope line under header: "Scene 4: Scene Look" (tokens at 0.792–0.893, y 0.287–0.299) — states which artifact/context the Director is bound to.
- Context entries: four stacked chip rows, y ≈0.365–0.50, pitch ≈0.038, light rounded chips with a left icon circle at x0.774–0.782 and label from x0.797: "Scene", "Mom character", "Cemetery", "Current look". This is the attached-context list (§8 for exact chip styling).
- Chat area: **no message history is drawn** — the mock shows a single input box: rounded field x 0.771–0.973 × y 0.533–0.582, slightly darker fill (237,233,224), placeholder text "Ask the Director..." (0.801–0.886, 0.543–0.554). Region below (y 0.58–0.66) is empty.

## 5. Finish presentation

- Right panel bottom (y 0.672–1.0), preceded by the "FINISH" micro-label above the divider. Checklist-style rows: `[check circle icon x0.830–0.833] [two-word action label] [right-edge status word]`.
  - Row y≈0.718: check circle, label illegible (§8).
  - Row y≈0.794: "Export finished" + "video" (0.847–0.932, 0.788–0.802).
  - Row y≈0.856–0.866: "Sync shot to" + "Editorial" (0.847–0.960, 0.852–0.866) + trailing status "Ready" (0.963–0.972, 0.856).
  - A 4th check-circle at y≈0.908 with no legible label.
- No export buttons/badges beyond the check circles and the "Ready" word.

## 6. Typography & density

- Scale ladder (measured glyph-box heights): hero caption "Cemetery entrance" ~36–44px (24–28px type) > ambiguous "93/Plan" 42–76px > section headers (Scenes, Director, Jobs, Alternates, pill text) ~21–31px boxes (≈14–21px type) > scope/chip/job text ~22–33px boxes (≈11–14px type) > rail labels, squiggles, "PLAN"/"FINISH"/"LOCATION" micro-caps ~12px.
- Uppercase micro-labels DO exist: "PLAN" (0.620), "FINISH" (0.620), and the in-image "LOCATION ▾". Everything else is title-case; labels and values are not visually differentiated beyond size (no color-coded label/value styling).
- All text left-aligned; trailing columns (status dots, times, "Ready", gear) right-aligned at fixed x (≈0.20–0.25 in left panel, ≈0.96–0.97 right panel).
- Density is high in the two lists (scene pitch 0.065, jobs pitch 0.048) and airy in the Director lower area, center-bottom-left, and rail-bottom.

## 7. Color system

- **Light theme** — warm paper backgrounds: page (250,246,237), panels (249,245,233), rail (242,237,232), input fill (237,233,224), pill fill (240,232,216). No dark chrome.
- Accent = **green**: active nav icon (49,86,72), alternate action bars (45,82,67) with light-green interior (149,178,133), approved-pill olive (141,133,59). Art blocks are teal/near-black: primary image (37,50,58); alternates T1 (55,71,71), T2 (24,23,19), T3 (102,125,139).
- UI ink/icons: gray (47,49,49); gear near-black (28,27,38).
- **Status coding is textual** (Complete / Pending / Ready / Approved + green checks), not color-coded; no amber/red/blue detected anywhere.

## 8. Prominence hierarchy & flagged ambiguities

Prominent: the primary image block (largest area, ~12% of canvas, dark against paper) >> the saturated green action bars and dark alternate thumbnails >> the "Cemetery entrance" hero caption >> the approval pill. Chrome text, lists and inputs are light pencil on paper — deliberately secondary. No card borders, shadows or heavy strokes: regions are separated by background-tone shifts and the two full-width dividers at y≈0.668/0.672 only. Whitespace frames the image block (~0.02 on all sides).

Ambiguous / must-be-decided-by-redesign (all flagged from real measurements, not guessed):
1. Top-bar line-2 item at (0.175–0.208, y0.073) — OCR reads "93" and/or "Plan", unusually large; could be a tab, count, or heading.
2. Nav rail items at y 0.428 and y 0.518 — pencil glyphs unreadable (7 icon slots, 5 legible labels: Home, Studio, Files, Assets, Voices).
3. Alternate action-bar button labels — illegible light-on-dark text; redesign must define them.
4. Center header word (0.289,0.215) reads "Details" in three passes though the plan doc calls this area "Scene Look"; "Inventory" (0.448,0.212) may be a second tab.
5. Scene list titles rows 3 & 5 — "Sturdy?" / "Violation?" illegible; rows 1,2,4,6 = "The Encounter", "Pursuit", "Cemetery" (selected), "Together".
6. First and fourth Finish row labels illegible.
7. Light "4 ▸▸" glyph cluster on T1's top edge (0.678–0.755,0.386) — pager/scroll control? Count?
8. Whether "PLAN"/"FINISH" are section footers or a workflow stepper — geometry supports the footer reading (both at y0.620 above their panels' dividers).
9. No dependencies metadata and no chat history are shown anywhere; if the redesign needs them, they are additions beyond this mock.
10. The mock's fixture text is "Production Studio — My Project" / scene "Cemetery" / caption "Cemetery entrance"; the "Last Train" fixture name does not appear in the mock's legible text (its platform/rail imagery appears only inside the drawn art blocks).
