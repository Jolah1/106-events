/**
 * People who asked for an account from the landing page.
 *
 * It sits on the Team page because inviting someone *is* how a request gets
 * resolved — the queue and the answer belong in the same place. Admins only:
 * staff never see it, rather than being shown a section that 403s.
 */

import { Check, Mail, Phone, UserPlus } from "lucide-react"
import { AnimatePresence, motion } from "motion/react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ApiError } from "@/lib/api"
import { useAccessRequests, useHandleAccessRequest, useInviteMember } from "@/lib/queries"
import type { AccessRequest } from "@/lib/types"

function asked(iso: string): string {
  const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000)
  if (days === 0) return "today"
  if (days === 1) return "yesterday"
  if (days < 14) return `${days} days ago`
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" })
}

export function AccessQueue({ isAdmin }: { isAdmin: boolean }) {
  const requests = useAccessRequests(isAdmin)
  const handle = useHandleAccessRequest()
  const invite = useInviteMember()

  const waiting = requests.data ?? []
  if (!isAdmin || waiting.length === 0) return null

  // Inviting them creates their account and clears the request in one go: an
  // admin shouldn't have to remember to come back and tidy up.
  function admit(request: AccessRequest) {
    invite.mutate(
      { email: request.email, name: request.name },
      {
        onSuccess: () => {
          handle.mutate(request.id)
          toast.success(`${request.name} can sign in now`)
        },
        onError: (err) => {
          if (err instanceof ApiError && err.status === 409) {
            // Already on the team — the request is stale, not a failure.
            handle.mutate(request.id)
            toast.success(`${request.name} was already on the team`)
            return
          }
          toast.error(err instanceof ApiError ? err.message : "Couldn't add them.")
        },
      },
    )
  }

  return (
    <section className="mt-8">
      <h2 className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
        Asking for access · {waiting.length}
      </h2>

      <div className="mt-2 flex flex-col gap-3">
        <AnimatePresence initial={false}>
          {waiting.map((request) => (
            <motion.div
              key={request.id}
              layout
              exit={{ opacity: 0, height: 0, marginBottom: -12 }}
              className="rounded-xl border border-dashed p-4"
            >
              {/* The buttons hold their place on the right; it's the contact
                  line that wraps, so every card in the queue reads the same. */}
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <p className="font-medium">{request.name}</p>
                  <p className="text-muted-foreground mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
                    <span className="inline-flex items-center gap-1.5">
                      <Mail className="size-3.5" />
                      <a className="hover:text-foreground" href={`mailto:${request.email}`}>
                        {request.email}
                      </a>
                    </span>
                    {request.phone && (
                      <span className="inline-flex items-center gap-1.5">
                        <Phone className="size-3.5" />
                        <a className="hover:text-foreground" href={`tel:${request.phone}`}>
                          {request.phone}
                        </a>
                      </span>
                    )}
                    <span>asked {asked(request.createdAt)}</span>
                  </p>
                </div>

                <div className="flex shrink-0 gap-2">
                  <Button size="sm" onClick={() => admit(request)} disabled={invite.isPending}>
                    <UserPlus data-slot="icon" />
                    Add to team
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handle.mutate(request.id)}
                    disabled={handle.isPending}
                  >
                    <Check data-slot="icon" />
                    Dismiss
                  </Button>
                </div>
              </div>

              {request.about && (
                <p className="mt-3 border-l-2 pl-3 text-sm whitespace-pre-line">
                  {request.about}
                </p>
              )}
              {request.budget && (
                <p className="text-muted-foreground mt-2 text-sm">
                  Budget: <span className="text-foreground">{request.budget}</span>
                </p>
              )}
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </section>
  )
}
