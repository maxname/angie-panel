import { describe, expect, it, vi } from 'vitest'

import { createQueryClient } from './query-client'

describe('createQueryClient', () => {
  it('refreshes the apply preview after any mutation', async () => {
    // The header count and sidebar badge live on ['apply','preview']. Before
    // this, only the Apply page invalidated it, so every other edit in the app
    // left them stale until a 20s poll — the operator added a host and the
    // header went on saying the config was applied.
    const client = createQueryClient()
    const invalidate = vi.spyOn(client, 'invalidateQueries')

    await client.getMutationCache().build(client, { mutationFn: async () => 'ok' }).execute(undefined)

    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['apply'] })
  })

  it('leaves the apply preview alone when a mutation fails', async () => {
    // Nothing changed server-side, so there is nothing to re-count.
    const client = createQueryClient()
    const invalidate = vi.spyOn(client, 'invalidateQueries')

    await client
      .getMutationCache()
      .build(client, {
        mutationFn: async () => {
          throw new Error('nope')
        },
        retry: false,
      })
      .execute(undefined)
      .catch(() => undefined)

    expect(invalidate).not.toHaveBeenCalled()
  })
})
