/**
 * Naira money on the client, mirroring `domain::money` on the server.
 *
 * Everything crossing the wire is kobo as an integer. Naira only exist as
 * display strings and as what the organizer types — the moment a value is
 * money being stored or added up, it is kobo.
 */

/** 250000 -> "₦2,500.00" */
export function formatNaira(kobo: number): string {
  return new Intl.NumberFormat("en-NG", {
    style: "currency",
    currency: "NGN",
    minimumFractionDigits: 2,
  }).format(kobo / 100)
}

/** Compact form for summary tiles: 150000000 -> "₦1.5m". */
export function formatNairaShort(kobo: number): string {
  const naira = kobo / 100
  if (naira >= 1_000_000) return `₦${trimZero(naira / 1_000_000)}m`
  if (naira >= 10_000) return `₦${trimZero(naira / 1000)}k`
  return formatNaira(kobo)
}

function trimZero(n: number): string {
  return n.toFixed(1).replace(/\.0$/, "")
}

/**
 * Parses what an organizer types into kobo. Accepts "150,000", "₦150000",
 * "150000.50". Returns null when it isn't a number.
 *
 * Rounds rather than truncates, and does the scaling on a string-free path:
 * `150000.55 * 100` is 15000054.999... in binary floating point, which
 * truncation would turn into a kobo short every time.
 */
export function parseNairaToKobo(input: string): number | null {
  const cleaned = input.replace(/[₦,\s]/g, "")
  if (!cleaned) return 0
  if (!/^\d*\.?\d*$/.test(cleaned)) return null
  const naira = Number(cleaned)
  if (!Number.isFinite(naira) || naira < 0) return null
  return Math.round(naira * 100)
}

/** Kobo back to the plain naira string an organizer edits. */
export function koboToNairaInput(kobo: number): string {
  if (kobo === 0) return ""
  return (kobo / 100).toFixed(2).replace(/\.00$/, "")
}

export const PAID_STATUS_LABEL: Record<string, string> = {
  unpaid: "Unpaid",
  part_paid: "Part paid",
  paid: "Paid",
  overpaid: "Overpaid",
}
