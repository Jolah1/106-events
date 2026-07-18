import { useMemo, useState } from "react"
import { Mail, Pencil, Phone, Plus, Search, Store, Trash2 } from "lucide-react"
import { AnimatePresence, motion } from "motion/react"
import { Link, useParams } from "react-router"
import { toast } from "sonner"

import {
  VendorDialog,
  draftFromVendor,
  emptyVendor,
  type VendorDraft,
} from "@/components/vendor-dialog"
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
import { PAID_STATUS_LABEL, formatNaira, parseNairaToKobo } from "@/lib/money"
import {
  useCreateVendor,
  useDeleteVendor,
  useEvent,
  useUpdateVendor,
  useVendors,
} from "@/lib/queries"
import type { EventDetail, Vendor } from "@/lib/types"

function toInput(draft: VendorDraft) {
  return {
    name: draft.name.trim(),
    category: draft.category.trim(),
    phone: draft.phone.trim(),
    email: draft.email.trim(),
    service: draft.service.trim(),
    costKobo: parseNairaToKobo(draft.cost) ?? 0,
    amountPaidKobo: parseNairaToKobo(draft.paid) ?? 0,
    notes: draft.notes.trim(),
  }
}

export function VendorsPage() {
  const { id } = useParams<{ id: string }>()
  const detail = useEvent(id!)
  const vendors = useVendors(id!)

  if (detail.isPending || vendors.isPending) {
    return (
      <div className="mx-auto max-w-3xl">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="mt-3 h-4 w-56" />
        <Skeleton className="mt-8 h-16 w-full rounded-xl" />
        <Skeleton className="mt-4 h-20 w-full rounded-xl" />
        <Skeleton className="mt-3 h-20 w-full rounded-xl" />
      </div>
    )
  }

  if (detail.isError || vendors.isError) {
    const error = detail.error ?? vendors.error
    const notFound = error instanceof ApiError && error.status === 404
    return (
      <div className="flex flex-col items-center gap-4 py-16 text-center">
        <p className="text-muted-foreground">
          {notFound ? "This event doesn't exist." : "Couldn't load the vendor sheet."}
        </p>
        <Button asChild variant="outline">
          <Link to="/">Back to events</Link>
        </Button>
      </div>
    )
  }

  return <VendorsView event={detail.data} vendors={vendors.data} />
}

/** Money tiles: what the event costs, what's gone out, what's still owed. */
function Totals({ vendors }: { vendors: Vendor[] }) {
  const totals = useMemo(() => {
    return vendors.reduce(
      (acc, v) => ({
        cost: acc.cost + v.costKobo,
        paid: acc.paid + v.amountPaidKobo,
        // Summed from each vendor's clamped outstanding, never as
        // cost - paid overall: one overpaid vendor must not cancel out
        // another's unpaid balance.
        outstanding: acc.outstanding + v.outstandingKobo,
      }),
      { cost: 0, paid: 0, outstanding: 0 },
    )
  }, [vendors])

  const tiles = [
    { label: "Budgeted", value: totals.cost },
    { label: "Paid", value: totals.paid },
    { label: "Outstanding", value: totals.outstanding },
  ]

  return (
    <div className="grid grid-cols-3 gap-3">
      {tiles.map((tile) => (
        <div key={tile.label} className="rounded-xl border bg-card px-4 py-3">
          <p className="text-xs text-muted-foreground">{tile.label}</p>
          <p className="mt-1 font-heading text-lg font-semibold tabular-nums">
            {formatNaira(tile.value)}
          </p>
        </div>
      ))}
    </div>
  )
}

const STATUS_STYLES: Record<Vendor["paidStatus"], string> = {
  unpaid: "border-transparent bg-muted text-muted-foreground",
  part_paid:
    "border-transparent bg-amber-100 text-amber-900 dark:bg-amber-500/15 dark:text-amber-400",
  paid: "border-transparent bg-emerald-100 text-emerald-900 dark:bg-emerald-500/15 dark:text-emerald-400",
  overpaid:
    "border-transparent bg-rose-100 text-rose-900 dark:bg-rose-500/15 dark:text-rose-400",
}

function VendorsView({ event, vendors }: { event: EventDetail; vendors: Vendor[] }) {
  const [query, setQuery] = useState("")
  const [addOpen, setAddOpen] = useState(false)
  const [editing, setEditing] = useState<Vendor | null>(null)
  const [deleting, setDeleting] = useState<Vendor | null>(null)

  const create = useCreateVendor(event.id)
  const update = useUpdateVendor(event.id)
  const remove = useDeleteVendor(event.id)

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return vendors
    return vendors.filter((v) =>
      [v.name, v.category, v.service, v.phone, v.email, v.notes]
        .join(" ")
        .toLowerCase()
        .includes(needle),
    )
  }, [vendors, query])

  async function submitNew(draft: VendorDraft) {
    try {
      await create.mutateAsync(toInput(draft))
      setAddOpen(false)
      toast.success(`Added ${draft.name.trim()}`)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : "Couldn't add that vendor")
    }
  }

  async function submitEdit(draft: VendorDraft) {
    if (!editing) return
    try {
      await update.mutateAsync({ id: editing.id, ...toInput(draft) })
      setEditing(null)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : "Couldn't save that vendor")
    }
  }

  async function confirmDelete() {
    if (!deleting) return
    const name = deleting.name
    try {
      await remove.mutateAsync(deleting.id)
      setDeleting(null)
      toast.success(`Removed ${name}`)
    } catch {
      toast.error("Couldn't remove that vendor")
    }
  }

  return (
    <div className="mx-auto max-w-3xl">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h1 className="font-heading text-2xl font-semibold">Vendors</h1>
          <p className="mt-1 truncate text-sm text-muted-foreground">
            <Link to={`/events/${event.id}`} className="hover:text-foreground">
              {event.title}
            </Link>
          </p>
        </div>
        <Button onClick={() => setAddOpen(true)}>
          <Plus data-slot="icon" />
          Add vendor
        </Button>
      </div>

      {vendors.length > 0 && (
        <div className="mt-6">
          <Totals vendors={vendors} />
        </div>
      )}

      {vendors.length > 0 && (
        <div className="relative mt-4">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search vendors"
            className="pl-9"
          />
        </div>
      )}

      {vendors.length === 0 ? (
        <div className="mt-10 flex flex-col items-center gap-3 rounded-xl border border-dashed py-14 text-center">
          <Store className="size-6 text-muted-foreground" />
          <p className="text-muted-foreground">No vendors on this event yet.</p>
          <Button variant="outline" onClick={() => setAddOpen(true)}>
            <Plus data-slot="icon" />
            Add the first one
          </Button>
        </div>
      ) : (
        <ul className="mt-4 flex flex-col gap-3">
          <AnimatePresence initial={false}>
            {shown.map((vendor) => (
              <motion.li
                key={vendor.id}
                layout
                initial={{ opacity: 0, y: -4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, height: 0 }}
                className="rounded-xl border bg-card p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="font-medium">{vendor.name}</p>
                      {vendor.category && (
                        <Badge variant="outline" className="font-normal">
                          {vendor.category}
                        </Badge>
                      )}
                      <span
                        className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium ${STATUS_STYLES[vendor.paidStatus]}`}
                      >
                        {PAID_STATUS_LABEL[vendor.paidStatus]}
                      </span>
                    </div>
                    {vendor.service && (
                      <p className="mt-1 text-sm text-muted-foreground">{vendor.service}</p>
                    )}
                  </div>
                  <div className="flex shrink-0 gap-1">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={`Edit ${vendor.name}`}
                      onClick={() => setEditing(vendor)}
                    >
                      <Pencil className="size-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={`Remove ${vendor.name}`}
                      onClick={() => setDeleting(vendor)}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  </div>
                </div>

                <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm tabular-nums">
                  <span className="text-muted-foreground">
                    Cost <span className="text-foreground">{formatNaira(vendor.costKobo)}</span>
                  </span>
                  <span className="text-muted-foreground">
                    Paid{" "}
                    <span className="text-foreground">{formatNaira(vendor.amountPaidKobo)}</span>
                  </span>
                  {vendor.outstandingKobo > 0 && (
                    <span className="text-muted-foreground">
                      Owing{" "}
                      <span className="font-medium text-foreground">
                        {formatNaira(vendor.outstandingKobo)}
                      </span>
                    </span>
                  )}
                </div>

                {(vendor.phone || vendor.email || vendor.notes) && (
                  <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-muted-foreground">
                    {vendor.phone && (
                      <a
                        href={`tel:${vendor.phone}`}
                        className="inline-flex items-center gap-1.5 hover:text-foreground"
                      >
                        <Phone className="size-3.5" />
                        {vendor.phone}
                      </a>
                    )}
                    {vendor.email && (
                      <a
                        href={`mailto:${vendor.email}`}
                        className="inline-flex items-center gap-1.5 hover:text-foreground"
                      >
                        <Mail className="size-3.5" />
                        {vendor.email}
                      </a>
                    )}
                  </div>
                )}
                {vendor.notes && (
                  <p className="mt-2 whitespace-pre-line text-sm text-muted-foreground">
                    {vendor.notes}
                  </p>
                )}
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      )}

      {shown.length === 0 && vendors.length > 0 && (
        <p className="mt-6 text-center text-sm text-muted-foreground">
          No vendor matches "{query}".
        </p>
      )}

      <VendorDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        initial={emptyVendor}
        title="Add vendor"
        submitLabel="Add vendor"
        onSubmit={submitNew}
        pending={create.isPending}
      />

      <VendorDialog
        open={editing !== null}
        onOpenChange={(open) => !open && setEditing(null)}
        initial={editing ? draftFromVendor(editing) : emptyVendor}
        title={editing ? `Edit ${editing.name}` : "Edit vendor"}
        submitLabel="Save"
        onSubmit={submitEdit}
        pending={update.isPending}
      />

      <Dialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Remove {deleting?.name}?</DialogTitle>
            <DialogDescription>
              This takes them off this event's sheet, including what you recorded paying them.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleting(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={confirmDelete} disabled={remove.isPending}>
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
