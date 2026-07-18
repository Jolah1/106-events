/**
 * Camera scanning with no dependency, via the browser's own BarcodeDetector.
 *
 * Chrome on Android has it, which is what door staff in this market are
 * holding. Everywhere else `supported` comes back false and the door falls back
 * to typing the code — which works, because the alphabet was chosen so an
 * eight-character code can be read aloud without ambiguity.
 */

import { useCallback, useEffect, useRef, useState } from "react"

interface BarcodeDetectorLike {
  detect(source: CanvasImageSource): Promise<{ rawValue: string }[]>
}

interface BarcodeDetectorConstructor {
  new (options?: { formats?: string[] }): BarcodeDetectorLike
}

function detectorConstructor(): BarcodeDetectorConstructor | null {
  const ctor = (globalThis as { BarcodeDetector?: BarcodeDetectorConstructor })
    .BarcodeDetector
  return ctor ?? null
}

export function isScanningSupported(): boolean {
  return detectorConstructor() !== null && typeof navigator?.mediaDevices?.getUserMedia === "function"
}

interface Props {
  /** Fires for every decode. The door de-duplicates; this stays dumb. */
  onScan: (value: string) => void
  /** Paused while a result banner is showing, so one badge isn't read twice. */
  paused: boolean
}

export function QrScanner({ onScan, paused }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null)
  const [error, setError] = useState<string | null>(null)
  // Held in a ref so the scan loop always sees the current value without being
  // torn down and restarted — restarting the camera between guests is slow.
  const pausedRef = useRef(paused)
  pausedRef.current = paused

  const onScanRef = useRef(onScan)
  onScanRef.current = onScan

  const stop = useCallback((stream: MediaStream | null) => {
    stream?.getTracks().forEach((track) => track.stop())
  }, [])

  useEffect(() => {
    const Detector = detectorConstructor()
    if (!Detector) return

    let stream: MediaStream | null = null
    let frame = 0
    let cancelled = false
    const detector = new Detector({ formats: ["qr_code"] })

    async function start() {
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          // The back camera: staff hold the phone facing the guest's screen.
          video: { facingMode: "environment" },
        })
      } catch {
        setError("No camera. Type the code instead.")
        return
      }
      if (cancelled) {
        stop(stream)
        return
      }
      const video = videoRef.current
      if (!video) return
      video.srcObject = stream
      await video.play().catch(() => setError("Camera wouldn't start. Type the code instead."))
      tick()
    }

    function tick() {
      frame = requestAnimationFrame(async () => {
        const video = videoRef.current
        if (!cancelled && video && video.readyState >= 2 && !pausedRef.current) {
          try {
            const codes = await detector.detect(video)
            if (codes.length > 0) onScanRef.current(codes[0].rawValue)
          } catch {
            // A single bad frame is normal; the next one usually decodes.
          }
        }
        if (!cancelled) tick()
      })
    }

    void start()
    return () => {
      cancelled = true
      cancelAnimationFrame(frame)
      stop(stream)
    }
  }, [stop])

  if (!detectorConstructor()) return null

  return (
    <div className="relative overflow-hidden rounded-xl border bg-black">
      <video
        ref={videoRef}
        className="aspect-square w-full object-cover"
        muted
        playsInline
      />
      {/* A target box: staff aim faster with something to aim at. */}
      <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
        <div className="size-2/3 rounded-xl border-2 border-amber-300/70" />
      </div>
      {error && (
        <p className="absolute inset-x-0 bottom-0 bg-black/70 p-2 text-center text-xs text-white">
          {error}
        </p>
      )}
    </div>
  )
}
