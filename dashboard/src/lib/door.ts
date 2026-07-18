/**
 * The door's local memory.
 *
 * Venues lose signal. The scanner has to keep admitting people through it and
 * reconcile afterwards, so every scan is written to localStorage first and only
 * then sent. Nothing is ever dropped because a request failed.
 */

import { api } from "@/lib/api"
import type { CheckInRequest, CheckInResult, DoorEntry, DoorManifest } from "@/lib/types"

/** A scan waiting to reach the server. */
export interface QueuedScan {
  /** Stable across retries so the list can be keyed and de-duplicated. */
  id: string
  code: string
  allowOver: boolean
  /** When the person actually walked in, not when we managed to send it. */
  scannedAt: string
}

const QUEUE_KEY = "106.door.queue"
const MANIFEST_KEY = "106.door.manifest"

/** The alphabet from the server's `domain::code` — kept in step deliberately. */
const ALPHABET = "ACDEFGHJKMNPQRTWXY34679"
const CODE_LEN = 8

/** Uppercases and drops the spaces and dashes staff add while typing. */
export function normalizeCode(raw: string): string {
  return raw
    .replace(/[\s-]/g, "")
    .toUpperCase()
}

/**
 * Whether this could be one of ours. Lets a scanner reject a Wi-Fi QR or a
 * product barcode instantly, without a round trip it may not be able to make.
 */
export function isPlausibleCode(code: string): boolean {
  return code.length === CODE_LEN && [...code].every((c) => ALPHABET.includes(c))
}

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    // A corrupted or unavailable store must not take the door down.
    return fallback
  }
}

function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Private mode, or a full quota. The in-memory path still works for this
    // session; losing durability is better than losing the door.
  }
}

export function loadQueue(subEventId: string): QueuedScan[] {
  return read<QueuedScan[]>(`${QUEUE_KEY}.${subEventId}`, [])
}

export function saveQueue(subEventId: string, queue: QueuedScan[]): void {
  write(`${QUEUE_KEY}.${subEventId}`, queue)
}

export function loadCachedManifest(subEventId: string): DoorManifest | null {
  return read<DoorManifest | null>(`${MANIFEST_KEY}.${subEventId}`, null)
}

export function cacheManifest(manifest: DoorManifest): void {
  write(`${MANIFEST_KEY}.${manifest.subEventId}`, manifest)
}

/**
 * Answers a scan from the cached manifest alone.
 *
 * This is what an offline door shows the operator. It is deliberately the same
 * vocabulary the server uses, so the screen reads identically whether or not
 * there's signal — and `locallyCheckedIn` carries the heads this device has
 * already admitted since the manifest was fetched.
 */
export function judgeLocally(
  manifest: DoorManifest,
  code: string,
  locallyCheckedIn: Set<string>,
  allowOver: boolean,
): CheckInResult {
  const entry = manifest.entries.find((e) => e.code === code)
  if (!entry) {
    return blank("unknown_code")
  }
  if (entry.checkedIn || locallyCheckedIn.has(code)) {
    return { ...blank("already_in"), label: entry.label, guestName: entry.label }
  }

  const through = partyThrough(manifest, entry, locallyCheckedIn)
  if (through >= entry.partyAllowed && !allowOver) {
    return {
      outcome: "over_allowance",
      label: entry.label,
      guestName: entry.label,
      partyCheckedIn: through,
      partyAllowed: entry.partyAllowed,
      checkedInAt: null,
    }
  }

  return {
    outcome: "admitted",
    label: entry.label,
    guestName: entry.label,
    partyCheckedIn: through + 1,
    partyAllowed: entry.partyAllowed,
    checkedInAt: new Date().toISOString(),
  }
}

/** How many of this guest's party are already through, per the local view. */
function partyThrough(
  manifest: DoorManifest,
  entry: DoorEntry,
  locallyCheckedIn: Set<string>,
): number {
  return manifest.entries.filter(
    (e) => e.guestId === entry.guestId && (e.checkedIn || locallyCheckedIn.has(e.code)),
  ).length
}

function blank(outcome: CheckInResult["outcome"]): CheckInResult {
  return {
    outcome,
    label: null,
    guestName: null,
    partyCheckedIn: 0,
    partyAllowed: 0,
    checkedInAt: null,
  }
}

/**
 * Sends one scan. Returns null when the request never reached the server, which
 * is the signal to leave it on the queue and try again later.
 */
export async function sendScan(
  subEventId: string,
  scan: QueuedScan,
  offline: boolean,
): Promise<CheckInResult | null> {
  const body: CheckInRequest = {
    code: scan.code,
    allowOver: scan.allowOver,
    offline,
    scannedAt: scan.scannedAt,
  }
  try {
    return await api.post<CheckInResult>(`/api/sub-events/${subEventId}/check-in`, body)
  } catch {
    // Every real outcome comes back as a 200 with an outcome string, so a
    // thrown error here means the network, not a refusal.
    return null
  }
}

/**
 * Drains the queue oldest-first, stopping at the first failure so scans reach
 * the server in the order they happened. Returns what's still outstanding.
 */
export async function flushQueue(
  subEventId: string,
  queue: QueuedScan[],
): Promise<QueuedScan[]> {
  const remaining = [...queue]
  while (remaining.length > 0) {
    const result = await sendScan(subEventId, remaining[0], true)
    if (result === null) break
    remaining.shift()
  }
  return remaining
}
