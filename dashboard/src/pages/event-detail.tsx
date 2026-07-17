import { useState, type FormEvent } from "react"
import { CalendarDays, MapPin, Pencil, Plus, Trash2, Users } from "lucide-react"
import { Link, useNavigate, useParams } from "react-router"
import { toast } from "sonner"

import { PartFields, emptyPart, partError, type PartDraft } from "@/components/part-fields"
import { ShareLink } from "@/components/share-link"
import { TimezoneSelect } from "@/components/timezone-select"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { ApiError } from "@/lib/api"
import {
  useAddSubEvent,
  useDeleteEvent,
  useDeleteSubEvent,
  useEvent,
  useUpdateEvent,
  useUpdateSubEvent,
} from "@/lib/queries"
import { formatInZone, utcToZonedLocal, zonedToUtcIso } from "@/lib/time"
import { coverImageUrlError } from "@/lib/validation"
import type { EventDetail, SubEvent } from "@/lib/types"

export function EventDetailPage() {
  const { id } = useParams<{ id: string }>()
  const detail = useEvent(id!)

  if (detail.isPending) {
    return (
      <div className="mx-auto max-w-2xl">
        <Skeleton className="h-8 w-2/3" />
        <Skeleton className="mt-3 h-4 w-1/3" />
        <Skeleton className="mt-8 h-28 w-full rounded-xl" />
        <Skeleton className="mt-4 h-28 w-full rounded-xl" />
      </div>
    )
  }

  if (detail.isError) {
    const notFound = detail.error instanceof ApiError && detail.error.status === 404
    return (
      <div className="flex flex-col items-center gap-4 py-16 text-center">
        <p className="text-muted-foreground">
          {notFound ? "This event doesn't exist or isn't yours." : "Couldn't load this event."}
        </p>
        {notFound ? (
          <Button asChild variant="outline">
            <Link to="/">Back to events</Link>
          </Button>
        ) : (
          <Button variant="outline" onClick={() => detail.refetch()}>
            Try again
          </Button>
        )}
      </div>
    )
  }

  return <EventDetailView event={detail.data} />
}

function EventDetailView({ event }: { event: EventDetail }) {
  const navigate = useNavigate()
  const deleteEvent = useDeleteEvent()
  const [editOpen, setEditOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [addOpen, setAddOpen] = useState(false)

  const addSubEvent = useAddSubEvent(event.id)
  const parts = [...event.subEvents].sort((a, b) => a.position - b.position)
  const soloDefault = parts.length === 1 && parts[0].isDefault

  return (
    <div className="mx-auto max-w-2xl">
      <Link to="/" className="text-sm text-muted-foreground hover:text-foreground">
        ← Events
      </Link>

      {event.coverImageUrl && (
        <img
          src={event.coverImageUrl}
          alt=""
          className="mt-3 aspect-[16/7] w-full rounded-xl border object-cover"
        />
      )}

      <div className="mt-2 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="font-heading text-3xl font-semibold leading-tight">{event.title}</h1>
          {event.timezone !== "Africa/Lagos" && (
            <p className="mt-1 text-sm text-muted-foreground">{event.timezone}</p>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button asChild variant="outline" size="sm">
            <Link to={`/events/${event.id}/guests`}>
              <Users data-slot="icon" />
              Guests
            </Link>
          </Button>
          <Button variant="outline" size="sm" onClick={() => setEditOpen(true)}>
            <Pencil data-slot="icon" />
            Edit
          </Button>
          <Button variant="destructive" size="sm" onClick={() => setDeleteOpen(true)}>
            <Trash2 data-slot="icon" />
            Delete
          </Button>
        </div>
      </div>

      {event.description && (
        <p className="mt-4 whitespace-pre-line text-muted-foreground">{event.description}</p>
      )}

      <div className="mt-6">
        <ShareLink slug={event.slug} />
      </div>

      <div className="mt-8 mb-4 flex items-center justify-between">
        <h2 className="font-heading text-lg font-semibold">
          {soloDefault ? "When & where" : "Schedule"}
        </h2>
        <Button variant="outline" size="sm" onClick={() => setAddOpen(true)}>
          <Plus data-slot="icon" />
          Add part
        </Button>
      </div>

      <div className="flex flex-col gap-3">
        {parts.map((part) => (
          <PartCard
            key={part.id}
            part={part}
            timezone={event.timezone}
            eventId={event.id}
            soloDefault={soloDefault}
          />
        ))}
      </div>

      <EditEventDialog event={event} open={editOpen} onOpenChange={setEditOpen} />

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={`Delete "${event.title}"?`}
        description="This permanently removes the event and all of its parts. There's no undo."
        confirmLabel="Delete event"
        pending={deleteEvent.isPending}
        onConfirm={() =>
          deleteEvent.mutate(event.id, {
            onSuccess: () => {
              toast.success("Event deleted")
              navigate("/", { replace: true })
            },
            onError: (err) =>
              toast.error(err instanceof ApiError ? err.message : "Couldn't reach the server."),
          })
        }
      />

      <PartDialog
        key={addOpen ? "open" : "closed"}
        open={addOpen}
        onOpenChange={setAddOpen}
        title="Add part"
        initial={emptyPart()}
        pending={addSubEvent.isPending}
        onSubmit={(draft, setError) => {
          addSubEvent.mutate(
            {
              name: draft.name.trim(),
              startsAt: zonedToUtcIso(draft.startsAt, event.timezone),
              endsAt: draft.endsAt ? zonedToUtcIso(draft.endsAt, event.timezone) : undefined,
              venueName: draft.venueName.trim() || undefined,
              venueAddress: draft.venueAddress.trim() || undefined,
            },
            {
              onSuccess: () => {
                toast.success("Part added")
                setAddOpen(false)
              },
              onError: (err) =>
                setError(err instanceof ApiError ? err.message : "Couldn't reach the server."),
            },
          )
        }}
      />
    </div>
  )
}

function PartCard({
  part,
  timezone,
  eventId,
  soloDefault,
}: {
  part: SubEvent
  timezone: string
  eventId: string
  soloDefault: boolean
}) {
  const [editOpen, setEditOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const updateSubEvent = useUpdateSubEvent(eventId)
  const deleteSubEvent = useDeleteSubEvent(eventId)

  const when = part.endsAt
    ? `${formatInZone(part.startsAt, timezone)} – ${formatInZone(part.endsAt, timezone)}`
    : formatInZone(part.startsAt, timezone)

  return (
    <div className="rounded-xl border bg-card p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          {!soloDefault && <h3 className="font-heading font-semibold">{part.name}</h3>}
          <p className={`flex items-center gap-2 text-sm text-muted-foreground ${soloDefault ? "" : "mt-1.5"}`}>
            <CalendarDays className="size-4 shrink-0" aria-hidden />
            {when}
          </p>
          {(part.venueName || part.venueAddress) && (
            <p className="mt-1.5 flex items-center gap-2 text-sm text-muted-foreground">
              <MapPin className="size-4 shrink-0" aria-hidden />
              {[part.venueName, part.venueAddress].filter(Boolean).join(", ")}
            </p>
          )}
        </div>
        <div className="flex shrink-0 gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Edit ${part.name}`}
            onClick={() => setEditOpen(true)}
          >
            <Pencil data-slot="icon" />
          </Button>
          {!soloDefault && (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Delete ${part.name}`}
              onClick={() => setDeleteOpen(true)}
            >
              <Trash2 data-slot="icon" />
            </Button>
          )}
        </div>
      </div>

      <PartDialog
        key={editOpen ? "open" : "closed"}
        open={editOpen}
        onOpenChange={setEditOpen}
        title={soloDefault ? "Edit time & venue" : `Edit ${part.name}`}
        hideName={soloDefault}
        initial={{
          name: part.name,
          startsAt: utcToZonedLocal(part.startsAt, timezone),
          endsAt: part.endsAt ? utcToZonedLocal(part.endsAt, timezone) : "",
          venueName: part.venueName,
          venueAddress: part.venueAddress,
        }}
        pending={updateSubEvent.isPending}
        onSubmit={(draft, setError) => {
          updateSubEvent.mutate(
            {
              id: part.id,
              ...(soloDefault ? {} : { name: draft.name.trim() }),
              startsAt: zonedToUtcIso(draft.startsAt, timezone),
              endsAt: draft.endsAt ? zonedToUtcIso(draft.endsAt, timezone) : null,
              venueName: draft.venueName.trim(),
              venueAddress: draft.venueAddress.trim(),
            },
            {
              onSuccess: () => {
                toast.success("Part updated")
                setEditOpen(false)
              },
              onError: (err) =>
                setError(err instanceof ApiError ? err.message : "Couldn't reach the server."),
            },
          )
        }}
      />

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={`Delete "${part.name}"?`}
        description="Guests already attached to this part will lose it. There's no undo."
        confirmLabel="Delete part"
        pending={deleteSubEvent.isPending}
        onConfirm={() =>
          deleteSubEvent.mutate(part.id, {
            onSuccess: () => {
              toast.success("Part deleted")
              setDeleteOpen(false)
            },
            onError: (err) => {
              setDeleteOpen(false)
              toast.error(err instanceof ApiError ? err.message : "Couldn't reach the server.")
            },
          })
        }
      />
    </div>
  )
}

function EditEventDialog({
  event,
  open,
  onOpenChange,
}: {
  event: EventDetail
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const updateEvent = useUpdateEvent(event.id)
  const [title, setTitle] = useState(event.title)
  const [description, setDescription] = useState(event.description)
  const [timezone, setTimezone] = useState(event.timezone)
  const [coverImageUrl, setCoverImageUrl] = useState(event.coverImageUrl ?? "")
  const [error, setError] = useState<string | null>(null)

  // Re-seed the form each time the dialog opens so stale drafts don't linger.
  const reset = (next: boolean) => {
    if (next) {
      setTitle(event.title)
      setDescription(event.description)
      setTimezone(event.timezone)
      setCoverImageUrl(event.coverImageUrl ?? "")
      setError(null)
    }
    onOpenChange(next)
  }

  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    if (!title.trim()) return setError("Give your event a title.")
    const coverError = coverImageUrlError(coverImageUrl)
    if (coverError) return setError(coverError)
    updateEvent.mutate(
      // An empty coverImageUrl is the API's "clear it" sentinel, which is
      // exactly what emptying the field should mean.
      { title: title.trim(), description: description.trim(), timezone, coverImageUrl: coverImageUrl.trim() },
      {
        onSuccess: () => {
          toast.success("Event updated")
          onOpenChange(false)
        },
        onError: (err) =>
          setError(err instanceof ApiError ? err.message : "Couldn't reach the server."),
      },
    )
  }

  return (
    <Dialog open={open} onOpenChange={reset}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit event</DialogTitle>
          <DialogDescription>Changes show up anywhere this event is shared.</DialogDescription>
        </DialogHeader>
        <form onSubmit={onSubmit} className="flex flex-col gap-4">
          <div className="space-y-2">
            <Label htmlFor="edit-title">Title</Label>
            <Input id="edit-title" value={title} onChange={(e) => setTitle(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="edit-description">Description</Label>
            <Textarea
              id="edit-description"
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="edit-timezone">Timezone</Label>
            <TimezoneSelect id="edit-timezone" value={timezone} onChange={setTimezone} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="edit-cover">
              Cover image URL <span className="text-muted-foreground">(optional)</span>
            </Label>
            <Input
              id="edit-cover"
              type="url"
              inputMode="url"
              placeholder="https://…"
              value={coverImageUrl}
              onChange={(e) => setCoverImageUrl(e.target.value)}
            />
            <p className="text-xs text-muted-foreground">
              Shown across the top of your public page and in WhatsApp link previews.
            </p>
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={updateEvent.isPending}>
              {updateEvent.isPending ? "Saving…" : "Save changes"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function PartDialog({
  open,
  onOpenChange,
  title,
  initial,
  hideName = false,
  pending,
  onSubmit,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  initial: PartDraft
  hideName?: boolean
  pending: boolean
  onSubmit: (draft: PartDraft, setError: (message: string) => void) => void
}) {
  const [draft, setDraft] = useState<PartDraft>(initial)
  const [error, setError] = useState<string | null>(null)

  const submit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    const err = partError(hideName ? { ...draft, name: "•" } : draft, "This part")
    if (err) return setError(err)
    onSubmit(draft, setError)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <PartFields draft={draft} onChange={setDraft} hideName={hideName} idPrefix="dlg" />
          {error && <p className="text-sm text-destructive">{error}</p>}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={pending}>
              {pending ? "Saving…" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  pending,
  onConfirm,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  confirmLabel: string
  pending: boolean
  onConfirm: () => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={pending}>
            {pending ? "Deleting…" : confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
