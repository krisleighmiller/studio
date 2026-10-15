import { REQUIREMENT_KINDS, REQUIREMENT_STATUSES } from "../../workflowTypes";
import { minorToMoney, moneyToMinor } from "../money";
import { getSurfaceHost, isProfileOpen, setProfileOpen } from "./session";
import { presentSurface } from "./shared";
import { buildProfileModel } from "./surfaceModels";
import type { ProfileIntent } from "./surfaceIntents";

function paintProjectProfile(emit: (intent: ProfileIntent) => void): HTMLElement {
  const wrap = document.createElement("section");
  wrap.className = "workflow-project";
  wrap.setAttribute("aria-label", "Production profile");
  const profile = getSurfaceHost().state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bar = document.createElement("div");
  bar.className = "workflow-profile-bar";
  const recipe = document.createElement("span");
  recipe.className = "workflow-profile-recipe";
  recipe.textContent = getSurfaceHost().activeRecipe()?.label ?? "Narrative dialogue";
  const summary = document.createElement("p");
  summary.className = "workflow-profile-summary";
  const bits = [
    deliverable?.name || getSurfaceHost().state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  summary.textContent = bits.join(" · ");
  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "ghost-button";
  edit.textContent = isProfileOpen() ? "Close profile" : "Edit profile";
  edit.disabled = Boolean(getSurfaceHost().busy);
  edit.addEventListener("click", () => {
    setProfileOpen(!isProfileOpen());
    getSurfaceHost().rerender();
  });
  bar.append(recipe, summary, edit);
  wrap.append(bar);
  if (isProfileOpen()) wrap.append(renderProfileForm(emit));
  if (isProfileOpen()) wrap.append(renderBudgetEditor(emit));
  wrap.append(renderRequirements(emit));
  return wrap;
}

function renderProfileForm(emit: (intent: ProfileIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const activeRecipeId = host.activeRecipe()?.id;
  const profile = host.state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const form = document.createElement("form");
  form.className = "workflow-profile-form";
  const recipeLabel = document.createElement("label");
  recipeLabel.textContent = "Recipe";
  const recipeInput = document.createElement("select");
  recipeInput.name = "recipe";
  const report = getSurfaceHost().report;
  const choices = report?.recipes?.length
    ? report.recipes
    : [
        { id: "narrative_dialogue", label: "Narrative dialogue" },
        { id: "montage", label: "Montage" },
      ];
  for (const choice of choices) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    option.selected = choice.id === (profile?.recipe ?? activeRecipeId ?? "narrative_dialogue");
    recipeInput.append(option);
  }
  recipeLabel.append(recipeInput);
  form.append(recipeLabel);
  const fields: { key: string; label: string; value: string; wide?: boolean }[] = [
    { key: "name", label: "Deliverable", value: deliverable?.name ?? "" },
    { key: "audience", label: "Audience", value: profile?.audience ?? deliverable?.audience ?? "" },
    {
      key: "platforms",
      label: "Platforms",
      value: (profile?.platforms ?? deliverable?.platforms ?? []).join(", "),
    },
    { key: "runtime", label: "Runtime", value: profile?.runtime ?? deliverable?.runtime ?? "" },
    {
      key: "aspect",
      label: "Aspect ratio",
      value: deliverable?.aspect_ratio ?? profile?.aspect_ratios?.[0] ?? "",
    },
    {
      key: "resolution",
      label: "Resolution",
      value: profile?.resolution ?? deliverable?.resolution ?? "",
    },
    {
      key: "frame_rate",
      label: "Frame rate",
      value: profile?.frame_rate ?? deliverable?.frame_rate ?? "",
    },
    { key: "language", label: "Language", value: profile?.language ?? deliverable?.language ?? "" },
    { key: "medium", label: "Medium", value: profile?.medium ?? "" },
    { key: "style_notes", label: "Style notes", value: profile?.style_notes ?? "", wide: true },
    { key: "audio_plan", label: "Audio plan", value: profile?.audio_plan ?? "", wide: true },
    {
      key: "continuity_rules",
      label: "Continuity rules",
      value: (profile?.continuity_rules ?? []).join("\n"),
      wide: true,
    },
    {
      key: "safety_constraints",
      label: "Safety constraints",
      value: (profile?.safety_constraints ?? []).join("\n"),
      wide: true,
    },
    { key: "budget", label: "Budget", value: profile?.budget ?? "" },
    { key: "schedule", label: "Schedule", value: profile?.schedule ?? "" },
    { key: "rights_notes", label: "Rights notes", value: profile?.rights_notes ?? "", wide: true },
  ];
  for (const field of fields) {
    const label = document.createElement("label");
    label.className = field.wide ? "wide" : "";
    label.textContent = field.label;
    const input = field.wide ? document.createElement("textarea") : document.createElement("input");
    input.name = field.key;
    input.value = field.value;
    if (field.key === "medium") {
      input.setAttribute("placeholder", "Clay, live action, anime…");
    }
    label.append(input);
    form.append(label);
  }
  const save = document.createElement("button");
  save.type = "submit";
  save.className = "ghost-button";
  save.textContent = "Save profile";
  save.disabled = Boolean(getSurfaceHost().busy);
  form.append(save);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const read = (key: string) =>
      (form.elements.namedItem(key) as HTMLInputElement | HTMLTextAreaElement | null)?.value ?? "";
    const blank = (value: string) => {
      const trimmed = value.trim();
      return trimmed ? trimmed : null;
    };
    const list = (value: string) =>
      value
        .split(/[,\n]/)
        .map((part) => part.trim())
        .filter(Boolean);
    const lines = (value: string) =>
      value
        .split("\n")
        .map((part) => part.trim())
        .filter(Boolean);
    const name = read("name").trim();
    const aspect = blank(read("aspect"));
    const platforms = list(read("platforms"));
    emit({
      type: "save",
      profile: {
        recipe: recipeInput.value,
        deliverables: [
          {
            id: deliverable?.id ?? "",
            name,
            audience: blank(read("audience")),
            platforms,
            runtime: blank(read("runtime")),
            aspect_ratio: aspect,
            resolution: blank(read("resolution")),
            frame_rate: blank(read("frame_rate")),
            language: blank(read("language")),
          },
        ],
        audience: blank(read("audience")),
        platforms,
        runtime: blank(read("runtime")),
        aspect_ratios: aspect ? [aspect] : [],
        resolution: blank(read("resolution")),
        frame_rate: blank(read("frame_rate")),
        language: blank(read("language")),
        medium: blank(read("medium")),
        style_notes: blank(read("style_notes")),
        audio_plan: blank(read("audio_plan")),
        continuity_rules: lines(read("continuity_rules")),
        safety_constraints: lines(read("safety_constraints")),
        budget: blank(read("budget")),
        schedule: blank(read("schedule")),
        rights_notes: blank(read("rights_notes")),
      },
    });
  });
  return form;
}

function renderBudgetEditor(emit: (intent: ProfileIntent) => void): HTMLElement {
  const budget = getSurfaceHost().state?.budget ?? null;
  const section = document.createElement("section");
  section.className = "workflow-budget-editor";
  section.setAttribute("aria-label", "Spend budget");
  const heading = document.createElement("h2");
  heading.textContent = "Spend budget";
  const form = document.createElement("form");
  form.className = "workflow-profile-form";

  const currency = document.createElement("label");
  currency.textContent = "Currency";
  const currencyInput = document.createElement("input");
  currencyInput.name = "budget_currency";
  currencyInput.value = budget?.currency ?? "USD";

  const limit = document.createElement("label");
  limit.textContent = "Limit";
  const limitInput = document.createElement("input");
  limitInput.name = "budget_limit";
  limitInput.placeholder = "e.g. 50.00";
  limitInput.value = budget?.limit_minor != null ? minorToMoney(budget.limit_minor) : "";

  const warning = document.createElement("label");
  warning.textContent = "Warning at";
  const warningInput = document.createElement("input");
  warningInput.name = "budget_warning";
  warningInput.placeholder = "e.g. 40.00";
  warningInput.value = budget?.warning_at_minor != null ? minorToMoney(budget.warning_at_minor) : "";

  const policyLabel = document.createElement("label");
  policyLabel.className = "wide";
  policyLabel.textContent = "Spending policy";
  const policySelect = document.createElement("select");
  policySelect.name = "budget_policy";
  for (const [value, label] of [
    ["strict_hard_stop", "Strict block for v1 — paid generation unavailable without verified maxima"],
    ["approval_required", "Approval required — authorize each paid request after unknown-cost disclosure"],
    ["warn_only", "Warn policy — v1 still requires authorization for each paid request"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    policySelect.append(option);
  }
  policySelect.value = budget?.policy ?? (budget?.hard_stop ? "strict_hard_stop" : "approval_required");
  policyLabel.append(policySelect);

  const note = document.createElement("p");
  note.className = "workflow-budget-note";
  note.textContent = "Studio v1 does not guarantee or verify provider charges. Strict policy remains fail-closed and blocks paid generation until the user explicitly changes it; Approval Required and Warn policies require per-request authorization after unknown-cost disclosure. LLM/chat requests remain blocked and are not covered by this budget.";

  currency.append(currencyInput);
  limit.append(limitInput);
  warning.append(warningInput);
  form.append(currency, limit, warning, policyLabel, note);

  const actions = document.createElement("span");
  actions.className = "workflow-budget-actions";
  const save = document.createElement("button");
  save.type = "submit";
  save.className = "ghost-button";
  save.textContent = "Save budget";
  save.disabled = Boolean(getSurfaceHost().busy);
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "ghost-button";
  clear.textContent = "Clear budget";
  clear.disabled = Boolean(getSurfaceHost().busy) || !budget;
  clear.addEventListener("click", () => emit({ type: "set-budget", budget: null }));
  actions.append(save, clear);
  form.append(actions);

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const currencyValue = currencyInput.value.trim() || "USD";
    const limitText = limitInput.value.trim();
    const warningText = warningInput.value.trim();
    const limitMinor = limitText ? moneyToMinor(limitText) : null;
    const warningMinor = warningText ? moneyToMinor(warningText) : null;
    if (limitText && limitMinor == null) {
      note.textContent = "Limit must be a dollar amount like 50 or 50.00.";
      return;
    }
    if (warningText && warningMinor == null) {
      note.textContent = "Warning level must be a dollar amount like 40 or 40.00.";
      return;
    }
    if (limitMinor != null && warningMinor != null && warningMinor > limitMinor) {
      note.textContent = "Warning level cannot exceed the limit.";
      return;
    }
    emit({
      type: "set-budget",
      budget: {
        currency: currencyValue,
        limit_minor: limitMinor,
        warning_at_minor: warningMinor,
        hard_stop: policySelect.value === "strict_hard_stop",
        policy: policySelect.value as "strict_hard_stop" | "approval_required" | "warn_only",
      },
    });
  });

  section.append(heading, form);
  return section;
}

function renderRequirements(emit: (intent: ProfileIntent) => void): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-requirements";
  section.setAttribute("aria-label", "Requirements");
  const heading = document.createElement("h2");
  heading.textContent = "Requirements";
  const count = document.createElement("span");
  const requirements = [...(getSurfaceHost().state?.requirements ?? [])];
  count.textContent = String(requirements.length);
  heading.append(count);
  const list = document.createElement("ul");
  list.className = "workflow-requirements-list";
  if (!requirements.length) {
    const empty = document.createElement("li");
    empty.className = "workflow-requirement empty";
    empty.textContent = "No requirements yet.";
    list.append(empty);
  }
  for (const requirement of requirements) {
    const item = document.createElement("li");
    item.className = "workflow-requirement";
    item.dataset.status = requirement.status;
    item.dataset.kind = requirement.kind;
    const selectedSceneId = getSurfaceHost().selectedSceneId;
    const inScene = selectedSceneId != null && (requirement.scene_ids ?? []).includes(selectedSceneId);
    if (inScene) item.classList.add("in-scene");
    if (inScene && getSurfaceHost().requirementBlocks(requirement)) item.classList.add("blocking");
    const kind = document.createElement("span");
    kind.className = "workflow-requirement-kind";
    kind.textContent = getSurfaceHost().requirementKindLabel(requirement.kind);
    const name = document.createElement("span");
    name.className = "workflow-requirement-name";
    name.textContent = requirement.name;
    const scenes = document.createElement("span");
    scenes.className = "workflow-requirement-scenes";
    scenes.textContent = (requirement.scene_ids ?? [])
      .map((id) => getSurfaceHost().state?.scenes.find((scene) => scene.id === id)?.title ?? `Scene ${id}`)
      .join(", ");
    const status = document.createElement("select");
    status.className = "workflow-requirement-status";
    status.setAttribute("aria-label", `${requirement.name} status`);
    status.disabled = Boolean(getSurfaceHost().busy);
    for (const option of REQUIREMENT_STATUSES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === requirement.status;
      status.append(choice);
    }
    status.addEventListener("change", () => {
      emit({
        type: "set-requirement",
        requirementId: requirement.id,
        status: status.value,
        name: requirement.name,
      });
    });
    item.append(kind, name, scenes, status);
    list.append(item);
  }
  const add = document.createElement("form");
  add.className = "workflow-requirement-add";
  const nameInput = document.createElement("input");
  nameInput.name = "name";
  nameInput.placeholder = "Add a requirement";
  nameInput.setAttribute("aria-label", "Requirement name");
  const kindInput = document.createElement("select");
  kindInput.name = "kind";
  kindInput.setAttribute("aria-label", "Requirement kind");
  for (const option of REQUIREMENT_KINDS) {
    const choice = document.createElement("option");
    choice.value = option.id;
    choice.textContent = option.label;
    kindInput.append(choice);
  }
  const button = document.createElement("button");
  button.type = "submit";
  button.className = "ghost-button";
  button.textContent = "Add";
  button.disabled = Boolean(getSurfaceHost().busy);
  add.append(nameInput, kindInput, button);
  add.addEventListener("submit", (event) => {
    event.preventDefault();
    const name = nameInput.value.trim();
    if (!name) return;
    emit({
      type: "add-requirement",
      name,
      kind: kindInput.value,
      sceneId: getSurfaceHost().selectedSceneId,
    });
  });
  section.append(heading, list, add);
  return section;
}

export function renderProjectProfile(onIntent: (intent: ProfileIntent) => void): HTMLElement {
  const profile = getSurfaceHost().state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bits = [
    deliverable?.name || getSurfaceHost().state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  const model = buildProfileModel({
    projectName: getSurfaceHost().state?.project_name ?? null,
    recipeLabel: getSurfaceHost().activeRecipe()?.label ?? null,
    summary: bits.join(" · "),
    requirementCount: getSurfaceHost().state?.requirements?.length ?? 0,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
  });
  return presentSurface(paintProjectProfile(onIntent), model, false);
}
