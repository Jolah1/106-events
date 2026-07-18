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
