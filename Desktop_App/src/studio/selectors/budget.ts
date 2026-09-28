import type { JobRecord, ProjectBudget, ProjectState } from "../../workflowTypes";
import { minorToMoney } from "../money";

export type BudgetModel = {
  currency: string;
  /** A budget limit is configured (limit_minor is a finite number). */
  hasLimit: boolean;
  /** The ceiling is non-bypassable for the Director. */
  hardStop: boolean;
  /** Number of jobs recorded, so the UI can decide whether to render at all. */
  jobCount: number;
  /** Formatted limit, or `null` when unbudgeted. */
  limit: string | null;
  /** Formatted completed + failed (actually settled) spend. */
  spent: string;
  /** Formatted pending/running estimated (committed but unsettled) spend. */
  committed: string;
  /** Formatted headroom for new spend (`limit − spent − committed`), `null` when unbudgeted. */
  remaining: string | null;
  /** Count of settled jobs whose real spend is unknown (never zero, flagged not gated). */
  unknownJobs: number;
  /** Committed + settled already exceeds the limit, so the next proposal is gated. */
  overLimit: boolean;
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
  const settled = settle(jobs);
  const spentMinor = settled.completed + settled.failed;
  const limit = budget?.limit_minor ?? null;
  const hasLimit = typeof limit === "number" && Number.isFinite(limit);
  const limitValue = hasLimit ? (limit as number) : 0;
  const committed = settled.committed;
  const overLimit = hasLimit && committed + spentMinor > limitValue;
  const remaining = hasLimit ? limitValue - spentMinor - committed : null;
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
  };
}

function settle(jobs: JobRecord[]): Settled {
  let completed = 0;
  let failed = 0;
  let committed = 0;
  let unknown = 0;
  for (const job of jobs) {
    const status = job.status ?? "";
    if (status === "pending" || status === "running") {
      if (isMinor(job.estimated_cost_minor)) committed += job.estimated_cost_minor as number;
    } else if (status === "completed") {
      if (isMinor(job.cost_minor)) completed += job.cost_minor as number;
      else unknown += 1;
    } else if (status === "failed" || status === "cancelled") {
      if (isMinor(job.cost_minor)) failed += job.cost_minor as number;
      else unknown += 1;
    }
  }
  return { completed, failed, committed, unknown };
}

function isMinor(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}