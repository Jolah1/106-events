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
