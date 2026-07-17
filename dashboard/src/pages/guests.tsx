import { useMemo, useState, type ReactNode } from "react"
import { Mail, Pencil, Phone, Plus, Search, Trash2, Upload, Users } from "lucide-react"
import { AnimatePresence, motion } from "motion/react"
import { Link, useParams } from "react-router"
import { toast } from "sonner"

import { GuestDialog, type GuestDraft } from "@/components/guest-dialog"
import { ImportGuestsDialog } from "@/components/import-guests-dialog"
import { Badge } from "@/components/ui/badge"
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
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import { useCreateGuest, useDeleteGuest, useEvent, useGuests, useUpdateGuest } from "@/lib/queries"
import type { EventDetail, Guest, SubEvent } from "@/lib/types"

/** Everyone who would walk through the door: each guest, plus their plus-ones. */
function headcount(guests: Guest[]) {
  return guests.reduce((total, guest) => total + 1 + guest.plusOnes, 0)
}

function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`
}

/** Turns the dialog's text fields into the API's typed shape. */
function toInput(draft: GuestDraft) {
  return {
    name: draft.name.trim(),
    phone: draft.phone.trim(),
    email: draft.email.trim(),
    plusOnes: draft.plusOnes.trim() ? Number(draft.plusOnes) : 0,
    dietary: draft.dietary.trim(),
    notes: draft.notes.trim(),
    subEventIds: draft.subEventIds,
  }
}

export function GuestsPage() {
  const { id } = useParams<{ id: string }>()
  const detail = useEvent(id!)
  const guests = useGuests(id!)

  if (detail.isPending || guests.isPending) {
    return (
      <div className="mx-auto max-w-3xl">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="mt-3 h-4 w-56" />
        <Skeleton className="mt-8 h-10 w-full rounded-lg" />
        <Skeleton className="mt-4 h-20 w-full rounded-xl" />
        <Skeleton className="mt-3 h-20 w-full rounded-xl" />
      </div>
    )
  }

  if (detail.isError || guests.isError) {
    const error = detail.error ?? guests.error
    const notFound = error instanceof ApiError && error.status === 404
    return (
      <div className="flex flex-col items-center gap-4 py-16 text-center">
        <p className="text-muted-foreground">
          {notFound ? "This event doesn't exist or isn't yours." : "Couldn't load the guest list."}
        </p>
        <Button asChild variant="outline">
          <Link to="/">Back to events</Link>
        </Button>
      </div>
    )
  }

  return <GuestsView event={detail.data} guests={guests.data} />
}

function GuestsView({ event, guests }: { event: EventDetail; guests: Guest[] }) {
  const [query, setQuery] = useState("")
  const [partFilter, setPartFilter] = useState<string | null>(null)
  const [addOpen, setAddOpen] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  const [editing, setEditing] = useState<Guest | null>(null)
  const [deleting, setDeleting] = useState<Guest | null>(null)

  const createGuest = useCreateGuest(event.id)
  const updateGuest = useUpdateGuest(event.id)
  const deleteGuest = useDeleteGuest(event.id)

  const parts = useMemo(
    () => [...event.subEvents].sort((a, b) => a.position - b.position),
    [event.subEvents],
  )
  const soloDefault = parts.length === 1 && parts[0].isDefault

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return guests.filter((guest) => {
      if (partFilter && !guest.subEventIds.includes(partFilter)) return false
      if (!needle) return true
      return [guest.name, guest.phone, guest.email, guest.dietary, guest.notes]
        .join(" ")
        .toLowerCase()
        .includes(needle)
    })
  }, [guests, query, partFilter])

  const filtered = shown.length !== guests.length

  return (
    <div className="mx-auto max-w-3xl">
      <Link
        to={`/events/${event.id}`}
        className="text-sm text-muted-foreground hover:text-foreground"
      >
        ← {event.title}
      </Link>

      <div className="mt-2 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="font-heading text-3xl font-semibold leading-tight">Guests</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            {guests.length === 0
              ? "Nobody invited yet"
              : `${plural(guests.length, "guest")} · ${headcount(guests)} expected with plus-ones`}
          </p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
            <Upload data-slot="icon" />
            Import CSV
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <Plus data-slot="icon" />
            Add guest
          </Button>
        </div>
      </div>

      {guests.length === 0 ? (
        <EmptyState onImport={() => setImportOpen(true)} onAdd={() => setAddOpen(true)} />
      ) : (
        <>
          <div className="mt-8 flex flex-col gap-3">
            <div className="relative">
              <Search
                data-slot="icon"
                className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search by name, phone or note"
                className="pl-9"
                aria-label="Search guests"
              />
            </div>

            {!soloDefault && (
              <div className="flex flex-wrap gap-2">
                <FilterChip active={partFilter === null} onClick={() => setPartFilter(null)}>
                  Everyone
                </FilterChip>
                {parts.map((part) => (
                  <FilterChip
                    key={part.id}
                    active={partFilter === part.id}
                    onClick={() => setPartFilter(partFilter === part.id ? null : part.id)}
                  >
                    {part.name}
                    <span className="ml-1.5 text-muted-foreground">
                      {guests.filter((g) => g.subEventIds.includes(part.id)).length}
                    </span>
                  </FilterChip>
                ))}
              </div>
            )}
          </div>

          {filtered && (
            <p className="mt-4 text-sm text-muted-foreground">
              {shown.length === 0
                ? "No guests match."
                : `${plural(shown.length, "guest")} · ${headcount(shown)} with plus-ones`}
            </p>
          )}

          <div className="mt-4 flex flex-col gap-3">
            <AnimatePresence initial={false}>
              {shown.map((guest) => (
                <motion.div
                  key={guest.id}
                  layout
                  initial={{ opacity: 0, y: 4 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, scale: 0.97 }}
                  transition={{ duration: 0.16 }}
                >
                  <GuestCard
                    guest={guest}
                    parts={parts}
                    soloDefault={soloDefault}
                    onEdit={() => setEditing(guest)}
                    onDelete={() => setDeleting(guest)}
                  />
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        </>
      )}

      <GuestDialog
        parts={parts}
        soloDefault={soloDefault}
        open={addOpen}
        onOpenChange={setAddOpen}
        pending={createGuest.isPending}
        onSubmit={(draft) => createGuest.mutateAsync(toInput(draft))}
      />

      <GuestDialog
        guest={editing ?? undefined}
        parts={parts}
        soloDefault={soloDefault}
        open={editing !== null}
        onOpenChange={(open) => !open && setEditing(null)}
        pending={updateGuest.isPending}
        onSubmit={(draft) => updateGuest.mutateAsync({ id: editing!.id, ...toInput(draft) })}
      />

      <ImportGuestsDialog
        eventId={event.id}
        parts={parts}
        soloDefault={soloDefault}
        open={importOpen}
        onOpenChange={setImportOpen}
      />

      <Dialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Remove {deleting?.name}?</DialogTitle>
            <DialogDescription>
              They'll be taken off the guest list. This can't be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-6">
            <Button variant="ghost" onClick={() => setDeleting(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const guest = deleting!
                setDeleting(null)
                deleteGuest.mutate(guest.id, {
                  onError: () => toast.error(`Couldn't remove ${guest.name}.`),
                })
              }}
            >
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`rounded-full border px-3 py-1 text-sm transition-colors ${
        active
          ? "border-primary/40 bg-primary/10 text-foreground"
          : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {children}
    </button>
  )
}

function GuestCard({
  guest,
  parts,
  soloDefault,
  onEdit,
  onDelete,
}: {
  guest: Guest
  parts: SubEvent[]
  soloDefault: boolean
  onEdit: () => void
  onDelete: () => void
}) {
  const invited = parts.filter((part) => guest.subEventIds.includes(part.id))

  return (
    <div className="group rounded-xl border p-4 transition-colors hover:border-primary/30">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="font-medium">{guest.name}</h3>
            {guest.plusOnes > 0 && <Badge variant="secondary">+{guest.plusOnes}</Badge>}
          </div>

          <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
            {guest.phone && (
              <span className="inline-flex items-center gap-1.5">
                <Phone data-slot="icon" className="size-3.5" />
                {guest.phone}
              </span>
            )}
            {guest.email && (
              <span className="inline-flex min-w-0 items-center gap-1.5">
                <Mail data-slot="icon" className="size-3.5 shrink-0" />
                <span className="truncate">{guest.email}</span>
              </span>
            )}
          </div>
        </div>

        {/* Always reachable on touch, where there is no hover to reveal them. */}
        <div className="flex shrink-0 gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100">
          <Button variant="ghost" size="icon" onClick={onEdit} aria-label={`Edit ${guest.name}`}>
            <Pencil data-slot="icon" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            onClick={onDelete}
            aria-label={`Remove ${guest.name}`}
          >
            <Trash2 data-slot="icon" />
          </Button>
        </div>
      </div>

      {!soloDefault && invited.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {invited.map((part) => (
            <Badge key={part.id} variant="outline">
              {part.name}
            </Badge>
          ))}
        </div>
      )}

      {(guest.dietary || guest.notes) && (
        <p className="mt-3 text-sm text-muted-foreground">
          {[guest.dietary, guest.notes].filter(Boolean).join(" · ")}
        </p>
      )}
    </div>
  )
}

function EmptyState({ onImport, onAdd }: { onImport: () => void; onAdd: () => void }) {
  return (
    <div className="mt-10 flex flex-col items-center gap-5 rounded-xl border border-dashed py-16 text-center">
      <Users data-slot="icon" className="size-8 text-muted-foreground" />
      <div>
        <p className="font-heading text-lg">Nobody on the list yet.</p>
        <p className="mt-1 text-sm text-muted-foreground">
          Import the spreadsheet you already have, or add guests one by one.
        </p>
      </div>
      <div className="flex gap-2">
        <Button variant="outline" onClick={onImport}>
          <Upload data-slot="icon" />
          Import CSV
        </Button>
        <Button onClick={onAdd}>
          <Plus data-slot="icon" />
          Add guest
        </Button>
      </div>
    </div>
  )
}
