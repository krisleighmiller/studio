import type { JobRecord, ProjectBudget, ProjectState } from "../../workflowTypes";
import { minorToMoney } from "../money";

export type GateStatus =
  | "none" /* NoLimit — no budget configured, or Allow — under limit */
  | "warn"
  | "confirm"
  | "blocked";

export type BudgetModel = {
  /** LLM/chat is not included in the accounted-job ledger or guaranteed by this budget. */
  chatCoverage: "not-accounted";
  currency: string;
  hasLimit: boolean;
  hardStop: boolean;
  jobCount: number;
  limit: string | null;
  spent: string;
  committed: string;
  /** Available numeric headroom only when every accounted exposure is bounded. */
  remaining: string | null;
  unknownJobs: number;
  unknownExposureJobs: number;
  overLimit: boolean;
  /** Budget gate status for the next operation: none / warn / confirm / blocked. */
  gate: GateStatus;
};

type Settled = {
  completed: number;
  failed: number;
  committed: number;
  unknown: number;
  unknownExposure: number;
};

/** Mirror of the host's accounted-job ledger semantics only. Chat is excluded;
 *  this is not a project-wide spend or authorization total.
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
  const remaining = hasLimit && settled.unknownExposure === 0 ? limitValue - onTheHook : null;
  const gate = computeGate(budget, spentMinor, committed, settled.unknownExposure);
  const policy = budget?.policy ?? (budget?.hard_stop ? "strict_hard_stop" : "approval_required");
  return {
    chatCoverage: "not-accounted",
    currency: budget?.currency || "USD",
    hasLimit,
    hardStop: policy === "strict_hard_stop",
    jobCount: jobs.length,
    limit: hasLimit ? minorToMoney(limitValue) : null,
    spent: minorToMoney(spentMinor),
    committed: minorToMoney(committed),
    remaining: remaining == null ? null : minorToMoney(remaining),
    unknownJobs: settled.unknown,
    unknownExposureJobs: settled.unknownExposure,
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
  unknownExposure: number,
): GateStatus {
  if (!budget) return "none";
  const limit = budget.limit_minor;
  if (typeof limit !== "number" || !Number.isFinite(limit)) return "none";
  const policy = budget.policy ?? (budget.hard_stop ? "strict_hard_stop" : "approval_required");
  if (policy === "strict_hard_stop" && unknownExposure > 0) return "blocked";
  const onTheHook = spentMinor + committed;
  if (onTheHook > limit) return policy === "strict_hard_stop" ? "blocked" : policy === "warn_only" ? "warn" : "confirm";
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
  let unknownExposure = 0;
  for (const job of jobs) {
    if (!job.provider && !job.spend_category && job.cost_minor == null && job.estimated_cost_minor == null) continue;
    if (job.currency != null && job.currency.trim().toUpperCase() !== currency.trim().toUpperCase()) {
      unknown += 1;
      unknownExposure += 1;
      continue;
    }
    const reserved = isMinor(job.reserved_max_minor) ? job.reserved_max_minor : null;
    if (reserved === null && !(job.actual_verified && isMinor(job.cost_minor))) unknownExposure += 1;
    const status = job.status ?? "";
    if (status === "pending" || status === "running") {
      if (reserved !== null) committed += reserved;
      else if (isMinor(job.estimated_cost_minor)) committed += job.estimated_cost_minor;
      else unknown += 1;
    } else if (status === "completed") {
      if ((job.spend_category || job.provider) && isMinor(job.cost_minor) && !job.actual_verified) unknown += 1;
      if (isMinor(job.cost_minor)) {
        completed += job.cost_minor;
        if (!job.actual_verified && reserved !== null) committed += Math.max(0, reserved - job.cost_minor);
      }
      else {
        unknown += 1;
        if (reserved !== null) committed += reserved;
      }
    } else if (status === "failed" || status === "cancelled") {
      if ((job.spend_category || job.provider) && isMinor(job.cost_minor) && !job.actual_verified) unknown += 1;
      if (isMinor(job.cost_minor)) {
        failed += job.cost_minor;
        if (!job.actual_verified && reserved !== null) committed += Math.max(0, reserved - job.cost_minor);
      } else {
        unknown += 1;
        if (reserved !== null) committed += reserved;
      }
    }
  }
  return { completed, failed, committed, unknown, unknownExposure };
}

function isMinor(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}