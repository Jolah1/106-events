export interface AppConfig {
  publicBaseUrl: string
}

export type Role = "admin" | "staff"

export interface User {
  id: string
  email: string | null
  phone: string | null
  name: string
  role: Role
  createdAt: string
}

export interface TeamMember {
  id: string
  email: string | null
  name: string
  role: Role
  createdAt: string
}

export interface InviteInput {
  email: string
  name?: string
  role?: Role
}

export interface SubEvent {
  id: string
  eventId: string
  name: string
  slug: string
  description: string
  startsAt: string
  endsAt: string | null
  venueName: string
  venueAddress: string
  isDefault: boolean
  position: number
}

export interface Event {
  id: string
  title: string
  slug: string
  description: string
  coverImageUrl: string | null
  timezone: string
  createdAt: string
  updatedAt: string
}

export interface EventSummary extends Event {
  subEventCount: number
  firstStartsAt: string | null
  lastStartsAt: string | null
}

export interface EventDetail extends Event {
  subEvents: SubEvent[]
}

export interface SubEventInput {
  name: string
  description?: string
  startsAt: string
  endsAt?: string | null
  venueName?: string
  venueAddress?: string
  isDefault?: boolean
}

export interface CreateEventInput {
  title: string
  description?: string
  timezone?: string
  coverImageUrl?: string
  subEvents: SubEventInput[]
}

export type RsvpStatus = "pending" | "confirmed" | "declined" | "partial"

export interface Guest {
  id: string
  eventId: string
  name: string
  phone: string | null
  email: string | null
  plusOnes: number
  dietary: string
  notes: string
  /** The parts of the event this guest is invited to. */
  subEventIds: string[]
  /** The guest's RSVP link is `{publicBaseUrl}/r/{rsvpToken}`. */
  rsvpToken: string
  rsvpStatus: RsvpStatus
  /** Most heads they'll bring to any one part. */
  attendingHeads: number
  createdAt: string
  updatedAt: string
}

export interface CreateGuestInput {
  name: string
  phone?: string
  email?: string
  plusOnes?: number
  dietary?: string
  notes?: string
  subEventIds?: string[]
}

/** `null` clears a field; omitting it leaves the field alone. */
export interface GuestPatch {
  name?: string
  phone?: string | null
  email?: string | null
  plusOnes?: number
  dietary?: string
  notes?: string
  subEventIds?: string[]
}

export interface ImportInput {
  csv: string
  subEventIds?: string[]
  dryRun?: boolean
}

export interface ImportReport {
  dryRun: boolean
  created: number
  updated: number
  errors: { line: number; message: string }[]
  ignoredColumns: string[]
  unknownParts: string[]
}

/** A rung on an event's reminder ladder. */
export interface ReminderSchedule {
  id: string
  eventId: string
  /** How long before the event's first part this rung fires. */
  offsetMinutes: number
  enabled: boolean
  sentCount: number
  failedCount: number
  createdAt: string
}

/** A supplier for one event, with what they cost and what they've been paid. */
export interface Vendor {
  id: string
  eventId: string
  name: string
  category: string
  phone: string | null
  email: string | null
  service: string
  costKobo: number
  amountPaidKobo: number
  /** Derived server-side from the two amounts, so it can't drift. */
  paidStatus: "unpaid" | "part_paid" | "paid" | "overpaid"
  outstandingKobo: number
  notes: string
  createdAt: string
  updatedAt: string
}

export interface CreateVendorInput {
  name: string
  category?: string
  phone?: string
  email?: string
  service?: string
  costKobo?: number
  amountPaidKobo?: number
  notes?: string
}

/** `null` clears a field; omitting it leaves the field alone. */
export interface VendorPatch {
  name?: string
  category?: string
  phone?: string | null
  email?: string | null
  service?: string
  costKobo?: number
  amountPaidKobo?: number
  notes?: string
}

/** One person who may walk through the door, and the code that admits them. */
export interface Attendee {
  id: string
  guestId: string
  guestName: string
  /** "Aunt Ngozi" for the guest themselves, "Aunt Ngozi +1" for a plus-one. */
  label: string
  headIndex: number
  code: string
  /** Added at the door beyond the guest's allowance. */
  isExtra: boolean
}

export type CheckInOutcome =
  | "admitted"
  | "already_in"
  | "not_invited"
  | "unknown_code"
  | "over_allowance"

export interface CheckInResult {
  outcome: CheckInOutcome
  label: string | null
  guestName: string | null
  partyCheckedIn: number
  partyAllowed: number
  checkedInAt: string | null
}

export interface CheckInRequest {
  code: string
  /** Staff decided to admit someone past their confirmed party. */
  allowOver?: boolean
  offline?: boolean
  /** When the scan happened, for a queue replayed after the signal returned. */
  scannedAt?: string
}

export interface CheckInRecord {
  attendeeId: string
  label: string
  checkedInAt: string
  overAllowance: boolean
  syncedOffline: boolean
}

/** Everything the door needs cached to keep working with no signal. */
export interface DoorManifest {
  subEventId: string
  subEventName: string
  eventTitle: string
  generatedAt: string
  entries: DoorEntry[]
}

export interface DoorEntry {
  code: string
  label: string
  guestId: string
  partyAllowed: number
  checkedIn: boolean
}

/** Where one part of the event stands: RSVPs in, heads through the door. */
export interface PartStats {
  subEventId: string
  name: string
  startsAt: string
  isDefault: boolean
  /** Parties invited to this part, and how each stands. */
  invitedParties: number
  confirmedParties: number
  declinedParties: number
  pendingParties: number
  /** Heads confirmed parties said they'd bring — what catering cooks for. */
  confirmedHeads: number
  /** Heads actually admitted, including walk-ins past the allowance. */
  checkedInHeads: number
  overAllowanceHeads: number
  offlineSyncedHeads: number
}

/** The organizer's rollup, derived server-side so it can't drift. */
export interface EventStats {
  eventId: string
  guestCount: number
  /** The ceiling: every party at full allowance. */
  headsInvited: number
  /** Guests with an answer for every part they're invited to. */
  repliedGuests: number
  /** Guests with at least one part still unanswered. */
  awaitingGuests: number
  vendorCount: number
  vendorCostKobo: number
  vendorPaidKobo: number
  vendorOutstandingKobo: number
  parts: PartStats[]
}

/** Someone who asked for an account from the landing page. */
export interface AccessRequest {
  id: string
  name: string
  email: string
  phone: string | null
  /** What they said they're planning, in their own words. */
  about: string
  createdAt: string
  handledAt: string | null
}
