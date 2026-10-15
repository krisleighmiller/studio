import type { JobRecord } from "../../workflowTypes";
import { moneyToMinor, minorToMoney } from "../money";

export const REVIEW_ACK = "I reviewed the provider evidence and accept this manual accounting entry";
export type SpendReviewList = { ledgerRevision: string; jobs: JobRecord[] };
export type SpendReviewIntent = {
  jobId: string; ledgerRevision: string; currency: string; actualMinor: number;
  providerOutcome: "charged" | "not_charged"; evidenceReference: string;
  reviewNote: string; acknowledgment: string;
};

/** No automated provider verification exists. This is explicitly a manual evidence review. */
export function renderSpendReview(
  list: SpendReviewList | null,
  fetch: () => void,
  submit: (intent: SpendReviewIntent) => void,
): HTMLElement {
  const panel = document.createElement("section");
  panel.className = "studio-spend-review";
  const heading = document.createElement("h3");
  heading.textContent = "Unsettled paid media · manual evidence review";
  const disclosure = document.createElement("p");
  disclosure.textContent = "Provider verification is unavailable here. Review attributable provider billing/history evidence yourself. Entering a number does not verify a charge; this manual accounting decision is recorded with its evidence and may unblock retry. Unknown cost blocks new paid work.";
  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.textContent = "Load / refresh unsettled jobs";
  refresh.addEventListener("click", fetch);
  panel.append(heading, disclosure, refresh);
  if (!list) return panel;
  if (!list.jobs.length) {
    const empty = document.createElement("p");
    empty.textContent = "No unsettled paid jobs.";
    panel.append(empty);
  }
  for (const job of list.jobs) {
    const group = document.createElement("div");
    const label = document.createElement("p");
    label.textContent = `${job.id} · ${job.provider ?? "unknown provider"} · ${job.kind} · ${job.status ?? "unknown"} · estimate ${minorToMoney(job.estimated_cost_minor)} ${job.currency ?? "?"}. ${job.error ?? ""}`;
    group.append(label);
    // A running or pending job has not yet reached a reviewable outcome.
    if (job.status !== "failed" && job.status !== "completed") {
      panel.append(group);
      continue;
    }
    const form = document.createElement("form");
    const amount = document.createElement("input");
    amount.placeholder = "Actual amount (e.g. 0.00)";
    amount.setAttribute("aria-label", `Actual charge for ${job.id}`);
    amount.required = true;
    const outcome = document.createElement("select");
    outcome.setAttribute("aria-label", `Provider outcome for ${job.id}`);
    for (const [value, text] of [["", "Confirm provider outcome"], ["charged", "Charged"], ["not_charged", "Not charged"]]) {
      const option = document.createElement("option"); option.value = value; option.textContent = text; outcome.append(option);
    }
    outcome.required = true;
    const evidence = document.createElement("input");
    evidence.placeholder = "Provider request/history ID or billing record reference";
    evidence.required = true;
    const note = document.createElement("input");
    note.placeholder = "What evidence did you review?";
    note.required = true;
    const acknowledge = document.createElement("input");
    acknowledge.type = "checkbox";
    const acknowledgeLabel = document.createElement("label");
    acknowledgeLabel.append(acknowledge, document.createTextNode(REVIEW_ACK));
    const submitButton = document.createElement("button");
    submitButton.type = "submit";
    submitButton.textContent = "Record manual review (not provider-verified)";
    form.append(amount, outcome, evidence, note, acknowledgeLabel, submitButton);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const actualMinor = moneyToMinor(amount.value);
      if (actualMinor === null || !Number.isSafeInteger(actualMinor) || !acknowledge.checked || !outcome.value ||
          (actualMinor === 0 && outcome.value !== "not_charged") || (actualMinor > 0 && outcome.value !== "charged")) {
        amount.setCustomValidity("Enter a valid amount matching the confirmed outcome, and acknowledge the manual review.");
        amount.reportValidity();
        return;
      }
      amount.setCustomValidity("");
      submit({ jobId: job.id, ledgerRevision: list.ledgerRevision, currency: job.currency ?? "",
        actualMinor, providerOutcome: outcome.value as "charged" | "not_charged",
        evidenceReference: evidence.value, reviewNote: note.value, acknowledgment: REVIEW_ACK });
    });
    group.append(form);
    panel.append(group);
  }
  return panel;
}
