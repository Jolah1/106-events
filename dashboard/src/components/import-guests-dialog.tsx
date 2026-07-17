import { useEffect, useRef, useState } from "react"
import { AlertTriangle, Check, Upload } from "lucide-react"
import { toast } from "sonner"

import { PartCheckboxes } from "@/components/guest-dialog"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { ApiError } from "@/lib/api"
import { useImportGuests } from "@/lib/queries"
import type { ImportReport, SubEvent } from "@/lib/types"

const SAMPLE = "Name,Phone,Plus Ones,Dietary\nAdaeze Okafor,0806 688 2563,2,Vegetarian"

function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`
}

/**
 * Counts the organizer cares about, with the zeroes left out: "3 guests to
 * add, 0 guests to update" reads like a bug report.
 */
function describe(report: ImportReport, tense: "will" | "did") {
  const [add, update] = tense === "will" ? ["to add", "to update"] : ["added", "updated"]
  return [
    report.created > 0 && `${plural(report.created, "guest")} ${add}`,
    report.updated > 0 && `${plural(report.updated, "guest")} ${update}`,
  ]
    .filter(Boolean)
    .join(", ")
}

function ReportView({ report }: { report: ImportReport }) {
  const willImport = report.created + report.updated
  return (
    <div className="flex flex-col gap-3 rounded-lg border p-3 text-sm">
      <p className="flex items-center gap-2 font-medium">
        {willImport ? (
          <Check data-slot="icon" className="size-4 text-primary" />
        ) : (
          <AlertTriangle data-slot="icon" className="size-4 text-destructive" />
        )}
        {willImport ? describe(report, "will") : "No usable rows in this file"}
      </p>

      {report.updated > 0 && (
        <p className="text-muted-foreground">
          Matched by phone or email — re-importing updates those guests rather than
          duplicating them.
        </p>
      )}

      {report.ignoredColumns.length > 0 && (
        <p className="text-muted-foreground">
          Ignored {report.ignoredColumns.length === 1 ? "column" : "columns"}:{" "}
          {report.ignoredColumns.join(", ")}.
        </p>
      )}

      {report.errors.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <p className="font-medium text-destructive">
            {plural(report.errors.length, "row")} will be skipped
          </p>
          <ul className="flex flex-col gap-1 text-muted-foreground">
            {report.errors.slice(0, 8).map((error) => (
              <li key={`${error.line}-${error.message}`}>
                <span className="text-foreground">Line {error.line}</span> — {error.message}
              </li>
            ))}
          </ul>
          {report.errors.length > 8 && (
            <p className="text-muted-foreground">
              …and {report.errors.length - 8} more.
            </p>
          )}
        </div>
      )}
    </div>
  )
}

export function ImportGuestsDialog({
  eventId,
  parts,
  soloDefault,
  open,
  onOpenChange,
}: {
  eventId: string
  parts: SubEvent[]
  soloDefault: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [csv, setCsv] = useState("")
  const [fileName, setFileName] = useState<string | null>(null)
  const [subEventIds, setSubEventIds] = useState<string[]>([])
  const [report, setReport] = useState<ImportReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  const importGuests = useImportGuests(eventId)

  useEffect(() => {
    if (!open) return
    setCsv("")
    setFileName(null)
    setReport(null)
    setError(null)
    // A single-part event has nowhere else to invite people to.
    setSubEventIds(soloDefault ? parts.map((p) => p.id) : [])
  }, [open, parts, soloDefault])

  // Any edit invalidates the preview: it described the previous text.
  function editCsv(value: string) {
    setCsv(value)
    setReport(null)
    setError(null)
  }

  async function chooseFile(file: File | undefined) {
    if (!file) return
    setFileName(file.name)
    editCsv(await file.text())
  }

  async function run(dryRun: boolean) {
    setError(null)
    try {
      const result = await importGuests.mutateAsync({ csv, subEventIds, dryRun })
      if (dryRun) {
        setReport(result)
      } else {
        toast.success(describe(result, "did") || "Nothing to import")
        onOpenChange(false)
      }
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Couldn't read that file. Try again.",
      )
    }
  }

  const canPreview = csv.trim().length > 0 && !importGuests.isPending
  const canCommit = report !== null && report.created + report.updated > 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Import guests</DialogTitle>
          <DialogDescription>
            Upload the spreadsheet you already have. Columns are matched by name, so
            "Phone", "Mobile" and "WhatsApp Number" all work.
          </DialogDescription>
        </DialogHeader>

        <div className="mt-5 flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-3">
            <input
              ref={fileInput}
              type="file"
              accept=".csv,text/csv"
              className="sr-only"
              onChange={(e) => chooseFile(e.target.files?.[0])}
            />
            <Button type="button" variant="outline" onClick={() => fileInput.current?.click()}>
              <Upload data-slot="icon" />
              Choose CSV file
            </Button>
            {fileName && (
              <span className="min-w-0 truncate text-sm text-muted-foreground">{fileName}</span>
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="import-csv">…or paste it here</Label>
            <Textarea
              id="import-csv"
              rows={6}
              value={csv}
              onChange={(e) => editCsv(e.target.value)}
              placeholder={SAMPLE}
              className="font-mono text-xs"
              spellCheck={false}
            />
          </div>

          {!soloDefault && (
            <div className="flex flex-col gap-2.5">
              <Label>Invite everyone to</Label>
              <PartCheckboxes
                parts={parts}
                selected={subEventIds}
                onChange={(ids) => {
                  setSubEventIds(ids)
                  setReport(null)
                }}
                idPrefix="import-part"
              />
              <p className="text-xs text-muted-foreground">
                Skipped for any row whose file says which parts it attends.
              </p>
            </div>
          )}

          {report && <ReportView report={report} />}
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>

        <DialogFooter className="mt-6">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          {report ? (
            <Button
              type="button"
              disabled={!canCommit || importGuests.isPending}
              onClick={() => run(false)}
            >
              {importGuests.isPending
                ? "Importing…"
                : `Import ${plural(report.created + report.updated, "guest")}`}
            </Button>
          ) : (
            <Button type="button" disabled={!canPreview} onClick={() => run(true)}>
              {importGuests.isPending ? "Reading…" : "Preview import"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
