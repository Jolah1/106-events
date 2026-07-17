import { useEffect, useState, type FormEvent } from "react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { Textarea } from "@/components/ui/textarea"
import { ApiError } from "@/lib/api"
import {
  guestEmailError,
  guestNameError,
  guestPhoneError,
  plusOnesError,
} from "@/lib/validation"
import type { Guest, SubEvent } from "@/lib/types"

export interface GuestDraft {
  name: string
  phone: string
  email: string
  plusOnes: string
  dietary: string
  notes: string
  subEventIds: string[]
}

export function emptyGuest(subEventIds: string[] = []): GuestDraft {
  return { name: "", phone: "", email: "", plusOnes: "", dietary: "", notes: "", subEventIds }
}

function draftOf(guest: Guest): GuestDraft {
  return {
    name: guest.name,
    phone: guest.phone ?? "",
    email: guest.email ?? "",
    // A zero is worth showing as blank: the field means "and how many more?".
    plusOnes: guest.plusOnes ? String(guest.plusOnes) : "",
    dietary: guest.dietary,
    notes: guest.notes,
    subEventIds: guest.subEventIds,
  }
}

/** The first error in the order the fields appear, so focus follows reading. */
function draftError(draft: GuestDraft): string | null {
  return (
    guestNameError(draft.name) ??
    guestPhoneError(draft.phone) ??
    guestEmailError(draft.email) ??
    plusOnesError(draft.plusOnes)
  )
}

export function PartCheckboxes({
  parts,
  selected,
  onChange,
  idPrefix,
}: {
  parts: SubEvent[]
  selected: string[]
  onChange: (ids: string[]) => void
  idPrefix: string
}) {
  return (
    <div className="flex flex-col gap-2">
      {parts.map((part) => (
        <label
          key={part.id}
          htmlFor={`${idPrefix}-${part.id}`}
          className="flex cursor-pointer items-center gap-2.5 text-sm"
        >
          <Checkbox
            id={`${idPrefix}-${part.id}`}
            checked={selected.includes(part.id)}
            onCheckedChange={(checked) =>
              onChange(
                checked
                  ? [...selected, part.id]
                  : selected.filter((id) => id !== part.id),
              )
            }
          />
          {part.name}
        </label>
      ))}
    </div>
  )
}

export function GuestDialog({
  guest,
  parts,
  soloDefault,
  open,
  onOpenChange,
  onSubmit,
  pending,
}: {
  /** Absent when adding rather than editing. */
  guest?: Guest
  parts: SubEvent[]
  soloDefault: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: (draft: GuestDraft) => Promise<unknown>
  pending: boolean
}) {
  const [draft, setDraft] = useState<GuestDraft>(emptyGuest())
  const [error, setError] = useState<string | null>(null)

  // Reopening must show the guest as they are now, not as they were when the
  // dialog last closed.
  useEffect(() => {
    if (!open) return
    setDraft(guest ? draftOf(guest) : emptyGuest(soloDefault ? parts.map((p) => p.id) : []))
    setError(null)
  }, [open, guest, parts, soloDefault])

  const set = <K extends keyof GuestDraft>(key: K, value: GuestDraft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  async function submit(event: FormEvent) {
    event.preventDefault()
    const problem = draftError(draft)
    if (problem) return setError(problem)
    try {
      await onSubmit(draft)
      onOpenChange(false)
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Couldn't save this guest. Try again.",
      )
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-lg">
        <form onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{guest ? "Edit guest" : "Add guest"}</DialogTitle>
            <DialogDescription>
              A phone number is what lets them RSVP over WhatsApp.
            </DialogDescription>
          </DialogHeader>

          <div className="mt-5 flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="guest-name">Name</Label>
              <Input
                id="guest-name"
                value={draft.name}
                onChange={(e) => set("name", e.target.value)}
                placeholder="Adaeze Okafor"
                autoFocus
              />
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              <div className="flex flex-col gap-2">
                <Label htmlFor="guest-phone">Phone</Label>
                <Input
                  id="guest-phone"
                  type="tel"
                  inputMode="tel"
                  value={draft.phone}
                  onChange={(e) => set("phone", e.target.value)}
                  placeholder="0806 688 2563"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="guest-plus-ones">Plus-ones</Label>
                <Input
                  id="guest-plus-ones"
                  inputMode="numeric"
                  value={draft.plusOnes}
                  onChange={(e) => set("plusOnes", e.target.value)}
                  placeholder="0"
                />
              </div>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="guest-email">Email</Label>
              <Input
                id="guest-email"
                type="email"
                value={draft.email}
                onChange={(e) => set("email", e.target.value)}
                placeholder="ada@example.com"
              />
            </div>

            {!soloDefault && (
              <div className="flex flex-col gap-2.5">
                <Label>Invited to</Label>
                <PartCheckboxes
                  parts={parts}
                  selected={draft.subEventIds}
                  onChange={(ids) => set("subEventIds", ids)}
                  idPrefix="guest-part"
                />
              </div>
            )}

            <div className="flex flex-col gap-2">
              <Label htmlFor="guest-dietary">Dietary needs</Label>
              <Input
                id="guest-dietary"
                value={draft.dietary}
                onChange={(e) => set("dietary", e.target.value)}
                placeholder="Vegetarian"
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="guest-notes">Notes</Label>
              <Textarea
                id="guest-notes"
                rows={2}
                value={draft.notes}
                onChange={(e) => set("notes", e.target.value)}
                placeholder="Bride's cousin — sit with family"
              />
            </div>

            {error && <p className="text-sm text-destructive">{error}</p>}
          </div>

          <DialogFooter className="mt-6">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={pending}>
              {pending ? "Saving…" : guest ? "Save changes" : "Add guest"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
