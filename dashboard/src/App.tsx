import { Navigate, Route, Routes } from "react-router"

import { AppShell } from "@/components/app-shell"
import { Toaster } from "@/components/ui/sonner"
import { EventDetailPage } from "@/pages/event-detail"
import { NewEventPage } from "@/pages/event-new"
import { EventsPage } from "@/pages/events"
import { GuestsPage } from "@/pages/guests"
import { LoginPage } from "@/pages/login"
import { TeamPage } from "@/pages/team"
import { VerifyPage } from "@/pages/verify"

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/auth/verify" element={<VerifyPage />} />
        <Route element={<AppShell />}>
          <Route index element={<EventsPage />} />
          <Route path="events/new" element={<NewEventPage />} />
          <Route path="events/:id" element={<EventDetailPage />} />
          <Route path="events/:id/guests" element={<GuestsPage />} />
          <Route path="team" element={<TeamPage />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <Toaster position="top-center" />
    </>
  )
}
