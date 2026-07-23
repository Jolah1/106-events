import { useState, type FormEvent } from "react"
import { useMutation } from "@tanstack/react-query"
import { ArrowRight, MailCheck } from "lucide-react"
import { useNavigate } from "react-router"

import logo from "@/assets/logo.webp"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api, ApiError } from "@/lib/api"

interface RequestLinkResponse {
  sent: boolean
  devLink: string | null
}

export function LoginPage() {
  const [email, setEmail] = useState("")
  const [staffCode, setStaffCode] = useState("")
  const [showCode, setShowCode] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()

  const requestLink = useMutation({
    mutationFn: (input: { email: string; staffCode: string }) =>
      api.post<RequestLinkResponse>("/api/auth/request-link", input),
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "Couldn't reach the server. Try again."),
  })

  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    requestLink.mutate({ email, staffCode })
  }

  const sent = requestLink.data

  return (
    <div className="flex min-h-dvh flex-col items-center justify-center px-4 py-10">
      {/* A plain anchor, not a router Link: the landing page is served by the
          server at /, outside this SPA's routes. */}
      <a href="/" aria-label="106 Events — home" className="mb-8">
        <img src={logo} alt="106 Events" className="h-28 w-auto" />
      </a>
      <div className="w-full max-w-sm overflow-hidden rounded-xl border bg-card">
        <div className="gilt-seam" />
        {sent ? (
          <div className="flex flex-col items-center gap-3 p-6 text-center">
            <MailCheck className="size-8 text-gold" aria-hidden />
            {sent.devLink ? (
              <>
                <h1 className="font-heading text-xl font-semibold">Your link is ready</h1>
                <p className="text-sm text-muted-foreground">
                  Signing in as <span className="text-foreground">{email.trim().toLowerCase()}</span>.
                  The link expires in 15 minutes.
                </p>
                <Button
                  className="mt-2 w-full"
                  onClick={() => {
                    const url = new URL(sent.devLink!)
                    navigate(url.pathname + url.search)
                  }}
                >
                  Continue to dashboard
                  <ArrowRight data-slot="icon" />
                </Button>
              </>
            ) : (
              <>
                <h1 className="font-heading text-xl font-semibold">Check your email</h1>
                <p className="text-sm text-muted-foreground">
                  We sent a sign-in link to <span className="text-foreground">{email.trim().toLowerCase()}</span>.
                  It expires in 15 minutes.
                </p>
              </>
            )}
            <button
              className="text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
              onClick={() => requestLink.reset()}
            >
              Use a different email
            </button>
          </div>
        ) : (
          <form onSubmit={onSubmit} className="flex flex-col gap-4 p-6">
            <div className="space-y-1">
              <h1 className="font-heading text-xl font-semibold">Sign in</h1>
              <p className="text-sm text-muted-foreground">
                No passwords — we'll email you a one-time sign-in link.
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="email">Email</Label>
              <Input
                id="email"
                type="email"
                autoComplete="email"
                autoFocus
                required
                placeholder="you@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </div>
            {showCode ? (
              <div className="space-y-2">
                <Label htmlFor="staff-code">Staff code</Label>
                <Input
                  id="staff-code"
                  type="password"
                  autoComplete="off"
                  placeholder="The team passphrase"
                  value={staffCode}
                  onChange={(e) => setStaffCode(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Shows your sign-in link right here instead of emailing it.
                </p>
              </div>
            ) : (
              <button
                type="button"
                className="self-start text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                onClick={() => setShowCode(true)}
              >
                Have a staff code?
              </button>
            )}
            {error && <p className="text-sm text-destructive">{error}</p>}
            <Button type="submit" disabled={requestLink.isPending}>
              {requestLink.isPending ? "Sending…" : showCode && staffCode ? "Sign me in" : "Email me a sign-in link"}
            </Button>
          </form>
        )}
      </div>
      <p className="mt-6 text-center text-xs text-muted-foreground">
        Every event on one guest list — owambes, launches and everything in between.
      </p>
    </div>
  )
}
