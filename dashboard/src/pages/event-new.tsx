import { useState, type FormEvent } from "react"
import { Plus, Trash2 } from "lucide-react"
import { Link, useNavigate } from "react-router"
import { toast } from "sonner"

import { PartFields, emptyPart, partError, type PartDraft } from "@/components/part-fields"
import { TimezoneSelect } from "@/components/timezone-select"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Textarea } from "@/components/ui/textarea"
import { ApiError } from "@/lib/api"
import { useCreateEvent } from "@/lib/queries"
import { zonedToUtcIso } from "@/lib/time"
import type { SubEventInput } from "@/lib/types"

function draftToInput(draft: PartDraft, timezone: string, isDefault: boolean): SubEventInput {
  return {
    name: draft.name.trim(),
    startsAt: zonedToUtcIso(draft.startsAt, timezone),
    endsAt: draft.endsAt ? zonedToUtcIso(draft.endsAt, timezone) : undefined,
    venueName: draft.venueName.trim() || undefined,
    venueAddress: draft.venueAddress.trim() || undefined,
    isDefault,
  }
}

export function NewEventPage() {
  const navigate = useNavigate()
  const createEvent = useCreateEvent()

  const [title, setTitle] = useState("")
  const [description, setDescription] = useState("")
  const [timezone, setTimezone] = useState("Africa/Lagos")
  const [multi, setMulti] = useState(false)
  const [single, setSingle] = useState<PartDraft>(emptyPart())
  const [parts, setParts] = useState<PartDraft[]>([emptyPart(), emptyPart()])
  const [error, setError] = useState<string | null>(null)

  const setPart = (i: number, draft: PartDraft) =>
    setParts((prev) => prev.map((p, j) => (j === i ? draft : p)))

  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)

    const trimmedTitle = title.trim()
    if (!trimmedTitle) return setError("Give your event a title.")

    let subEvents: SubEventInput[]
    if (multi) {
      for (const [i, part] of parts.entries()) {
        const err = partError(part, part.name.trim() || `Part ${i + 1}`)
        if (err) return setError(err)
      }
      subEvents = parts.map((p) => draftToInput(p, timezone, false))
    } else {
      const draft = { ...single, name: trimmedTitle }
      const err = partError(draft, "Your event")
      if (err) return setError(err)
      subEvents = [draftToInput(draft, timezone, true)]
    }

    createEvent.mutate(
      {
        title: trimmedTitle,
        description: description.trim() || undefined,
        timezone,
        subEvents,
      },
      {
        onSuccess: (created) => {
          toast.success("Event created")
          navigate(`/events/${created.id}`, { replace: true })
        },
        onError: (err) =>
          setError(err instanceof ApiError ? err.message : "Couldn't reach the server."),
      },
    )
  }

  return (
    <div className="mx-auto max-w-2xl">
      <div className="mb-6">
        <Link to="/" className="text-sm text-muted-foreground hover:text-foreground">
          ← Events
        </Link>
        <h1 className="mt-2 font-heading text-2xl font-semibold">New event</h1>
      </div>

      <form onSubmit={onSubmit} className="flex flex-col gap-6">
        <div className="space-y-2">
          <Label htmlFor="title">Title</Label>
          <Input
            id="title"
            autoFocus
            placeholder="Tolu & Emeka's Wedding"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="description">
            Description <span className="text-muted-foreground">(optional)</span>
          </Label>
          <Textarea
            id="description"
            rows={3}
            placeholder="A note for your guests — shown on the event page."
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="timezone">Timezone</Label>
          <TimezoneSelect id="timezone" value={timezone} onChange={setTimezone} />
        </div>

        <Separator />

        <div className="flex items-start gap-3">
          <input
            id="multi"
            type="checkbox"
            checked={multi}
            onChange={(e) => setMulti(e.target.checked)}
            className="mt-0.5 size-4 accent-[var(--color-gold)]"
          />
          <div>
            <Label htmlFor="multi">This event has multiple parts</Label>
            <p className="mt-1 text-sm text-muted-foreground">
              e.g. traditional engagement, church ceremony, reception — each with its own time
              and venue.
            </p>
          </div>
        </div>

        {multi ? (
          <div className="flex flex-col gap-4">
            {parts.map((part, i) => (
              <div key={i} className="rounded-xl border bg-card p-4">
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="text-sm font-medium text-muted-foreground">Part {i + 1}</h2>
                  {parts.length > 1 && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label={`Remove part ${i + 1}`}
                      onClick={() => setParts((prev) => prev.filter((_, j) => j !== i))}
                    >
                      <Trash2 data-slot="icon" />
                    </Button>
                  )}
                </div>
                <PartFields draft={part} onChange={(d) => setPart(i, d)} idPrefix={`part-${i}`} />
              </div>
            ))}
            {parts.length < 20 && (
              <Button
                type="button"
                variant="outline"
                className="self-start"
                onClick={() => setParts((prev) => [...prev, emptyPart()])}
              >
                <Plus data-slot="icon" />
                Add part
              </Button>
            )}
          </div>
        ) : (
          <PartFields draft={single} onChange={setSingle} hideName idPrefix="single" />
        )}

        {error && <p className="text-sm text-destructive">{error}</p>}

        <div className="flex items-center gap-3">
          <Button type="submit" disabled={createEvent.isPending}>
            {createEvent.isPending ? "Creating…" : "Create event"}
          </Button>
          <Button asChild type="button" variant="ghost">
            <Link to="/">Cancel</Link>
          </Button>
        </div>
      </form>
    </div>
  )
}
