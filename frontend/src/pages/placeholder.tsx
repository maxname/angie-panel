import { useTranslation } from 'react-i18next'

import { Card, CardContent } from '@/components/ui/card'

interface PlaceholderPageProps {
  titleKey: string
  milestone: 'M1' | 'M2'
}

export function PlaceholderPage({ titleKey, milestone }: PlaceholderPageProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">{t(titleKey)}</h1>
      <Card>
        <CardContent className="py-12 text-center">
          <p className="text-sm text-muted-foreground">
            {t('placeholder.note', { milestone })}
          </p>
        </CardContent>
      </Card>
    </div>
  )
}
