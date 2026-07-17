import { LogOut, Plus, Users } from "lucide-react"
import { Link, Navigate, Outlet, useNavigate } from "react-router"

import { CommandPalette } from "@/components/command-palette"
import { Wordmark } from "@/components/wordmark"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { ApiError } from "@/lib/api"
import { useLogout, useMe } from "@/lib/queries"

export function AppShell() {
  const me = useMe()
  const logout = useLogout()
  const navigate = useNavigate()

  if (me.isPending) {
    return (
      <div className="min-h-dvh">
        <header className="mx-auto flex h-14 max-w-5xl items-center justify-between px-4">
          <Wordmark className="text-lg" />
          <Skeleton className="size-8 rounded-full" />
        </header>
        <div className="gilt-seam" />
      </div>
    )
  }

  if (me.isError) {
    if (me.error instanceof ApiError && me.error.status === 401) {
      return <Navigate to="/login" replace />
    }
    return (
      <div className="flex min-h-dvh flex-col items-center justify-center gap-4 px-4 text-center">
        <Wordmark className="text-2xl" />
        <p className="text-muted-foreground">Couldn't reach the server. Check your connection.</p>
        <Button variant="outline" onClick={() => me.refetch()}>
          Try again
        </Button>
      </div>
    )
  }

  const user = me.data
  const initial = (user.name || user.email || "?").trim().charAt(0).toUpperCase()

  return (
    <div className="min-h-dvh">
      <header className="mx-auto flex h-14 max-w-5xl items-center justify-between px-4">
        <Link to="/" aria-label="106 Events home">
          <Wordmark className="text-lg" />
        </Link>
        <div className="flex items-center gap-2">
          <Button asChild size="sm" variant="ghost" className="hidden sm:inline-flex">
            <Link to="/events/new">
              <Plus data-slot="icon" />
              New event
            </Link>
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                className="flex size-8 items-center justify-center rounded-full bg-secondary text-sm font-medium text-secondary-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-ring"
                aria-label="Account menu"
              >
                {initial}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-52">
              <DropdownMenuLabel className="font-normal text-muted-foreground">
                {user.email ?? user.phone}
                {user.role === "admin" && (
                  <span className="ml-2 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-primary">
                    Admin
                  </span>
                )}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              {user.role === "admin" && (
                <DropdownMenuItem asChild>
                  <Link to="/team">
                    <Users data-slot="icon" />
                    Team
                  </Link>
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onSelect={() =>
                  logout.mutate(undefined, { onSuccess: () => navigate("/login") })
                }
              >
                <LogOut data-slot="icon" />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>
      <div className="gilt-seam" />
      <main className="mx-auto max-w-5xl px-4 py-6 sm:py-8">
        <Outlet />
      </main>
      <CommandPalette />
    </div>
  )
}
