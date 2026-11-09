import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { beforeAll, describe, expect, it } from 'vitest'

import i18n from '@/i18n'

import { LoginForm } from './login-form'

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

function renderLoginForm() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

  return render(
    <QueryClientProvider client={queryClient}>
      <LoginForm onSuccess={() => {}} />
    </QueryClientProvider>,
  )
}

describe('login page form', () => {
  it('renders email, password and submit controls', () => {
    renderLoginForm()

    expect(screen.getByLabelText('Email')).toBeInTheDocument()
    expect(screen.getByLabelText('Password')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument()
  })
})
