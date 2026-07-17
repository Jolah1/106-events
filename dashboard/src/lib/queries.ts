import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import { api, ApiError } from "@/lib/api"
import type {
  AppConfig,
  CreateEventInput,
  Event as EventModel,
  EventDetail,
  EventSummary,
  SubEvent,
  SubEventInput,
  User,
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
