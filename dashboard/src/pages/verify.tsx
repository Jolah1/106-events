import { useEffect, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Loader2 } from "lucide-react"
import { Link, useNavigate, useSearchParams } from "react-router"

import { Wordmark } from "@/components/wordmark"
import { Button } from "@/components/ui/button"
import { api, ApiError } from "@/lib/api"

export function VerifyPage() {
  const [params] = useSearchParams()
  const navigate = useNavigate()
  const client = useQueryClient()
  const [error, setError] = useState<string | null>(null)
  // Tokens are single-use; the ref guards StrictMode's double effect run.
  const fired = useRef(false)

  useEffect(() => {
    if (fired.current) return
    fired.current = true

    const token = params.get("token")
    if (!token) {
      setError("This sign-in link is missing its token. Request a new one.")
      return
    }
    api
      .post("/api/auth/verify", { token })
      .then(() => {
        client.clear()
        navigate("/", { replace: true })
      })
      .catch((err) => {
        if (err instanceof ApiError && err.status === 403) {
          // The link was valid but the email isn't on the team. Say so plainly
          // rather than showing a generic access error.
          setError("This email isn't on the 106 Events team. Ask an admin to add you.")
        } else if (err instanceof ApiError) {
          setError(err.message)
        } else {
          setError("Couldn't reach the server. Try the link again in a moment.")
        }
      })
  }, [params, client, navigate])

  return (
    <div className="flex min-h-dvh flex-col items-center justify-center gap-6 px-4 text-center">
      <Wordmark className="text-3xl" />
      {error ? (
        <>
          <p className="max-w-sm text-sm text-muted-foreground">{error}</p>
          <Button asChild variant="outline">
            <Link to="/login">Request a new link</Link>
          </Button>
        </>
      ) : (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden />
          Signing you in…
        </p>
      )}
    </div>
  )
}
