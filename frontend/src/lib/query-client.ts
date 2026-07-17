import { MutationCache, QueryClient } from '@tanstack/react-query'

/**
 * The app's QueryClient.
 *
 * Every mutation invalidates ['apply'] on success, from here rather than from
 * each call site. The header's "unapplied changes" count and the sidebar badge
 * read ['apply','preview'], and nothing told them anything had changed: of the
 * ~40 mutations in the app, only the Apply page itself invalidated that key. So
 * adding a host, a stream, a certificate — anything — left the count stale until
 * its own 20s poll came round, and the operator saw no sign their edit had
 * landed anywhere.
 *
 * Doing it per-mutation is what produced that: every new page had to remember,
 * and every page but one forgot. Here it cannot be forgotten. The cost is that
 * mutations which touch no config (logging in, editing a user) also refetch the
 * preview — one cheap GET, and only while a component is actually watching the
 * key, so the login screen fetches nothing.
 */
export function createQueryClient(): QueryClient {
  const client: QueryClient = new QueryClient({
    mutationCache: new MutationCache({
      // Runs after the mutation's own onSuccess. `client` is initialised by
      // then — this closure only runs once a mutation has resolved.
      onSuccess: () => {
        void client.invalidateQueries({ queryKey: ['apply'] })
      },
    }),
    defaultOptions: {
      queries: {
        retry: 1,
        refetchOnWindowFocus: false,
      },
    },
  })
  return client
}
