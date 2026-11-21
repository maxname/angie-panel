import { describe, expect, it, vi } from 'vitest'

import { ApiError } from '@/lib/api'

import { deleteAction, lifecycleActions, runBulk } from './bulk'

describe('runBulk', () => {
  it('sends one request per row, each only after the one before has settled', async () => {
    // Enabling two hosts that claim the same domain has to let the first win
    // and refuse the second; in parallel both could pass the check. So the
    // second request must not even start until the first is done.
    const events: string[] = []
    const action = vi.fn(async (id: number) => {
      events.push(`start ${id}`)
      await new Promise((resolve) => setTimeout(resolve, id === 1 ? 20 : 0))
      events.push(`end ${id}`)
    })

    const result = await runBulk([1, 2, 3], action, { fallbackMessage: 'x' })

    expect(events).toEqual([
      'start 1',
      'end 1',
      'start 2',
      'end 2',
      'start 3',
      'end 3',
    ])
    expect(result).toEqual({ succeeded: [1, 2, 3], failed: [] })
  })

  it('carries on past a failure and reports each one with the server’s reason', async () => {
    const progress: number[] = []
    const result = await runBulk(
      ['a', 'b', 'c', 'd'],
      async (item) => {
        if (item === 'b') {
          throw new ApiError(409, 'cert_in_use', 'certificate is used by proxy #1')
        }
        if (item === 'c') {
          throw new TypeError('network down')
        }
      },
      { fallbackMessage: 'Something went wrong', onProgress: (n) => progress.push(n) },
    )

    expect(result.succeeded).toEqual(['a', 'd'])
    expect(result.failed).toEqual([
      { item: 'b', message: 'certificate is used by proxy #1' },
      // Not an API answer: nothing worth showing verbatim.
      { item: 'c', message: 'Something went wrong' },
    ])
    expect(progress).toEqual([1, 2, 3, 4])
  })

  it('stops sending once the session is gone', async () => {
    // After a 401 every further request would fail the same way (and the app
    // is already on its way to the login page).
    const action = vi.fn(async (item: string) => {
      if (item !== 'a') {
        throw new ApiError(401, 'unauthorized', 'authentication required')
      }
    })

    const result = await runBulk(['a', 'b', 'c', 'd'], action, {
      fallbackMessage: 'x',
    })

    expect(action).toHaveBeenCalledTimes(2)
    expect(result.succeeded).toEqual(['a'])
    expect(result.failed.map((f) => f.item)).toEqual(['b', 'c', 'd'])
    expect(result.failed.every((f) => f.message === 'authentication required')).toBe(
      true,
    )
  })
})

describe('bulk action sets', () => {
  const on = { id: 1, enabled: true }
  const off = { id: 2, enabled: false }

  it('enable is for rows that are off, disable for rows that are on, delete for all', async () => {
    const api = {
      enable: vi.fn().mockResolvedValue(undefined),
      disable: vi.fn().mockResolvedValue(undefined),
      remove: vi.fn().mockResolvedValue(undefined),
    }
    const [enable, disable, remove] = lifecycleActions(api)

    expect([enable.kind, disable.kind, remove.kind]).toEqual([
      'enable',
      'disable',
      'delete',
    ])
    expect([on, off].filter(enable.appliesTo!)).toEqual([off])
    expect([on, off].filter(disable.appliesTo!)).toEqual([on])
    expect(remove.appliesTo).toBeUndefined()

    await enable.run(off)
    await disable.run(on)
    await remove.run(on)
    expect(api.enable).toHaveBeenCalledWith(2)
    expect(api.disable).toHaveBeenCalledWith(1)
    expect(api.remove).toHaveBeenCalledWith(1)
  })

  it('a list with no on/off state gets delete alone', async () => {
    const remove = vi.fn().mockResolvedValue(undefined)
    const actions = deleteAction(remove)

    expect(actions.map((a) => a.kind)).toEqual(['delete'])
    await actions[0].run({ id: 7 })
    expect(remove).toHaveBeenCalledWith(7)
  })
})
