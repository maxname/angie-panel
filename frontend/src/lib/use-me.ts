import { useQuery } from '@tanstack/react-query'

import { api } from '@/lib/api'

/** The current operator (email + role), cached across the app. */
export function useMe() {
  return useQuery({
    queryKey: ['auth', 'me'],
    queryFn: () => api.me(),
    staleTime: 5 * 60 * 1000,
    retry: false,
  })
}

/** True when the current operator is an admin. Undefined-safe (defaults false). */
export function useIsAdmin(): boolean {
  const { data } = useMe()
  return data?.role === 'admin'
}
