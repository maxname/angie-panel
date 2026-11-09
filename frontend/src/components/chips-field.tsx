import { Plus, X } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

export interface ChipsFieldProps {
  /** Goes on the input, so a caller can focus it when submit fails. */
  id: string
  label: string
  values: string[]
  onChange: (values: string[]) => void
  addLabel: string
  removeLabel: (value: string) => string
  placeholder?: string
  /** Applied before validating and storing — e.g. lowercase a domain. */
  normalize?: (raw: string) => string
  /** The message to show, or null when the value is acceptable. */
  validate?: (value: string, existing: string[]) => string | null
  /** Id of a hint paragraph to tie to the input. */
  describedBy?: string
}

/**
 * A list you build one entry at a time: type, Enter, it becomes a chip.
 *
 * The mechanics — draft state, Enter/comma to commit, inline error, remove —
 * are the same wherever this shape appears, so they live here once. What
 * differs is what counts as a valid entry, which the caller supplies.
 */
export function ChipsField({
  id,
  label,
  values,
  onChange,
  addLabel,
  removeLabel,
  placeholder,
  normalize = (raw) => raw.trim(),
  validate,
  describedBy,
}: ChipsFieldProps) {
  const [draft, setDraft] = useState('')
  const [error, setError] = useState<string | null>(null)
  const errorId = `${id}-error`

  const add = () => {
    const candidate = normalize(draft)
    if (candidate === '') {
      return
    }
    const problem = validate?.(candidate, values) ?? null
    if (problem !== null) {
      setError(problem)
      return
    }
    onChange([...values, candidate])
    setDraft('')
    setError(null)
  }

  const remove = (value: string) =>
    onChange(values.filter((item) => item !== value))

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      {values.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {values.map((value) => (
            <span
              key={value}
              className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-sm"
            >
              {value}
              <button
                type="button"
                onClick={() => remove(value)}
                className="text-muted-foreground hover:text-foreground"
                aria-label={removeLabel(value)}
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="flex gap-2">
        <Input
          id={id}
          value={draft}
          placeholder={placeholder}
          aria-invalid={error !== null}
          aria-describedby={
            [error !== null ? errorId : null, describedBy]
              .filter(Boolean)
              .join(' ') || undefined
          }
          onChange={(event) => {
            setDraft(event.target.value)
            setError(null)
          }}
          onKeyDown={(event) => {
            // Enter must not reach the surrounding form: in a dialog it would
            // submit the whole thing instead of adding the entry.
            if (event.key === 'Enter' || event.key === ',') {
              event.preventDefault()
              add()
            }
          }}
        />
        <Button type="button" variant="outline" onClick={add}>
          <Plus aria-hidden="true" />
          {addLabel}
        </Button>
      </div>
      {error !== null && (
        <p id={errorId} role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}
