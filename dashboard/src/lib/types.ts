export interface AppConfig {
  publicBaseUrl: string
}

export interface User {
  id: string
  email: string | null
  phone: string | null
  name: string
  createdAt: string
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
