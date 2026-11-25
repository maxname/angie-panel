import { useCallback, useMemo, useState } from 'react'

/**
 * Which rows of a list are ticked, for the bulk actions.
 *
 * The answer is always computed against the rows currently on screen: an id
 * that was ticked and has since left the list — deleted here, or by someone
 * else between two refetches — simply stops counting, with no effect needed
 * to prune it and no way for an action to run against a row that is gone.
 */
export interface Selection<T> {
  /** The ticked rows, in list order. */
  selected: T[]
  isSelected: (id: number) => boolean
  toggle: (id: number) => void
  /** Tick every row, or untick them all when every row already is. */
  toggleAll: () => void
  /** Replace the selection — after a bulk run, with the rows that failed. */
  set: (ids: readonly number[]) => void
  clear: () => void
  /** Every row is ticked (false for an empty list). */
  all: boolean
  /** Some rows are ticked, but not all of them. */
  some: boolean
}

export function useSelection<T extends { id: number }>(
  rows: readonly T[],
): Selection<T> {
  const [ids, setIds] = useState<ReadonlySet<number>>(() => new Set())

  const selected = useMemo(
    () => rows.filter((row) => ids.has(row.id)),
    [rows, ids],
  )
  const all = rows.length > 0 && selected.length === rows.length

  const toggle = useCallback((id: number) => {
    setIds((prev) => {
      const next = new Set(prev)
      if (!next.delete(id)) {
        next.add(id)
      }
      return next
    })
  }, [])

  const toggleAll = useCallback(() => {
    setIds(all ? new Set() : new Set(rows.map((row) => row.id)))
  }, [all, rows])

  const set = useCallback((next: readonly number[]) => {
    setIds(new Set(next))
  }, [])

  const clear = useCallback(() => setIds(new Set()), [])

  return {
    selected,
    isSelected: (id) => ids.has(id),
    toggle,
    toggleAll,
    set,
    clear,
    all,
    some: selected.length > 0 && !all,
  }
}
