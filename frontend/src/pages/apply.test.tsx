import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { ApplyPreview } from '@/lib/api'

import { ApplyPage } from './apply'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const preview: ApplyPreview = {
  db_revision: 7,
  diff: {
    files: [
      {
        name: '01-example.com.conf',
        status: 'modified',
        unified:
          '--- a/01-example.com.conf\n+++ b/01-example.com.conf\n@@ -1,2 +1,2 @@\n-listen 8080;\n+listen 80;',
        drift: false,
      },
    ],
    foreign: [{ name: 'legacy.conf' }],
    added: 2,
    modified: 3,
    removed: 1,
    unchanged: 5,
    has_drift: false,
  },
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('apply page', () => {
  it('renders the diff counts and file diff from a mocked preview', async () => {
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/apply/preview') {
        return Promise.resolve(jsonResponse(preview))
      }
      if (input === '/api/apply/history') {
        return Promise.resolve(jsonResponse({ history: [] }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <ApplyPage />
      </QueryClientProvider>,
    )

    // Diff summary counts (distinct values, one per category).
    expect(await screen.findByText('2')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('1')).toBeInTheDocument()
    expect(screen.getByText('5')).toBeInTheDocument()
    expect(screen.getByText('added')).toBeInTheDocument()
    expect(screen.getByText('modified')).toBeInTheDocument()

    // The changed file and its diff render.
    expect(screen.getByText('01-example.com.conf')).toBeInTheDocument()
    expect(
      screen.getByText((content) => content.includes('+listen 80;')),
    ).toBeInTheDocument()
  })
})
