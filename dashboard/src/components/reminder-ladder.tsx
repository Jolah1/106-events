import { useState } from "react"
import { AnimatePresence, motion } from "motion/react"
import { BellRing, Check, Plus, Trash2, TriangleAlert } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import { useAddReminder, useDeleteReminder, useReminders } from "@/lib/queries"

const DAY = 24 * 60

/** The rungs organizers actually reach for, so the common case is one tap. */
const PRESETS = [
  { label: "2 weeks before", minutes: 14 * DAY },
  { label: "1 week before", minutes: 7 * DAY },
  { label: "3 days before", minutes: 3 * DAY },
  { label: "The day before", minutes: 1 * DAY },
  { label: "Morning of", minutes: 4 * 60 },
]

/** Renders an offset the way the presets read, including custom values. */
export function describeOffset(minutes: number): string {
  const preset = PRESETS.find((p) => p.minutes === minutes)
  if (preset) return preset.label
  if (minutes % DAY === 0) {
    const days = minutes / DAY
    return `${days} day${days === 1 ? "" : "s"} before`
  }
  const hours = Math.round(minutes / 60)
  return `${hours} hour${hours === 1 ? "" : "s"} before`
}

export function ReminderLadder({ eventId }: { eventId: string }) {
  const reminders = useReminders(eventId)
  const add = useAddReminder(eventId)
  const remove = useDeleteReminder(eventId)
  const [pendingOffset, setPendingOffset] = useState<number | null>(null)

  const scheduled = reminders.data ?? []
  const taken = new Set(scheduled.map((r) => r.offsetMinutes))
  const available = PRESETS.filter((p) => !taken.has(p.minutes))

  async function addRung(minutes: number) {
    setPendingOffset(minutes)
    try {
      await add.mutateAsync(minutes)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : "Couldn't add that reminder",
      )
    } finally {
      setPendingOffset(null)
    }
  }

  async function removeRung(id: string, label: string) {
    try {
      await remove.mutateAsync(id)
      toast.success(`Removed "${label}"`)
    } catch {
      toast.error("Couldn't remove that reminder")
    }
  }

  return (
    <section className="mt-8">
      <div className="flex items-center gap-2">
        <BellRing className="size-4 text-muted-foreground" />
        <h2 className="font-heading text-lg font-semibold">Reminders</h2>
      </div>
      <p className="mt-1 text-sm text-muted-foreground">
        Guests who haven't replied get a WhatsApp or SMS nudge with their own RSVP
        link. Timed from the first part, so moving the date moves these too.
      </p>

      {reminders.isPending ? (
        <div className="mt-4 space-y-2">
          <Skeleton className="h-14 w-full rounded-lg" />
          <Skeleton className="h-14 w-full rounded-lg" />
        </div>
      ) : (
        <ul className="mt-4 space-y-2">
          <AnimatePresence initial={false}>
            {scheduled.map((rung) => {
              const label = describeOffset(rung.offsetMinutes)
              const fired = rung.sentCount > 0 || rung.failedCount > 0
              return (
                <motion.li
                  key={rung.id}
                  layout
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, height: 0 }}
                  className="flex items-center justify-between gap-3 rounded-lg border bg-card px-4 py-3"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium">{label}</p>
                    <p className="mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                      {fired ? (
                        <>
                          <Check className="size-3 text-emerald-500" />
                          Sent to {rung.sentCount}
                          {rung.failedCount > 0 && (
                            <span className="flex items-center gap-1 text-amber-600 dark:text-amber-500">
                              <TriangleAlert className="size-3" />
                              {rung.failedCount} failed
                            </span>
                          )}
                        </>
                      ) : (
                        "Not sent yet"
                      )}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Remove ${label}`}
                    onClick={() => removeRung(rung.id, label)}
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </motion.li>
              )
            })}
          </AnimatePresence>
        </ul>
      )}

      {scheduled.length === 0 && !reminders.isPending && (
        <p className="mt-2 text-sm text-muted-foreground">
          No reminders scheduled — nobody will be chased automatically.
        </p>
      )}

      {available.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {available.map((preset) => (
            <Button
              key={preset.minutes}
              variant="outline"
              size="sm"
              disabled={pendingOffset !== null}
              onClick={() => addRung(preset.minutes)}
            >
              <Plus className="size-3.5" />
              {preset.label}
            </Button>
          ))}
        </div>
      )}
    </section>
  )
}
