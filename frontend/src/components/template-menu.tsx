import { useQuery } from '@tanstack/react-query'
import { ChevronDown, LayoutTemplate } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { api, type Template, type TemplateKind } from '@/lib/api'

interface TemplateMenuProps<K extends TemplateKind> {
  /** Which templates to offer: a list page only starts its own kind. */
  kind: K
  onPick: (template: Extract<Template, { kind: K }>) => void
}

/**
 * "From template": the templates of one kind, next to a list's Add button.
 * Picking one opens the ordinary create editor with that template's settings
 * already filled in.
 *
 * Always shown, even with no templates yet — an empty menu that says where
 * they come from is how the feature gets found at all.
 */
export function TemplateMenu<K extends TemplateKind>({
  kind,
  onPick,
}: TemplateMenuProps<K>) {
  const { t } = useTranslation()
  // Shares the ['templates'] key with the Templates page and the editors that
  // save one, so a template made a moment ago is already listed here.
  const templatesQuery = useQuery({
    queryKey: ['templates'],
    queryFn: () => api.listTemplates(),
  })
  const templates = (templatesQuery.data?.templates ?? []).filter(
    (template): template is Extract<Template, { kind: K }> =>
      template.kind === kind,
  )

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">
          <LayoutTemplate aria-hidden="true" />
          {t('templates.fromTemplate')}
          <ChevronDown aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-w-xs">
        {templates.length === 0 ? (
          <p className="px-2 py-1.5 text-sm text-muted-foreground">
            {t('templates.noneHint')}
          </p>
        ) : (
          templates.map((template) => (
            <DropdownMenuItem key={template.id} onSelect={() => onPick(template)}>
              {template.name}
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
