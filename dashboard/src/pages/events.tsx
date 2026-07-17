import { CalendarDays, MapPin, Plus } from "lucide-react"
import { motion } from "motion/react"
import { Link } from "react-router"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useEvents } from "@/lib/queries"
import { formatDateInZone } from "@/lib/time"
import type { EventSummary } from "@/lib/types"

function dateLabel(event: EventSummary): string {
  if (!event.firstStartsAt) return "No date yet"
  const first = formatDateInZone(event.firstStartsAt, event.timezone)
  if (!event.lastStartsAt) return first
  const last = formatDateInZone(event.lastStartsAt, event.timezone)
  return first === last ? first : `${first} – ${last}`
}

export function EventsPage() {
  const events = useEvents()

  if (events.isPending) {
    return (
      <div className="grid gap-4 sm:grid-cols-2">
        {[0, 1, 2].map((i) => (
          <div key={i} className="rounded-xl border bg-card p-5">
            <Skeleton className="h-6 w-3/4" />
            <Skeleton className="mt-3 h-4 w-1/2" />
          </div>
        ))}
      </div>
    )
  }

  if (events.isError) {
    return (
      <div className="flex flex-col items-center gap-4 py-16 text-center">
        <p className="text-muted-foreground">Couldn't load your events.</p>
        <Button variant="outline" onClick={() => events.refetch()}>
          Try again
        </Button>
      </div>
    )
  }

  if (events.data.length === 0) {
    return (
      <div className="flex flex-col items-center gap-4 py-20 text-center">
        <h1 className="font-heading text-3xl font-semibold">Plan something beautiful.</h1>
        <p className="max-w-md text-muted-foreground">
          Create your event, share a stunning page, and keep every guest and every part of the
          day in one place.
        </p>
        <Button asChild size="lg" className="mt-2">
          <Link to="/events/new">
            <Plus data-slot="icon" />
            Create your first event
          </Link>
        </Button>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h1 className="font-heading text-2xl font-semibold">Your events</h1>
        <Button asChild size="sm" className="sm:hidden">
          <Link to="/events/new">
            <Plus data-slot="icon" />
            New
          </Link>
        </Button>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        {events.data.map((event, i) => (
          <motion.div
            key={event.id}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: Math.min(i, 8) * 0.04, duration: 0.3, ease: "easeOut" }}
          >
            <Link
              to={`/events/${event.id}`}
              className="group block rounded-xl border bg-card p-5 transition-colors hover:border-gold/40"
            >
              <div className="flex items-start justify-between gap-3">
                <h2 className="font-heading text-xl font-semibold leading-snug group-hover:text-gold-bright">
                  {event.title}
                </h2>
                {event.subEventCount > 1 && (
                  <Badge variant="secondary" className="shrink-0">
                    {event.subEventCount} parts
                  </Badge>
                )}
              </div>
              <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
                <CalendarDays className="size-4 shrink-0" aria-hidden />
                {dateLabel(event)}
              </p>
              {event.description && (
                <p className="mt-2 line-clamp-2 text-sm text-muted-foreground">
                  {event.description}
                </p>
              )}
              {event.timezone !== "Africa/Lagos" && (
                <p className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                  <MapPin className="size-3.5 shrink-0" aria-hidden />
                  {event.timezone}
                </p>
              )}
            </Link>
          </motion.div>
        ))}
      </div>
    </div>
  )
}
