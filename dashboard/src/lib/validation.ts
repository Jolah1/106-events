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

export function guestNameError(name: string): string | null {
  const trimmed = name.trim()
  if (!trimmed) return "Give your guest a name."
  if (trimmed.length > 200) return "That name is too long."
  return null
}

/**
 * Mirrors `domain::phone` closely enough to catch typos while the organizer is
 * still looking at the field. The server does the real normalization, so this
 * only ever needs to agree about what is obviously wrong — an empty value is
 * fine and simply means "no number".
 */
export function guestPhoneError(phone: string): string | null {
  const trimmed = phone.trim()
  if (!trimmed) return null
  if (/[^\d\s+().-]/.test(trimmed)) return "A phone number can only contain digits."
  const digits = trimmed.replace(/\D/g, "")
  const international = trimmed.startsWith("+") || trimmed.startsWith("00")
  const national = digits.replace(/^234/, "").replace(/^0/, "")
  if (/^[789]\d{9}$/.test(national)) return null
  if (international && digits.length >= 8 && digits.length <= 15) return null
  return "That doesn't look like a phone number we can send to."
}

export function guestEmailError(email: string): string | null {
  const trimmed = email.trim()
  if (!trimmed) return null
  if (!/^[^\s@]+@[^\s@.][^\s@]*\.[^\s@.]+$/.test(trimmed)) {
    return "That doesn't look like an email address."
  }
  if (trimmed.length > 254) return "That email address is too long."
  return null
}

export function plusOnesError(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  if (!/^\d+$/.test(trimmed)) return "Plus-ones must be a whole number."
  if (Number(trimmed) > 20) return "20 plus-ones is the limit."
  return null
}
