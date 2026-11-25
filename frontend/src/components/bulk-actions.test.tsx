import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import { ApiError } from '@/lib/api'
import { lifecycleActions } from '@/lib/bulk'
import { resetToasts } from '@/lib/toast'
import { useSelection } from '@/lib/use-selection'
import { Toaster } from '@/components/ui/toaster'

import {
  BulkActionsBar,
  SelectAllCheckbox,
  SelectRowCheckbox,
} from './bulk-actions'

interface Row {
  id: number
  name: string
  enabled: boolean
}

const ROWS: Row[] = [
  { id: 1, name: 'alpha', enabled: true },
  { id: 2, name: 'beta', enabled: false },
  { id: 3, name: 'gamma', enabled: false },
]

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  resetToasts()
})

function makeApi() {
  return {
    enable: vi.fn().mockResolvedValue(undefined),
    disable: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn().mockResolvedValue(undefined),
  }
}

/** A minimal selectable list: what every list page wires together. */
function Harness({
  api,
  onChanged,
  rows = ROWS,
}: {
  api: ReturnType<typeof makeApi>
  onChanged: () => void
  rows?: Row[]
}) {
  const selection = useSelection(rows)
  return (
    <>
      <SelectAllCheckbox selection={selection} />
      {rows.map((row) => (
        <SelectRowCheckbox
          key={row.id}
          selection={selection}
          item={row}
          name={row.name}
        />
      ))}
      <BulkActionsBar
        selection={selection}
        label={(row) => row.name}
        actions={lifecycleActions<Row>(api)}
        onChanged={onChanged}
        successNote="Go to Apply."
      />
      <Toaster />
    </>
  )
}

const box = (name: string) => screen.getByRole('checkbox', { name })
const toolbar = () => screen.getByRole('toolbar')

describe('bulk actions bar', () => {
  it('stays out of the way until a row is ticked', async () => {
    const user = userEvent.setup()
    render(<Harness api={makeApi()} onChanged={vi.fn()} />)

    expect(screen.queryByRole('toolbar')).toBeNull()

    await user.click(box('Select beta'))
    expect(within(toolbar()).getByText('Selected: 1')).toBeInTheDocument()
    // One of three: the "all" box shows the in-between state.
    expect(box('Select all')).toHaveAttribute('aria-checked', 'mixed')

    await user.click(screen.getByRole('button', { name: 'Clear selection' }))
    expect(screen.queryByRole('toolbar')).toBeNull()
    expect(box('Select beta')).not.toBeChecked()
  })

  it('enables only the ticked rows that are off, then unticks them', async () => {
    const user = userEvent.setup()
    const api = makeApi()
    const onChanged = vi.fn()
    render(<Harness api={api} onChanged={onChanged} />)

    await user.click(box('Select all'))
    expect(within(toolbar()).getByText('Selected: 3')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Enable' }))

    // alpha is already on: no request for it, and it is not in the count.
    expect(await screen.findByText('Enabled: 2')).toBeInTheDocument()
    expect(screen.getByText('Go to Apply.')).toBeInTheDocument()
    expect(api.enable.mock.calls).toEqual([[2], [3]])
    expect(api.disable).not.toHaveBeenCalled()
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('toolbar')).toBeNull()
    expect(box('Select all')).not.toBeChecked()
  })

  it('offers no action that has nothing to do', async () => {
    const user = userEvent.setup()
    render(<Harness api={makeApi()} onChanged={vi.fn()} />)

    // Only a row that is already on is ticked.
    await user.click(box('Select alpha'))
    expect(screen.getByRole('button', { name: 'Enable' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Disable' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Delete' })).toBeEnabled()
  })

  it('asks before deleting, naming what will go', async () => {
    const user = userEvent.setup()
    const api = makeApi()
    render(<Harness api={api} onChanged={vi.fn()} />)

    await user.click(box('Select alpha'))
    await user.click(box('Select gamma'))
    await user.click(screen.getByRole('button', { name: 'Delete' }))

    const dialog = await screen.findByRole('dialog', {
      name: 'Delete the selected (2)?',
    })
    expect(within(dialog).getByText('alpha')).toBeInTheDocument()
    expect(within(dialog).getByText('gamma')).toBeInTheDocument()
    expect(within(dialog).queryByText('beta')).toBeNull()
    // Nothing is sent until the confirmation.
    expect(api.remove).not.toHaveBeenCalled()

    // Backing out leaves everything, selection included, as it was.
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(api.remove).not.toHaveBeenCalled()
    expect(within(toolbar()).getByText('Selected: 2')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Delete' }))
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Delete',
      }),
    )
    expect(await screen.findByText('Deleted: 2')).toBeInTheDocument()
    expect(api.remove.mock.calls).toEqual([[1], [3]])
  })

  it('reports what could not be done and keeps exactly those rows ticked', async () => {
    const user = userEvent.setup()
    const api = makeApi()
    api.disable.mockImplementation((id: number) =>
      id === 1
        ? Promise.reject(new ApiError(409, 'in_use', 'alpha is still needed'))
        : Promise.resolve(),
    )
    const onChanged = vi.fn()
    const rows = ROWS.map((row) => ({ ...row, enabled: true }))
    render(<Harness api={api} onChanged={onChanged} rows={rows} />)

    await user.click(box('Select all'))
    await user.click(screen.getByRole('button', { name: 'Disable' }))

    const report = await screen.findByRole('dialog', {
      name: 'Could not disable 1 of 3',
    })
    // The server's own reason, next to the row it is about.
    expect(within(report).getByText('alpha')).toBeInTheDocument()
    expect(within(report).getByText(/alpha is still needed/)).toBeInTheDocument()
    // The two that went through still count: refetch, and say so.
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(screen.getByText('Disabled: 2')).toBeInTheDocument()

    await user.click(within(report).getByRole('button', { name: 'OK' }))
    // Only the failed row is left ticked, ready for another go.
    expect(box('Select alpha')).toBeChecked()
    expect(box('Select beta')).not.toBeChecked()
    expect(box('Select gamma')).not.toBeChecked()
    expect(within(toolbar()).getByText('Selected: 1')).toBeInTheDocument()
  })

  it('says nothing changed when every row failed', async () => {
    const user = userEvent.setup()
    const api = makeApi()
    api.remove.mockRejectedValue(new ApiError(403, 'forbidden', 'read-only access'))
    const onChanged = vi.fn()
    render(<Harness api={api} onChanged={onChanged} />)

    await user.click(box('Select beta'))
    await user.click(screen.getByRole('button', { name: 'Delete' }))
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Delete',
      }),
    )

    expect(
      await screen.findByRole('dialog', { name: 'Could not delete 1 of 1' }),
    ).toBeInTheDocument()
    expect(screen.getByText(/read-only access/)).toBeInTheDocument()
    // No refetch and no success message for a run that changed nothing.
    expect(onChanged).not.toHaveBeenCalled()
    expect(screen.queryByText(/Deleted:/)).toBeNull()
  })

  it('spells out only the first few names of a long selection', async () => {
    const user = userEvent.setup()
    const many = Array.from({ length: 11 }, (_, i) => ({
      id: i + 1,
      name: `row-${i + 1}`,
      enabled: true,
    }))
    render(<Harness api={makeApi()} onChanged={vi.fn()} rows={many} />)

    await user.click(box('Select all'))
    await user.click(screen.getByRole('button', { name: 'Delete' }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('row-8')).toBeInTheDocument()
    expect(within(dialog).queryByText('row-9')).toBeNull()
    expect(within(dialog).getByText('…and 3 more')).toBeInTheDocument()
  })
})
