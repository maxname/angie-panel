import { ApiError } from '@/lib/api'

export interface BulkFailure<T> {
  item: T
  message: string
}

export interface BulkResult<T> {
  succeeded: T[]
  failed: BulkFailure<T>[]
}

/**
 * Apply one action to many rows: one ordinary request per row, one after
 * another, collecting which rows went through and why the others did not.
 *
 * Per row, not one bulk endpoint, so every row passes exactly the checks, and
 * leaves exactly the audit entry, that doing it by hand would — "this
 * certificate is still used by host X" stays an answer about that one row.
 *
 * One after another, not in parallel, because some of those checks read state
 * the previous request has just written: enabling two hosts that claim the
 * same domain must let the first win and refuse the second, and two
 * concurrent requests could each pass the check before either had saved.
 *
 * A failure does not stop the run — the rest of the selection is still worth
 * doing. The exception is a lost session: every request after a 401 would
 * fail the same way, so the rows not yet tried are reported as such at once.
 */
export async function runBulk<T>(
  items: readonly T[],
  action: (item: T) => Promise<unknown>,
  options: {
    fallbackMessage: string
    onProgress?: (done: number) => void
  },
): Promise<BulkResult<T>> {
  const result: BulkResult<T> = { succeeded: [], failed: [] }
  let unauthorized: string | null = null

  for (const [index, item] of items.entries()) {
    if (unauthorized !== null) {
      result.failed.push({ item, message: unauthorized })
    } else {
      try {
        await action(item)
        result.succeeded.push(item)
      } catch (error) {
        const message =
          error instanceof ApiError ? error.message : options.fallbackMessage
        result.failed.push({ item, message })
        if (error instanceof ApiError && error.status === 401) {
          unauthorized = message
        }
      }
    }
    options.onProgress?.(index + 1)
  }
  return result
}

export type BulkActionKind = 'enable' | 'disable' | 'delete'

export interface BulkAction<T> {
  kind: BulkActionKind
  /** The single-row request — the same one the row's own menu sends. */
  run: (item: T) => Promise<unknown>
  /** Which of the ticked rows this action has anything to do for. Rows it
   *  does not apply to are left alone (and stay out of the count). */
  appliesTo?: (item: T) => boolean
}

/** Enable / disable / delete for a list whose rows can be switched off. */
export function lifecycleActions<T extends { id: number; enabled: boolean }>(api: {
  enable: (id: number) => Promise<unknown>
  disable: (id: number) => Promise<unknown>
  remove: (id: number) => Promise<unknown>
}): BulkAction<T>[] {
  return [
    {
      kind: 'enable',
      run: (item) => api.enable(item.id),
      appliesTo: (item) => !item.enabled,
    },
    {
      kind: 'disable',
      run: (item) => api.disable(item.id),
      appliesTo: (item) => item.enabled,
    },
    { kind: 'delete', run: (item) => api.remove(item.id) },
  ]
}

/** Delete only, for a list whose rows have no on/off state. */
export function deleteAction<T extends { id: number }>(
  remove: (id: number) => Promise<unknown>,
): BulkAction<T>[] {
  return [{ kind: 'delete', run: (item) => remove(item.id) }]
}
