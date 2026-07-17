import { useState } from "react"
import { Check, Copy, ExternalLink } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useConfig } from "@/lib/queries"

/**
 * The organizer's copy-and-paste-into-WhatsApp surface. The origin comes from
 * the server: in development the dashboard and the public pages are on
 * different ports, so window.location is not it.
 */
export function ShareLink({ slug }: { slug: string }) {
  const config = useConfig()
  const [copied, setCopied] = useState(false)

  if (config.isPending || config.isError) {
    return <Skeleton className="h-[58px] w-full rounded-xl" />
  }

  const url = `${config.data.publicBaseUrl}/e/${slug}`
  const pretty = url.replace(/^https?:\/\//, "")

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url)
      setCopied(true)
      toast.success("Link copied")
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard access needs a secure context and permission; when it's
      // refused, selecting the text by hand still works.
      toast.error("Couldn't copy — select the link and copy it manually.")
    }
  }

  return (
    <div className="flex items-center gap-2 rounded-xl border bg-card p-3">
      <div className="min-w-0 flex-1">
        <p className="text-xs text-muted-foreground">Public event page</p>
        <p className="truncate text-sm text-foreground" title={url}>
          {pretty}
        </p>
      </div>
      <Button variant="ghost" size="icon-sm" onClick={copy} aria-label="Copy public link">
        {copied ? <Check data-slot="icon" className="text-gold" /> : <Copy data-slot="icon" />}
      </Button>
      <Button asChild variant="ghost" size="icon-sm" aria-label="Open public page">
        <a href={url} target="_blank" rel="noopener noreferrer">
          <ExternalLink data-slot="icon" />
        </a>
      </Button>
    </div>
  )
}
