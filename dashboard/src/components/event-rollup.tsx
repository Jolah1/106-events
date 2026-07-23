import { ScanLine } from "lucide-react"

import { Skeleton } from "@/components/ui/skeleton"
import { formatNaira } from "@/lib/money"
import { useEventStats } from "@/lib/queries"
import type { PartStats } from "@/lib/types"

/**
 * The "at a glance" block on the event page: RSVP standings, the door count,
 * and what's still owed to vendors. Every number is derived server-side from
 * the same rows the other pages edit, so this never disagrees with them.
 */
export function EventRollup({ eventId, soloDefault }: { eventId: string; soloDefault: boolean }) {
  const stats = useEventStats(eventId)

  if (stats.isPending) {
    return (
      <div className="mt-8">
        <Skeleton className="h-16 w-full rounded-xl" />
      </div>
    )
  }
  // The rollup is a convenience, not a control: when it can't load, the page
  // it sits on still works, so a quiet line beats an error wall.
  if (stats.isError) {
    return (
      <p className="mt-8 text-sm text-muted-foreground">
        Couldn't load the numbers.{" "}
        <button className="underline hover:text-foreground" onClick={() => stats.refetch()}>
          Try again
        </button>
      </p>
    )
  }

  const s = stats.data
  const tiles = [
    {
      label: "Guests",
      value: String(s.guestCount),
      sub: s.guestCount > 0 ? `up to ${s.headsInvited} heads` : "none invited yet",
    },
    { label: "Replied", value: String(s.repliedGuests), sub: null },
    { label: "Still to answer", value: String(s.awaitingGuests), sub: null },
  ]
  if (s.vendorCount > 0) {
    tiles.push({
      label: "Owed to vendors",
      value: formatNaira(s.vendorOutstandingKobo),
      sub: `of ${formatNaira(s.vendorCostKobo)} budgeted`,
    })
  }

  return (
    <section className="mt-8">
      <h2 className="mb-4 font-heading text-lg font-semibold">At a glance</h2>
      <div className={`grid gap-3 ${tiles.length === 4 ? "grid-cols-2 sm:grid-cols-4" : "grid-cols-3"}`}>
        {tiles.map((tile) => (
          <div key={tile.label} className="rounded-xl border bg-card px-4 py-3">
            <p className="text-xs text-muted-foreground">{tile.label}</p>
            <p className="mt-1 font-heading text-lg font-semibold tabular-nums">{tile.value}</p>
            {tile.sub && <p className="text-xs text-muted-foreground">{tile.sub}</p>}
          </div>
        ))}
      </div>

      <div className="mt-3 flex flex-col gap-2">
        {s.parts.map((part) => (
          <PartRow key={part.subEventId} part={part} soloDefault={soloDefault} />
        ))}
      </div>
    </section>
  )
}

function PartRow({ part, soloDefault }: { part: PartStats; soloDefault: boolean }) {
  const standing =
    part.invitedParties === 0
      ? "Nobody invited yet"
      : [
          `${part.confirmedHeads} coming`,
          `${part.pendingParties} to answer`,
          `${part.declinedParties} declined`,
        ].join(" · ")

  // The override and offline counts only matter once they're nonzero; a row
  // of reassuring zeros would bury the one door where something happened.
  const doorNotes = [
    part.overAllowanceHeads > 0 && `${part.overAllowanceHeads} over allowance`,
    part.offlineSyncedHeads > 0 && `${part.offlineSyncedHeads} synced offline`,
  ].filter(Boolean)

  return (
    <div className="flex items-center justify-between gap-3 rounded-xl border bg-card px-4 py-3">
      <div className="min-w-0">
        {!soloDefault && <p className="truncate text-sm font-medium">{part.name}</p>}
        <p className="text-sm text-muted-foreground">{standing}</p>
      </div>
      {part.checkedInHeads > 0 && (
        <div className="shrink-0 text-right">
          <p className="flex items-center justify-end gap-1.5 text-sm font-medium tabular-nums">
            <ScanLine className="size-4 text-muted-foreground" aria-hidden />
            {part.checkedInHeads} through the door
          </p>
          {doorNotes.length > 0 && (
            <p className="text-xs text-muted-foreground">{doorNotes.join(" · ")}</p>
          )}
        </div>
      )}
    </div>
  )
}
