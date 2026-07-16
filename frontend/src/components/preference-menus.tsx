import { Languages, Monitor, Moon, Sun } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  changeLanguage,
  LANGUAGE_NAMES,
  SUPPORTED_LANGUAGES,
  type Language,
} from '@/i18n'
import { THEMES, useTheme, type Theme } from '@/theme/theme-context'

// Language and theme belong to the person, not to the session, so they live
// here rather than in the app shell: the login and setup screens need them
// just as much — arguably more, since that's where someone lands in the wrong
// language with no way through, or gets a white page at 2am.

const THEME_ICONS = { light: Sun, dark: Moon, system: Monitor } as const

/** Both menus report the current choice through their icon and mark it with a
 *  tick, rather than naming a "next" — with three themes there isn't one, and
 *  the language list is meant to grow. */
export function LanguageMenu() {
  const { t, i18n } = useTranslation()
  const current = (i18n.resolvedLanguage ?? 'en') as Language

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('header.language')}
            >
              <Languages aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{t('header.language')}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup
          value={current}
          onValueChange={(value) => void changeLanguage(value as Language)}
        >
          {SUPPORTED_LANGUAGES.map((language) => (
            <DropdownMenuRadioItem key={language} value={language}>
              {LANGUAGE_NAMES[language]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function ThemeMenu() {
  const { t } = useTranslation()
  const { theme, setTheme } = useTheme()
  const ThemeIcon = THEME_ICONS[theme]

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label={t('header.theme')}>
              <ThemeIcon aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{t('header.theme')}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup
          value={theme}
          onValueChange={(value) => setTheme(value as Theme)}
        >
          {THEMES.map((option) => {
            const Icon = THEME_ICONS[option]
            return (
              <DropdownMenuRadioItem key={option} value={option}>
                <Icon aria-hidden="true" />
                {t(`header.themes.${option}`)}
              </DropdownMenuRadioItem>
            )
          })}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
