/** Format integer minor units (cents) as a decimal string for display only.
 *  Authoritative totals stay in minor units on the host. Unknown is
 *  `null`/`undefined`/non-finite — never formatted as zero. */
export function minorToMoney(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "unknown";
  const sign = value < 0 ? "-" : "";
  const abs = Math.trunc(Math.abs(value));
  const major = Math.trunc(abs / 100);
  const cents = abs - major * 100;
  return `${sign}${major}.${String(cents).padStart(2, "0")}`;
}

/** Parse a dollar amount string ("50", "50.00", "0.05") into integer minor units
 *  (cents). Returns `null` for empty or non-numeric input — the caller decides
 *  whether empty means "no limit" vs an invalid entry. */
export function moneyToMinor(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(trimmed);
  if (!match) return null;
  const major = Number(match[1]);
  const cents = match[2] ? Number(match[2].padEnd(2, "0")) : 0;
  return major * 100 + cents;
}