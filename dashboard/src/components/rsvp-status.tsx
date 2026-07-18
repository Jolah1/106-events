import { Check, Clock, Link2, MinusCircle, X } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { useConfig } from "@/lib/queries"
import type { Guest, RsvpStatus } from "@/lib/types"

const META: Record<RsvpStatus, { label: string; className: string; Icon: typeof Check }> = {
  confirmed: {
    label: "Coming",
    className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-400",
    Icon: Check,
  },
  declined: {
    label: "Can't come",
    className: "border-muted-foreground/25 bg-muted/40 text-muted-foreground",
    Icon: X,
  },
  partial: {
    label: "Some parts",
    className: "border-primary/30 bg-primary/10 text-primary",
    Icon: MinusCircle,
  },
  pending: {
    label: "Awaiting",
    className: "border-amber-500/25 bg-amber-500/10 text-amber-400",
    Icon: Clock,
  },
}

export function RsvpBadge({ status }: { status: RsvpStatus }) {
  const { label, className, Icon } = META[status]
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium ${className}`}
    >
      <Icon className="size-3" />
      {label}
    </span>
  )
}

/** Counts guests in each RSVP state, for the summary strip. */
export function rsvpBreakdown(guests: Guest[]) {
  const counts = { confirmed: 0, declined: 0, partial: 0, pending: 0 }
  for (const guest of guests) counts[guest.rsvpStatus]++
  return counts
}

/** Copies a guest's public RSVP link. The organizer pastes it into WhatsApp. */
export function CopyRsvpLink({ guest }: { guest: Guest }) {
  const config = useConfig()

  async function copy() {
    if (!config.data) return
    const url = `${config.data.publicBaseUrl}/r/${guest.rsvpToken}`
    try {
      await navigator.clipboard.writeText(url)
      toast.success("RSVP link copied", { description: url })
    } catch {
      toast.error("Couldn't copy — long-press the link to copy it manually.", {
        description: url,
      })
    }
  }

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={copy}
      disabled={!config.data}
      aria-label={`Copy RSVP link for ${guest.name}`}
    >
      <Link2 data-slot="icon" />
    </Button>
  )
}
