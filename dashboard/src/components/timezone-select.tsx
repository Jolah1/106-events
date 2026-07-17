import { cn } from "@/lib/utils"

const COMMON = [
  "Africa/Lagos",
  "Africa/Accra",
  "Africa/Nairobi",
  "Africa/Johannesburg",
  "Europe/London",
  "Europe/Paris",
  "America/New_York",
  "America/Toronto",
  "Asia/Dubai",
]

const ALL: string[] = (() => {
  try {
    return Intl.supportedValuesOf("timeZone")
  } catch {
    return COMMON
  }
})()

export function TimezoneSelect({
  id,
  value,
  onChange,
  className,
}: {
  id?: string
  value: string
  onChange: (tz: string) => void
  className?: string
}) {
  return (
    <select
      id={id}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className={cn(
        "h-9 w-full appearance-none rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs transition-[color,box-shadow] outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 [&>option]:bg-popover [&>option]:text-popover-foreground",
        className,
      )}
    >
      <optgroup label="Common">
        {COMMON.map((tz) => (
          <option key={tz} value={tz}>
            {tz.replace(/_/g, " ")}
          </option>
        ))}
      </optgroup>
      <optgroup label="All timezones">
        {ALL.filter((tz) => !COMMON.includes(tz)).map((tz) => (
          <option key={tz} value={tz}>
            {tz.replace(/_/g, " ")}
          </option>
        ))}
      </optgroup>
    </select>
  )
}
