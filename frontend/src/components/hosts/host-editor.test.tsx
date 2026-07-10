import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import { isValidDomain } from '@/lib/domain'

import { HostEditorForm } from './host-editor-dialog'

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

function renderForm() {
  const queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <HostEditorForm host={null} onDone={() => {}} />
    </QueryClientProvider>,
  )
}

describe('host editor validation', () => {
  it('accepts FQDNs and rejects junk in isValidDomain', () => {
    expect(isValidDomain('example.com')).toBe(true)
    expect(isValidDomain('*.example.com')).toBe(true)
    expect(isValidDomain('not a domain')).toBe(false)
    expect(isValidDomain('example')).toBe(false)
  })

  it('blocks submit and shows an error when no domains are entered', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    renderForm()

    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Add at least one domain name.'),
    ).toBeInTheDocument()
    // Nothing was submitted to the server.
    expect(fetchMock).not.toHaveBeenCalled()

    vi.unstubAllGlobals()
  })

  it('rejects an invalid domain when adding a chip', async () => {
    const user = userEvent.setup()
    renderForm()

    await user.type(screen.getByLabelText('Domain names'), 'not valid')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    expect(
      await screen.findByText('That does not look like a valid domain name.'),
    ).toBeInTheDocument()
  })
})
