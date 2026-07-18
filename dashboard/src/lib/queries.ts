import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import { api, ApiError } from "@/lib/api"
import type {
  AppConfig,
  CreateEventInput,
  CreateGuestInput,
  Event as EventModel,
  EventDetail,
  EventSummary,
  Guest,
  GuestPatch,
  ImportInput,
  ImportReport,
  InviteInput,
  ReminderSchedule,
  Role,
  SubEvent,
  SubEventInput,
  TeamMember,
  User,
  Vendor,
  VendorPatch,
  CreateVendorInput,
} from "@/lib/types"

/** Server-provided origins. Static for the life of the process, so never stale. */
export function useConfig() {
  return useQuery({
    queryKey: ["config"],
    queryFn: () => api.get<AppConfig>("/api/config"),
    staleTime: Infinity,
  })
}

export function useMe() {
  return useQuery({
    queryKey: ["me"],
    queryFn: () => api.get<{ user: User }>("/api/auth/me").then((r) => r.user),
    retry: (count, error) =>
      !(error instanceof ApiError && error.status === 401) && count < 2,
    staleTime: 5 * 60 * 1000,
  })
}

export function useLogout() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: () => api.post<void>("/api/auth/logout"),
    onSuccess: () => client.clear(),
  })
}

export function useEvents() {
  return useQuery({
    queryKey: ["events"],
    queryFn: () => api.get<EventSummary[]>("/api/events"),
  })
}

export function useEvent(id: string) {
  return useQuery({
    queryKey: ["events", id],
    queryFn: () => api.get<EventDetail>(`/api/events/${id}`),
  })
}

export function useCreateEvent() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateEventInput) => api.post<EventDetail>("/api/events", input),
    onSuccess: (created) => {
      client.setQueryData(["events", created.id], created)
      client.invalidateQueries({ queryKey: ["events"] })
    },
  })
}

export function useUpdateEvent(id: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (patch: Partial<CreateEventInput>) =>
      api.patch<EventModel>(`/api/events/${id}`, patch),
    // Optimistic: apply the patch to the detail cache, roll back on error.
    onMutate: async (patch) => {
      await client.cancelQueries({ queryKey: ["events", id] })
      const previous = client.getQueryData<EventDetail>(["events", id])
      if (previous) {
        client.setQueryData(["events", id], { ...previous, ...patch })
      }
      return { previous }
    },
    onError: (_err, _patch, context) => {
      if (context?.previous) client.setQueryData(["events", id], context.previous)
    },
    onSettled: () => {
      client.invalidateQueries({ queryKey: ["events"] })
    },
  })
}

export function useDeleteEvent() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/events/${id}`),
    onSuccess: (_data, id) => {
      client.removeQueries({ queryKey: ["events", id] })
      client.invalidateQueries({ queryKey: ["events"] })
    },
  })
}

export function useAddSubEvent(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: SubEventInput) =>
      api.post<SubEvent>(`/api/events/${eventId}/sub-events`, input),
    onSuccess: () => client.invalidateQueries({ queryKey: ["events"] }),
  })
}

export function useUpdateSubEvent(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...patch }: Partial<SubEventInput> & { id: string }) =>
      api.patch<SubEvent>(`/api/sub-events/${id}`, patch),
    onSuccess: () => client.invalidateQueries({ queryKey: ["events", eventId] }),
  })
}

export function useDeleteSubEvent(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/sub-events/${id}`),
    onSuccess: () => client.invalidateQueries({ queryKey: ["events", eventId] }),
  })
}

export function useGuests(eventId: string) {
  return useQuery({
    queryKey: ["guests", eventId],
    queryFn: () => api.get<Guest[]>(`/api/events/${eventId}/guests`),
  })
}

export function useCreateGuest(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateGuestInput) =>
      api.post<Guest>(`/api/events/${eventId}/guests`, input),
    onSuccess: () => client.invalidateQueries({ queryKey: ["guests", eventId] }),
  })
}

export function useUpdateGuest(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...patch }: GuestPatch & { id: string }) =>
      api.patch<Guest>(`/api/guests/${id}`, patch),
    // Optimistic: the server normalizes phone numbers, so onSettled below
    // refetches and the displayed value snaps to whatever it actually stored.
    onMutate: async ({ id, ...patch }) => {
      await client.cancelQueries({ queryKey: ["guests", eventId] })
      const previous = client.getQueryData<Guest[]>(["guests", eventId])
      client.setQueryData<Guest[]>(["guests", eventId], (guests) =>
        guests?.map((g) => (g.id === id ? { ...g, ...patch } : g)),
      )
      return { previous }
    },
    onError: (_err, _patch, context) => {
      if (context?.previous) client.setQueryData(["guests", eventId], context.previous)
    },
    onSettled: () => client.invalidateQueries({ queryKey: ["guests", eventId] }),
  })
}

export function useDeleteGuest(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/guests/${id}`),
    onMutate: async (id) => {
      await client.cancelQueries({ queryKey: ["guests", eventId] })
      const previous = client.getQueryData<Guest[]>(["guests", eventId])
      client.setQueryData<Guest[]>(["guests", eventId], (guests) =>
        guests?.filter((g) => g.id !== id),
      )
      return { previous }
    },
    onError: (_err, _id, context) => {
      if (context?.previous) client.setQueryData(["guests", eventId], context.previous)
    },
    onSettled: () => client.invalidateQueries({ queryKey: ["guests", eventId] }),
  })
}

export function useImportGuests(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: ImportInput) =>
      api.post<ImportReport>(`/api/events/${eventId}/guests/import`, input),
    onSuccess: (report) => {
      // A dry run deliberately changed nothing, so leave the cache alone.
      if (!report.dryRun) client.invalidateQueries({ queryKey: ["guests", eventId] })
    },
  })
}

export function useTeam() {
  return useQuery({
    queryKey: ["team"],
    queryFn: () => api.get<TeamMember[]>("/api/team"),
  })
}

export function useInviteMember() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: InviteInput) => api.post<TeamMember>("/api/team", input),
    onSuccess: () => client.invalidateQueries({ queryKey: ["team"] }),
  })
}

export function useUpdateMember() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...patch }: { id: string; role?: Role; name?: string }) =>
      api.post<TeamMember>(`/api/team/${id}`, patch),
    onSuccess: () => client.invalidateQueries({ queryKey: ["team"] }),
  })
}

export function useRemoveMember() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/team/${id}`),
    onSuccess: () => client.invalidateQueries({ queryKey: ["team"] }),
  })
}

export function useReminders(eventId: string) {
  return useQuery({
    queryKey: ["reminders", eventId],
    queryFn: () => api.get<ReminderSchedule[]>(`/api/events/${eventId}/reminders`),
  })
}

export function useAddReminder(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (offsetMinutes: number) =>
      api.post<ReminderSchedule>(`/api/events/${eventId}/reminders`, { offsetMinutes }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["reminders", eventId] }),
  })
}

export function useDeleteReminder(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/reminders/${id}`),
    onSuccess: () => client.invalidateQueries({ queryKey: ["reminders", eventId] }),
  })
}

export function useVendors(eventId: string) {
  return useQuery({
    queryKey: ["vendors", eventId],
    queryFn: () => api.get<Vendor[]>(`/api/events/${eventId}/vendors`),
  })
}

export function useCreateVendor(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateVendorInput) =>
      api.post<Vendor>(`/api/events/${eventId}/vendors`, input),
    onSuccess: () => client.invalidateQueries({ queryKey: ["vendors", eventId] }),
  })
}

export function useUpdateVendor(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...patch }: VendorPatch & { id: string }) =>
      api.patch<Vendor>(`/api/vendors/${id}`, patch),
    onSuccess: () => client.invalidateQueries({ queryKey: ["vendors", eventId] }),
  })
}

export function useDeleteVendor(eventId: string) {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<void>(`/api/vendors/${id}`),
    onSuccess: () => client.invalidateQueries({ queryKey: ["vendors", eventId] }),
  })
}
