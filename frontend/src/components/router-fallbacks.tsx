import { Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export function RouterPending() {
  return (
    <div className="flex min-h-svh items-center justify-center">
      <Loader2 className="size-6 animate-spin text-muted-foreground" aria-hidden="true" />
    </div>
  )
}

export function RouterError({ error }: { error: Error }) {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-2 p-6 text-center">
      <p className="text-sm font-medium">{t('common.error')}</p>
      <p className="text-sm text-muted-foreground">{error.message}</p>
    </div>
  )
}
