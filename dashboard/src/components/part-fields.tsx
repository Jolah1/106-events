import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/** Form draft for one part (sub-event); datetimes are datetime-local strings. */
export interface PartDraft {
  name: string
  startsAt: string
  endsAt: string
  venueName: string
  venueAddress: string
}

export function emptyPart(): PartDraft {
  return { name: "", startsAt: "", endsAt: "", venueName: "", venueAddress: "" }
}

/** Returns a user-facing validation error, or null when the draft is valid. */
export function partError(draft: PartDraft, label: string): string | null {
  if (!draft.name.trim()) return `${label} needs a name.`
  if (!draft.startsAt) return `${label} needs a start time.`
  if (draft.endsAt && draft.endsAt <= draft.startsAt) return `${label} must end after it starts.`
  return null
}

export function PartFields({
  draft,
  onChange,
  hideName = false,
  idPrefix,
}: {
  draft: PartDraft
  onChange: (draft: PartDraft) => void
  hideName?: boolean
  idPrefix: string
}) {
  const set = (patch: Partial<PartDraft>) => onChange({ ...draft, ...patch })

  return (
    <div className="grid gap-4">
      {!hideName && (
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-name`}>Name</Label>
          <Input
            id={`${idPrefix}-name`}
            placeholder="Ceremony, Reception, After party…"
            value={draft.name}
            onChange={(e) => set({ name: e.target.value })}
          />
        </div>
      )}
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-starts`}>Starts</Label>
          <Input
            id={`${idPrefix}-starts`}
            type="datetime-local"
            value={draft.startsAt}
            onChange={(e) => set({ startsAt: e.target.value })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-ends`}>
            Ends <span className="text-muted-foreground">(optional)</span>
          </Label>
          <Input
            id={`${idPrefix}-ends`}
            type="datetime-local"
            value={draft.endsAt}
            onChange={(e) => set({ endsAt: e.target.value })}
          />
        </div>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-venue`}>Venue</Label>
          <Input
            id={`${idPrefix}-venue`}
            placeholder="The Monarch Event Centre"
            value={draft.venueName}
            onChange={(e) => set({ venueName: e.target.value })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-address`}>Address</Label>
          <Input
            id={`${idPrefix}-address`}
            placeholder="Lekki-Epe Expressway, Lagos"
            value={draft.venueAddress}
            onChange={(e) => set({ venueAddress: e.target.value })}
          />
        </div>
      </div>
    </div>
  )
}
