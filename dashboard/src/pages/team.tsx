import { useState, type FormEvent } from "react"
import { Plus, Shield, Trash2, UserRound } from "lucide-react"
import { Link } from "react-router"
import { toast } from "sonner"

import { AccessQueue } from "@/components/access-queue"
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
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import {
  useInviteMember,
  useMe,
  useRemoveMember,
  useTeam,
  useUpdateMember,
} from "@/lib/queries"
import type { Role, TeamMember } from "@/lib/types"

export function TeamPage() {
  const me = useMe()
  const team = useTeam()

  if (me.isPending || team.isPending || !me.data) {
    return (
      <div className="mx-auto max-w-2xl">
        <Skeleton className="h-8 w-32" />
        <Skeleton className="mt-3 h-4 w-64" />
        <Skeleton className="mt-8 h-16 w-full rounded-xl" />
        <Skeleton className="mt-3 h-16 w-full rounded-xl" />
      </div>
    )
  }

  // The nav only offers this to admins, but a staff member typing /team lands
  // on a 403. Meet it with a real explanation rather than a blank list.
  if (team.isError) {
    const forbidden = team.error instanceof ApiError && team.error.status === 403
    return (
      <div className="flex flex-col items-center gap-4 py-16 text-center">
        <p className="text-muted-foreground">
          {forbidden
            ? "Only admins can manage the team."
            : "Couldn't load the team."}
        </p>
        <Button asChild variant="outline">
          <Link to="/events">Back to events</Link>
        </Button>
      </div>
    )
  }

  return <TeamView members={team.data} meId={me.data.id} />
}

function TeamView({ members, meId }: { members: TeamMember[]; meId: string }) {
  const [inviteOpen, setInviteOpen] = useState(false)
  const [removing, setRemoving] = useState<TeamMember | null>(null)

  const updateMember = useUpdateMember()
  const removeMember = useRemoveMember()

  const adminCount = members.filter((m) => m.role === "admin").length
  // Reaching this view already means the team endpoint let us in, which only
  // admins get. Reading it off the list keeps that implicit fact checkable.
  const isAdmin = members.some((m) => m.id === meId && m.role === "admin")

  function changeRole(member: TeamMember, role: Role) {
    updateMember.mutate(
      { id: member.id, role },
      {
        onError: (err) =>
          toast.error(
            err instanceof ApiError ? err.message : `Couldn't update ${member.name || "member"}.`,
          ),
      },
    )
  }

  return (
    <div className="mx-auto max-w-2xl">
      <Link to="/events" className="text-sm text-muted-foreground hover:text-foreground">
        ← Events
      </Link>

      <div className="mt-2 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="font-heading text-3xl font-semibold leading-tight">Team</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Everyone here can see and work every event. Admins also manage this list.
          </p>
        </div>
        <Button size="sm" onClick={() => setInviteOpen(true)}>
          <Plus data-slot="icon" />
          Invite
        </Button>
      </div>

      <AccessQueue isAdmin={isAdmin} />

      <div className="mt-8 flex flex-col gap-3">
        {members.map((member) => {
          const isSelf = member.id === meId
          const isLastAdmin = member.role === "admin" && adminCount === 1
          return (
            <div
              key={member.id}
              className="flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4"
            >
              <div className="flex items-center gap-3">
                <div className="flex size-9 items-center justify-center rounded-full bg-secondary text-secondary-foreground">
                  {member.role === "admin" ? (
                    <Shield data-slot="icon" className="size-4" />
                  ) : (
                    <UserRound data-slot="icon" className="size-4" />
                  )}
                </div>
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{member.name || member.email}</span>
                    {isSelf && <Badge variant="secondary">You</Badge>}
                  </div>
                  {member.name && (
                    <p className="truncate text-sm text-muted-foreground">{member.email}</p>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2">
                <RoleToggle
                  role={member.role}
                  // Demoting the only admin is refused by the server; disable it
                  // here too so the button doesn't offer a dead end.
                  disabled={updateMember.isPending || isLastAdmin}
                  onChange={(role) => changeRole(member, role)}
                />
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={`Remove ${member.name || member.email}`}
                  disabled={isSelf || isLastAdmin}
                  onClick={() => setRemoving(member)}
                >
                  <Trash2 data-slot="icon" />
                </Button>
              </div>
            </div>
          )
        })}
      </div>

      <InviteDialog open={inviteOpen} onOpenChange={setInviteOpen} />

      <Dialog open={removing !== null} onOpenChange={(open) => !open && setRemoving(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Remove {removing?.name || removing?.email}?</DialogTitle>
            <DialogDescription>
              They'll lose access immediately. Events they created stay, no longer
              attributed to anyone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-6">
            <Button variant="ghost" onClick={() => setRemoving(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const member = removing!
                setRemoving(null)
                removeMember.mutate(member.id, {
                  onError: (err) =>
                    toast.error(
                      err instanceof ApiError
                        ? err.message
                        : `Couldn't remove ${member.name || "member"}.`,
                    ),
                })
              }}
            >
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function RoleToggle({
  role,
  disabled,
  onChange,
}: {
  role: Role
  disabled: boolean
  onChange: (role: Role) => void
}) {
  return (
    <div className="inline-flex rounded-lg border p-0.5 text-sm" role="group" aria-label="Role">
      {(["staff", "admin"] as const).map((option) => (
        <button
          key={option}
          type="button"
          disabled={disabled && role !== option}
          aria-pressed={role === option}
          onClick={() => role !== option && onChange(option)}
          className={`rounded-md px-2.5 py-1 capitalize transition-colors ${
            role === option
              ? "bg-secondary text-secondary-foreground"
              : "text-muted-foreground hover:text-foreground disabled:opacity-40"
          }`}
        >
          {option}
        </button>
      ))}
    </div>
  )
}

function InviteDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const invite = useInviteMember()
  const [email, setEmail] = useState("")
  const [name, setName] = useState("")
  const [role, setRole] = useState<Role>("staff")
  const [error, setError] = useState<string | null>(null)

  function reset() {
    setEmail("")
    setName("")
    setRole("staff")
    setError(null)
  }

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (!/^[^\s@]+@[^\s@.][^\s@]*\.[^\s@.]+$/.test(email.trim())) {
      return setError("Enter a valid email address.")
    }
    try {
      const member = await invite.mutateAsync({ email: email.trim(), name: name.trim(), role })
      toast.success(`${member.name || member.email} can now sign in.`)
      onOpenChange(false)
      reset()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Couldn't send the invite. Try again.")
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next)
        if (!next) reset()
      }}
    >
      <DialogContent className="sm:max-w-md">
        <form onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>Invite a colleague</DialogTitle>
            <DialogDescription>
              They sign in with a magic link to this email — no password to share.
            </DialogDescription>
          </DialogHeader>

          <div className="mt-5 flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="invite-email">Email</Label>
              <Input
                id="invite-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="colleague@example.com"
                autoFocus
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="invite-name">Name (optional)</Label>
              <Input
                id="invite-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Their name"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label>Role</Label>
              <RoleToggle role={role} disabled={false} onChange={setRole} />
              <p className="text-xs text-muted-foreground">
                Admins can manage the team. Staff can do everything else.
              </p>
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
          </div>

          <DialogFooter className="mt-6">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={invite.isPending}>
              {invite.isPending ? "Inviting…" : "Send invite"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
