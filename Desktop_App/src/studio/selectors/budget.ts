import type { JobRecord, ProjectBudget, ProjectState } from "../../workflowTypes";
import { minorToMoney } from "../money";

export type GateStatus =
  | "none" /* NoLimit — no budget configured, or Allow — under limit */
  | "warn"
  | "confirm"
  | "blocked";

export type BudgetModel = {
  currency: string;
  hasLimit: boolean;
  hardStop: boolean;
  jobCount: number;
  limit: string | null;
  spent: string;
  committed: string;
  /** Limit minus settled spend and outstanding estimated commitments. */
  remaining: string | null;
  unknownJobs: number;
  overLimit: boolean;
  /** Budget gate status for the next operation: none / warn / confirm / blocked. */
  gate: GateStatus;
};

type Settled = {
  completed: number;
  failed: number;
  committed: number;
  unknown: number;
};

/** Mirror of the host's `compute_ledger` semantics, in the renderer, so the UI
 *  shows the same "known / unknown / whether budget allows" that the host gates on.
 *  Integer minor units summed as integers (safe below 2^53 cents). */
export function buildBudgetModel(state: ProjectState | null): BudgetModel {
  const jobs: JobRecord[] = state?.jobs ?? [];
  const budget: ProjectBudget | null = state?.budget ?? null;
  const settled = settle(jobs, budget?.currency || "USD");
  const spentMinor = settled.completed + settled.failed;
  const limit = budget?.limit_minor ?? null;
  const hasLimit = typeof limit === "number" && Number.isFinite(limit);
  const limitValue = hasLimit ? (limit as number) : 0;
  const committed = settled.committed;
  const onTheHook = committed + spentMinor;
  const overLimit = hasLimit && onTheHook > limitValue;
  const remaining = hasLimit ? limitValue - onTheHook : null;
  const gate = computeGate(budget, spentMinor, committed);
  return {
    currency: budget?.currency || "USD",
    hasLimit,
    hardStop: budget?.hard_stop ?? false,
    jobCount: jobs.length,
    limit: hasLimit ? minorToMoney(limitValue) : null,
    spent: minorToMoney(spentMinor),
    committed: minorToMoney(committed),
    remaining: remaining == null ? null : minorToMoney(remaining),
    unknownJobs: settled.unknown,
    overLimit,
    gate,
  };
}

/** Mirror of the host's `gate_spend` for the UI.  Returns the most restrictive
 *  gate across the current on-the-hook spend. */
function computeGate(
  budget: ProjectBudget | null,
  spentMinor: number,
  committed: number,
): GateStatus {
  if (!budget) return "none";
  const limit = budget.limit_minor;
  if (typeof limit !== "number" || !Number.isFinite(limit)) return "none";
  const onTheHook = spentMinor + committed;
  if (onTheHook > limit) return budget.hard_stop ? "blocked" : "confirm";
  const warningAt = budget.warning_at_minor;
  if (typeof warningAt === "number" && Number.isFinite(warningAt) && onTheHook > warningAt) {
    return "warn";
  }
  return "none";
}

function settle(jobs: JobRecord[], currency: string): Settled {
  let completed = 0;
  let failed = 0;
  let committed = 0;
  let unknown = 0;
  for (const job of jobs) {
    if (job.currency != null && job.currency.trim().toUpperCase() !== currency.trim().toUpperCase()) {
      unknown += 1;
      continue;
    }
    const status = job.status ?? "";
    if (status === "pending" || status === "running") {
      if (isMinor(job.estimated_cost_minor)) committed += job.estimated_cost_minor;
      else unknown += 1;
    } else if (status === "completed") {
      if (isMinor(job.cost_minor)) completed += job.cost_minor as number;
      else {
        unknown += 1;
        if (isMinor(job.estimated_cost_minor)) committed += job.estimated_cost_minor;
      }
    } else if (status === "failed" || status === "cancelled") {
      if (isMinor(job.cost_minor)) failed += job.cost_minor as number;
      else unknown += 1;
    }
  }
  return { completed, failed, committed, unknown };
}

function isMinor(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}