/**
 * The door.
 *
 * Held in one hand, in a marquee, on a phone whose signal comes and goes. That
 * shapes everything here: the manifest is cached before doors open, scans are
 * written locally before they're sent, and nothing on this screen ever blocks
 * on the network.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  ArrowLeft,
  Check,
  CloudOff,
  Keyboard,
  RefreshCw,
  TriangleAlert,
  UserCheck,
  X,
} from "lucide-react"
import { AnimatePresence, motion } from "motion/react"
import { Link, useParams, useSearchParams } from "react-router"
import { toast } from "sonner"

import { QrScanner, isScanningSupported } from "@/components/qr-scanner"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import {
  cacheManifest,
  flushQueue,
  isPlausibleCode,
  judgeLocally,
  loadCachedManifest,
  loadQueue,
  normalizeCode,
  saveQueue,
  sendScan,
  type QueuedScan,
} from "@/lib/door"
import { useCheckIns, useDoorManifest, useEvent } from "@/lib/queries"
import type { CheckInResult, DoorManifest } from "@/lib/types"

/** How long a result stays on screen before the scanner starts looking again. */
const RESULT_MS = 2200

const OUTCOME: Record<
  CheckInResult["outcome"],
  { title: string; tone: string; icon: typeof Check }
> = {
  admitted: {
    title: "Come in",
    tone: "border-emerald-500/40 bg-emerald-500/10 text-emerald-200",
    icon: Check,
  },
  already_in: {
    title: "Already inside",
    tone: "border-sky-500/40 bg-sky-500/10 text-sky-200",
    icon: UserCheck,
  },
  over_allowance: {
    title: "More than they confirmed",
    tone: "border-amber-500/40 bg-amber-500/10 text-amber-200",
    icon: TriangleAlert,
  },
  not_invited: {
    title: "Not on this list",
    tone: "border-red-500/40 bg-red-500/10 text-red-200",
    icon: X,
  },
  unknown_code: {
    title: "Code not recognised",
    tone: "border-red-500/40 bg-red-500/10 text-red-200",
    icon: X,
  },
}

export function DoorPage() {
  const { id } = useParams<{ id: string }>()
  const [params, setParams] = useSearchParams()
  const detail = useEvent(id!)

  // The part being worked. Kept in the URL so a staff member can be sent
  // straight to the right door by a link.
  const partId = params.get("part") ?? ""
  useEffect(() => {
    if (partId === "" && detail.data && detail.data.subEvents.length > 0) {
      setParams({ part: detail.data.subEvents[0].id }, { replace: true })
    }
  }, [partId, detail.data, setParams])

  if (detail.isPending) {
    return (
      <div className="mx-auto max-w-md">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="mt-6 aspect-square w-full rounded-xl" />
      </div>
    )
  }

  const parts = detail.data?.subEvents ?? []

  return (
    <div className="mx-auto max-w-md pb-16">
      <Link
        to={`/events/${id}`}
        className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-sm"
      >
        <ArrowLeft className="size-4" />
        {detail.data?.title}
      </Link>

      {parts.length > 1 && (
        <div className="mt-4 flex flex-wrap gap-2">
          {parts.map((part) => (
            <Button
              key={part.id}
              size="sm"
              variant={part.id === partId ? "default" : "outline"}
              onClick={() => setParams({ part: part.id }, { replace: true })}
            >
              {part.name}
            </Button>
          ))}
        </div>
      )}

      {partId !== "" && <Door key={partId} subEventId={partId} />}
    </div>
  )
}

function Door({ subEventId }: { subEventId: string }) {
  const manifestQuery = useDoorManifest(subEventId)
  const checkIns = useCheckIns(subEventId)

  // Falls back to whatever was cached last time, so opening the door screen in
  // a basement with no bars still gives staff a working list.
  const [manifest, setManifest] = useState<DoorManifest | null>(() =>
    loadCachedManifest(subEventId),
  )
  useEffect(() => {
    if (manifestQuery.data) {
      cacheManifest(manifestQuery.data)
      setManifest(manifestQuery.data)
    }
  }, [manifestQuery.data])

  const [queue, setQueue] = useState<QueuedScan[]>(() => loadQueue(subEventId))
  const [online, setOnline] = useState(() => navigator.onLine)
  const [result, setResult] = useState<CheckInResult | null>(null)
  const [pendingCode, setPendingCode] = useState<string | null>(null)
  const [typed, setTyped] = useState("")
  const [typing, setTyping] = useState(!isScanningSupported())

  // Codes this device has admitted since the manifest was fetched. Lets the
  // offline judgement know about people it let in a minute ago.
  const admittedHere = useRef<Set<string>>(new Set())
  // Guards against the same badge being decoded on twenty consecutive frames.
  const lastScan = useRef<{ code: string; at: number }>({ code: "", at: 0 })

  useEffect(() => saveQueue(subEventId, queue), [subEventId, queue])

  useEffect(() => {
    const up = () => setOnline(true)
    const down = () => setOnline(false)
    window.addEventListener("online", up)
    window.addEventListener("offline", down)
    return () => {
      window.removeEventListener("online", up)
      window.removeEventListener("offline", down)
    }
  }, [])

  // Drain whatever is waiting whenever the signal is back. Runs on a timer as
  // well as the online event, because `navigator.onLine` lies about captive
  // venue Wi-Fi that is "connected" but routing nowhere.
  const flushing = useRef(false)
  const drain = useCallback(async () => {
    if (flushing.current) return
    const waiting = loadQueue(subEventId)
    if (waiting.length === 0) return
    flushing.current = true
    try {
      const left = await flushQueue(subEventId, waiting)
      setQueue(left)
      if (left.length < waiting.length) {
        void checkIns.refetch()
        toast.success(`Synced ${waiting.length - left.length} scans`)
      }
    } finally {
      flushing.current = false
    }
  }, [subEventId, checkIns])

  useEffect(() => {
    if (!online) return
    void drain()
    const timer = setInterval(() => void drain(), 15_000)
    return () => clearInterval(timer)
  }, [online, drain])

  const submit = useCallback(
    async (raw: string, allowOver: boolean) => {
      const code = normalizeCode(raw)
      if (!isPlausibleCode(code)) {
        setResult({
          outcome: "unknown_code",
          label: null,
          guestName: null,
          partyCheckedIn: 0,
          partyAllowed: 0,
          checkedInAt: null,
        })
        return
      }

      const scan: QueuedScan = {
        id: crypto.randomUUID(),
        code,
        allowOver,
        scannedAt: new Date().toISOString(),
      }

      // Offline: answer from the manifest and queue the scan. The operator gets
      // the same words either way, and the server reconciles later.
      if (!online) {
        const local = manifest
          ? judgeLocally(manifest, code, admittedHere.current, allowOver)
          : ({
              outcome: "admitted",
              label: code,
              guestName: null,
              partyCheckedIn: 0,
              partyAllowed: 0,
              checkedInAt: scan.scannedAt,
            } satisfies CheckInResult)
        setResult(local)
        if (local.outcome === "admitted") {
          admittedHere.current.add(code)
          setQueue((q) => [...q, scan])
        } else if (local.outcome === "over_allowance") {
          setPendingCode(code)
        }
        return
      }

      const sent = await sendScan(subEventId, scan, false)
      if (sent === null) {
        // The request died on the way out. Treat it exactly like being offline
        // rather than telling staff to try again.
        setOnline(false)
        setQueue((q) => [...q, scan])
        setResult({
          outcome: "admitted",
          label: manifest?.entries.find((e) => e.code === code)?.label ?? code,
          guestName: null,
          partyCheckedIn: 0,
          partyAllowed: 0,
          checkedInAt: scan.scannedAt,
        })
        admittedHere.current.add(code)
        return
      }

      setResult(sent)
      setPendingCode(sent.outcome === "over_allowance" ? code : null)
      if (sent.outcome === "admitted") {
        admittedHere.current.add(code)
        void checkIns.refetch()
      }
    },
    [manifest, online, subEventId, checkIns],
  )

  const onScan = useCallback(
    (value: string) => {
      const code = normalizeCode(value)
      const now = Date.now()
      // The same badge in front of the lens decodes many times a second.
      if (lastScan.current.code === code && now - lastScan.current.at < RESULT_MS * 2) return
      lastScan.current = { code, at: now }
      void submit(value, false)
    },
    [submit],
  )

  // Clear the banner so the next guest can step up. An over-allowance decision
  // stays until staff answer it — that one is a question, not a notification.
  useEffect(() => {
    if (!result || result.outcome === "over_allowance") return
    const timer = setTimeout(() => setResult(null), RESULT_MS)
    return () => clearTimeout(timer)
  }, [result])

  const through = checkIns.data?.length ?? 0
  // Heads, not parties: the number staff are counting up to is people through
  // the door. Each guest's allowance is on all of their entries, so take it
  // once per guest.
  const expected = useMemo(() => {
    if (!manifest) return 0
    const byGuest = new Map<string, number>()
    for (const entry of manifest.entries) byGuest.set(entry.guestId, entry.partyAllowed)
    return [...byGuest.values()].reduce((sum, n) => sum + n, 0)
  }, [manifest])

  return (
    <div className="mt-5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">
            {manifest?.subEventName ?? "Door"}
          </h1>
          <p className="text-muted-foreground mt-1 text-sm">
            {through} through
            {expected > 0 && ` · ${expected} confirmed`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {!online && (
            <Badge variant="outline" className="gap-1.5 border-amber-500/40 text-amber-300">
              <CloudOff className="size-3.5" />
              Offline
            </Badge>
          )}
          {queue.length > 0 && (
            <Badge variant="outline" className="gap-1.5">
              <RefreshCw className="size-3.5" />
              {queue.length} to sync
            </Badge>
          )}
        </div>
      </div>

      <div className="mt-5">
        {!typing && <QrScanner onScan={onScan} paused={result !== null} />}

        <AnimatePresence mode="wait">
          {result && (
            <motion.div
              key={`${result.outcome}-${result.label}-${result.checkedInAt}`}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              className={`mt-4 rounded-xl border p-4 ${OUTCOME[result.outcome].tone}`}
            >
              <div className="flex items-start gap-3">
                {(() => {
                  const Icon = OUTCOME[result.outcome].icon
                  return <Icon className="mt-0.5 size-5 shrink-0" />
                })()}
                <div className="min-w-0 flex-1">
                  <p className="font-medium">{OUTCOME[result.outcome].title}</p>
                  {result.label && <p className="mt-0.5 truncate text-lg">{result.label}</p>}
                  {result.partyAllowed > 0 && (
                    <p className="mt-0.5 text-sm opacity-80">
                      {result.partyCheckedIn} of {result.partyAllowed} in their party
                    </p>
                  )}
                  {result.outcome === "over_allowance" && (
                    <p className="mt-0.5 text-sm opacity-80">
                      {result.partyAllowed === 0
                        ? "They didn't confirm for this part."
                        : "Everyone they confirmed for is already inside."}
                    </p>
                  )}
                </div>
              </div>

              {result.outcome === "over_allowance" && pendingCode && (
                <div className="mt-3 flex gap-2">
                  <Button
                    size="sm"
                    onClick={() => {
                      const code = pendingCode
                      setPendingCode(null)
                      setResult(null)
                      void submit(code, true)
                    }}
                  >
                    Let them in
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      setPendingCode(null)
                      setResult(null)
                    }}
                  >
                    Turn away
                  </Button>
                </div>
              )}
            </motion.div>
          )}
        </AnimatePresence>

        <form
          className="mt-4 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            if (typed.trim() === "") return
            void submit(typed, false)
            setTyped("")
          }}
        >
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder="Type a code"
            autoCapitalize="characters"
            autoComplete="off"
            spellCheck={false}
            className="font-mono tracking-widest uppercase"
          />
          <Button type="submit" variant="outline" disabled={typed.trim() === ""}>
            Check
          </Button>
        </form>

        {isScanningSupported() ? (
          <Button
            variant="ghost"
            size="sm"
            className="mt-2 w-full"
            onClick={() => setTyping((t) => !t)}
          >
            <Keyboard className="size-4" />
            {typing ? "Use the camera" : "Type codes only"}
          </Button>
        ) : (
          // Said once, plainly, rather than leaving staff wondering where the
          // camera went. Every pass prints its code underneath the square.
          <p className="text-muted-foreground mt-2 text-center text-xs">
            This browser can't scan. Read the code under the guest's QR and type it.
          </p>
        )}
      </div>

      {(checkIns.data?.length ?? 0) > 0 && (
        <div className="mt-8">
          <h2 className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
            Last through
          </h2>
          <ul className="mt-2 divide-y rounded-xl border">
            {checkIns.data!.slice(0, 12).map((record) => (
              <li
                key={record.attendeeId}
                className="flex items-center justify-between gap-3 px-4 py-2.5 text-sm"
              >
                <span className="truncate">{record.label}</span>
                <span className="flex shrink-0 items-center gap-2">
                  {record.overAllowance && (
                    <Badge variant="outline" className="border-amber-500/40 text-amber-300">
                      over
                    </Badge>
                  )}
                  <span className="text-muted-foreground tabular-nums">
                    {new Date(record.checkedInAt).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}
