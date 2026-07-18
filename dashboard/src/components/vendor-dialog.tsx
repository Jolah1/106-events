import { useEffect, useState } from "react"

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
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { koboToNairaInput, parseNairaToKobo } from "@/lib/money"
import type { Vendor } from "@/lib/types"

/** The categories that come up on nearly every job, as one-tap chips. Free
 *  text underneath, because the next event always needs one nobody listed. */
const CATEGORIES = [
  "Catering",
  "Venue",
  "Photography",
  "Decor",
  "Music",
  "Cake",
  "Aso-ebi",
  "Transport",
]

export interface VendorDraft {
  name: string
  category: string
  phone: string
  email: string
  service: string
  cost: string
  paid: string
  notes: string
}

export function draftFromVendor(vendor: Vendor): VendorDraft {
  return {
    name: vendor.name,
    category: vendor.category,
    phone: vendor.phone ?? "",
    email: vendor.email ?? "",
    service: vendor.service,
    cost: koboToNairaInput(vendor.costKobo),
    paid: koboToNairaInput(vendor.amountPaidKobo),
    notes: vendor.notes,
  }
}

export const emptyVendor: VendorDraft = {
  name: "",
  category: "",
  phone: "",
  email: "",
  service: "",
  cost: "",
  paid: "",
  notes: "",
}

export function VendorDialog({
  open,
  onOpenChange,
  initial,
  title,
  submitLabel,
  onSubmit,
  pending,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  initial: VendorDraft
  title: string
  submitLabel: string
  onSubmit: (draft: VendorDraft) => void
  pending?: boolean
}) {
  const [draft, setDraft] = useState(initial)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (open) {
      setDraft(initial)
      setError(null)
    }
    // `initial` is rebuilt each render by the caller; keying off `open` is what
    // makes the dialog reset per opening rather than fight the user's typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const set = (key: keyof VendorDraft) => (value: string) =>
    setDraft((d) => ({ ...d, [key]: value }))

  function submit() {
    if (!draft.name.trim()) return setError("Give the vendor a name.")
    if (parseNairaToKobo(draft.cost) === null) return setError("Cost isn't a number.")
    if (parseNairaToKobo(draft.paid) === null) return setError("Amount paid isn't a number.")
    setError(null)
    onSubmit(draft)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            Costs are what you agreed. Paid is what has actually left the account.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="vendor-name">Name</Label>
            <Input
              id="vendor-name"
              value={draft.name}
              onChange={(e) => set("name")(e.target.value)}
              placeholder="Ronke's Kitchen"
              autoFocus
            />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="vendor-category">Category</Label>
            <Input
              id="vendor-category"
              value={draft.category}
              onChange={(e) => set("category")(e.target.value)}
              placeholder="Catering"
            />
            <div className="flex flex-wrap gap-1.5">
              {CATEGORIES.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => set("category")(c)}
                  className="rounded-full border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                >
                  {c}
                </button>
              ))}
            </div>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor="vendor-cost">Cost (₦)</Label>
              <Input
                id="vendor-cost"
                inputMode="decimal"
                value={draft.cost}
                onChange={(e) => set("cost")(e.target.value)}
                placeholder="150,000"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="vendor-paid">Paid so far (₦)</Label>
              <Input
                id="vendor-paid"
                inputMode="decimal"
                value={draft.paid}
                onChange={(e) => set("paid")(e.target.value)}
                placeholder="50,000"
              />
            </div>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor="vendor-phone">Phone</Label>
              <Input
                id="vendor-phone"
                value={draft.phone}
                onChange={(e) => set("phone")(e.target.value)}
                placeholder="0806 688 2563"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="vendor-email">Email</Label>
              <Input
                id="vendor-email"
                value={draft.email}
                onChange={(e) => set("email")(e.target.value)}
                placeholder="hello@vendor.com"
              />
            </div>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="vendor-service">What they're doing</Label>
            <Input
              id="vendor-service"
              value={draft.service}
              onChange={(e) => set("service")(e.target.value)}
              placeholder="Small chops and jollof for 300"
            />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="vendor-notes">Notes</Label>
            <Textarea
              id="vendor-notes"
              value={draft.notes}
              onChange={(e) => set("notes")(e.target.value)}
              placeholder="Deposit paid by transfer on the 3rd"
              rows={2}
            />
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={pending}>
            {submitLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
