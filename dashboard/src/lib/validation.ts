/**
 * Client-side mirrors of server validation rules. The server stays the
 * authority — these exist so the UI can answer instantly, and so an optimistic
 * update never writes a value we already know the server will reject.
 */

/** Returns an error message, or null when the URL is acceptable (empty clears it). */
export function coverImageUrlError(url: string): string | null {
  const trimmed = url.trim()
  if (!trimmed) return null
  if (!/^https?:\/\//i.test(trimmed)) {
    return "Cover image must be a full https:// or http:// URL."
  }
  if (trimmed.length > 2000) return "Cover image URL is too long."
  return null
}
