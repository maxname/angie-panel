import { act, renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { useSelection } from './use-selection'

const rows = [{ id: 1 }, { id: 2 }, { id: 3 }]

describe('useSelection', () => {
  it('ticks and unticks single rows, in list order', () => {
    const { result } = renderHook(() => useSelection(rows))
    expect(result.current.selected).toEqual([])
    expect(result.current.some).toBe(false)
    expect(result.current.all).toBe(false)

    act(() => result.current.toggle(3))
    act(() => result.current.toggle(1))
    // List order, not click order.
    expect(result.current.selected).toEqual([{ id: 1 }, { id: 3 }])
    expect(result.current.isSelected(1)).toBe(true)
    expect(result.current.isSelected(2)).toBe(false)
    expect(result.current.some).toBe(true)
    expect(result.current.all).toBe(false)

    act(() => result.current.toggle(1))
    expect(result.current.selected).toEqual([{ id: 3 }])
  })

  it('select-all ticks everything, and unticks everything once all are ticked', () => {
    const { result } = renderHook(() => useSelection(rows))

    act(() => result.current.toggle(2))
    // From a partial selection, "all" completes it rather than clearing it.
    act(() => result.current.toggleAll())
    expect(result.current.selected).toHaveLength(3)
    expect(result.current.all).toBe(true)
    expect(result.current.some).toBe(false)

    act(() => result.current.toggleAll())
    expect(result.current.selected).toEqual([])
  })

  it('forgets a ticked row that has left the list', () => {
    // Deleted here or by someone else between two refetches: it must not
    // stay counted, and no action may run against it.
    const { result, rerender } = renderHook(({ list }) => useSelection(list), {
      initialProps: { list: rows },
    })
    act(() => result.current.toggle(1))
    act(() => result.current.toggle(2))

    rerender({ list: [{ id: 2 }, { id: 3 }] })

    expect(result.current.selected).toEqual([{ id: 2 }])
    // With the stale id gone, ticking the one other row completes the list.
    act(() => result.current.toggle(3))
    expect(result.current.all).toBe(true)
  })

  it('can be replaced wholesale and cleared', () => {
    const { result } = renderHook(() => useSelection(rows))

    act(() => result.current.set([2, 3]))
    expect(result.current.selected).toEqual([{ id: 2 }, { id: 3 }])

    act(() => result.current.clear())
    expect(result.current.selected).toEqual([])
  })

  it('an empty list is never "all selected"', () => {
    const { result } = renderHook(() => useSelection([] as { id: number }[]))
    expect(result.current.all).toBe(false)
    expect(result.current.some).toBe(false)
  })
})
