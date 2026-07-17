/**
 * Converts a naive local datetime (as produced by <input type="datetime-local">)
 * interpreted in the given IANA timezone into a UTC ISO string.
 *
 * Two-pass technique: guess UTC = naive time, measure what wall-clock that
 * lands on in the target zone, correct by the difference. A second pass covers
 * DST edges (irrelevant for Africa/Lagos, but organizers can pick other zones).
 */
export function zonedToUtcIso(naive: string, timeZone: string): string {
  const target = new Date(`${naive}:00Z`).getTime()
  let guess = target
  for (let i = 0; i < 2; i++) {
    guess = target - (wallClockAsUtc(guess, timeZone) - guess)
  }
  return new Date(guess).toISOString().replace(".000Z", "Z")
}

/** Renders a UTC instant as a datetime-local value in the given timezone. */
export function utcToZonedLocal(iso: string, timeZone: string): string {
  const parts = formatterFor(timeZone).formatToParts(new Date(iso))
  const get = (type: string) => parts.find((p) => p.type === type)?.value ?? "00"
  return `${get("year")}-${get("month")}-${get("day")}T${get("hour")}:${get("minute")}`
}

function wallClockAsUtc(instant: number, timeZone: string): number {
  const parts = formatterFor(timeZone).formatToParts(new Date(instant))
  const get = (type: string) => Number(parts.find((p) => p.type === type)?.value ?? 0)
  return Date.UTC(get("year"), get("month") - 1, get("day"), get("hour"), get("minute"), get("second"))
}

const formatters = new Map<string, Intl.DateTimeFormat>()

function formatterFor(timeZone: string): Intl.DateTimeFormat {
  let fmt = formatters.get(timeZone)
  if (!fmt) {
    fmt = new Intl.DateTimeFormat("en-CA", {
      timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hourCycle: "h23",
    })
    formatters.set(timeZone, fmt)
  }
  return fmt
}

/** "Sat 21 Nov 2026, 1:00 pm" in the event's timezone. */
export function formatInZone(iso: string, timeZone: string): string {
  return new Intl.DateTimeFormat("en-NG", {
    timeZone,
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(iso))
}

/** "21 Nov 2026" in the event's timezone. */
export function formatDateInZone(iso: string, timeZone: string): string {
  return new Intl.DateTimeFormat("en-NG", {
    timeZone,
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(iso))
}
