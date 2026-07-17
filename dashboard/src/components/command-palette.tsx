import { useEffect, useState } from "react"
import { CalendarDays, LogOut, Plus } from "lucide-react"
import { useNavigate } from "react-router"

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import { useEvents, useLogout } from "@/lib/queries"

/** Cmd+K / Ctrl+K everywhere in the authenticated app. */
export function CommandPalette() {
  const [open, setOpen] = useState(false)
  const navigate = useNavigate()
  const events = useEvents()
  const logout = useLogout()

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setOpen((v) => !v)
      }
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [])

  const go = (to: string) => {
    setOpen(false)
    navigate(to)
  }

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Search events and actions…" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Actions">
          <CommandItem onSelect={() => go("/events/new")}>
            <Plus data-slot="icon" />
            New event
          </CommandItem>
        </CommandGroup>
        {(events.data?.length ?? 0) > 0 && (
          <CommandGroup heading="Events">
            {events.data!.map((event) => (
              <CommandItem
                key={event.id}
                value={`event-${event.title}-${event.id}`}
                onSelect={() => go(`/events/${event.id}`)}
              >
                <CalendarDays data-slot="icon" />
                {event.title}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        <CommandSeparator />
        <CommandGroup heading="Account">
          <CommandItem
            onSelect={() => {
              setOpen(false)
              logout.mutate(undefined, { onSuccess: () => navigate("/login") })
            }}
          >
            <LogOut data-slot="icon" />
            Sign out
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  )
}
