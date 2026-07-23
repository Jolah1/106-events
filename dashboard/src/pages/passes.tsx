/**
 * Every head on the guest list, with the code that admits them.
 *
 * Guests normally see their own passes on their RSVP link; this is the
 * organizer's copy — for printing, for reading a code out over the phone, and
 * for spotting who hasn't been issued one yet.
 */

import { useMemo, useState } from "react"
import { ArrowLeft, Printer, QrCode, RefreshCw, Search } from "lucide-react"
import { Link, useParams } from "react-router"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import { useAttendees, useConfig, useEvent, useSyncAttendees } from "@/lib/queries"

/** Grouped the way it reads: the code in two halves of four. */
function spoken(code: string): string {
  return `${code.slice(0, 4)} ${code.slice(4)}`
}

export function PassesPage() {
  const { id } = useParams<{ id: string }>()
  const detail = useEvent(id!)
  const attendees = useAttendees(id!)
  const config = useConfig()
  const sync = useSyncAttendees(id!)
  const [query, setQuery] = useState("")

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const all = attendees.data ?? []
    if (needle === "") return all
    return all.filter(
      (a) =>
        a.label.toLowerCase().includes(needle) || a.code.toLowerCase().includes(needle),
    )
  }, [attendees.data, query])

  if (detail.isPending || attendees.isPending) {
    return (
      <div className="mx-auto max-w-3xl">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="mt-8 h-16 w-full rounded-xl" />
        <Skeleton className="mt-4 h-24 w-full rounded-xl" />
      </div>
    )
  }

  if (detail.isError || attendees.isError) {
    const error = detail.error ?? attendees.error
    const notFound = error instanceof ApiError && error.status === 404
    return (
      <div className="py-16 text-center">
        <p className="text-muted-foreground">
          {notFound ? "That event doesn't exist." : "Couldn't load the passes."}
        </p>
        <Button asChild variant="outline" className="mt-4">
          <Link to="/events">Back to events</Link>
        </Button>
      </div>
    )
  }

  const all = attendees.data ?? []
  const extras = all.filter((a) => a.isExtra).length

  return (
    <div className="mx-auto max-w-3xl pb-16">
      <Link
        to={`/events/${id}`}
        className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-sm print:hidden"
      >
        <ArrowLeft className="size-4" />
        {detail.data?.title}
      </Link>

      <div className="mt-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Passes</h1>
          <p className="text-muted-foreground mt-1 text-sm">
            {all.length} {all.length === 1 ? "pass" : "passes"}
            {extras > 0 && ` · ${extras} added at the door`}
          </p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() =>
              sync.mutate(undefined, {
                onSuccess: (report) =>
                  toast.success(
                    report.created === 0
                      ? "Everyone already has a pass"
                      : `Issued ${report.created} new ${report.created === 1 ? "pass" : "passes"}`,
                  ),
                onError: () => toast.error("Couldn't issue passes"),
              })
            }
            disabled={sync.isPending}
          >
            <RefreshCw className={sync.isPending ? "size-4 animate-spin" : "size-4"} />
            Issue passes
          </Button>
          <Button variant="outline" onClick={() => window.print()}>
            <Printer className="size-4" />
            Print
          </Button>
        </div>
      </div>

      {all.length === 0 ? (
        <div className="mt-10 rounded-xl border border-dashed py-16 text-center">
          <QrCode className="text-muted-foreground mx-auto size-8" />
          <p className="mt-3 font-medium">No passes yet</p>
          <p className="text-muted-foreground mx-auto mt-1 max-w-sm text-sm">
            Guests get theirs automatically when they RSVP. Issue them up front if
            you'd rather send codes out with the invitations.
          </p>
        </div>
      ) : (
        <>
          <div className="relative mt-6 print:hidden">
            <Search className="text-muted-foreground absolute top-1/2 left-3 size-4 -translate-y-1/2" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search a name or code"
              className="pl-9"
            />
          </div>

          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            {matches.map((attendee) => (
              <div
                key={attendee.id}
                className="flex items-center gap-4 rounded-xl border p-4 break-inside-avoid"
              >
                {/* White plate behind the square: scanners need the contrast. */}
                <span className="shrink-0 rounded-lg bg-white p-1.5">
                  <img
                    src={`${config.data?.publicBaseUrl ?? ""}/q/${attendee.code}`}
                    alt={`Entry code ${spoken(attendee.code)}`}
                    width={72}
                    height={72}
                    className="block size-[72px]"
                  />
                </span>
                <div className="min-w-0">
                  <p className="truncate font-medium">{attendee.label}</p>
                  <p className="mt-1 font-mono text-sm tracking-widest">
                    {spoken(attendee.code)}
                  </p>
                  {attendee.isExtra && (
                    <Badge variant="outline" className="mt-1.5 border-amber-500/40 text-amber-300">
                      added at the door
                    </Badge>
                  )}
                </div>
              </div>
            ))}
          </div>

          {matches.length === 0 && (
            <p className="text-muted-foreground mt-8 text-center text-sm">
              Nothing matches “{query}”.
            </p>
          )}
        </>
      )}
    </div>
  )
}
