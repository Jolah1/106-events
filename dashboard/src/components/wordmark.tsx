import { cn } from "@/lib/utils"

export function Wordmark({ className }: { className?: string }) {
  return (
    <span className={cn("font-heading font-semibold tracking-wide", className)}>
      <span className="text-gold">106</span>
      <span className="ml-1.5 text-foreground">Events</span>
    </span>
  )
}
