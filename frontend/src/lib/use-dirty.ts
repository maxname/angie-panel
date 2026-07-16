import { useMemo } from 'react'

/**
 * Whether a form still matches what it was opened with — the answer to "is
 * there anything to save?".
 *
 * Compares by JSON, which reads as wasteful per keystroke and isn't: measured
 * at 0.7µs for a typical host and 11.6µs for the heaviest one a user can build,
 * against a 16.7ms frame, and it rides along with a re-render that costs far
 * more. It also compares what we'd actually send, rather than a hand-rolled
 * field list that drifts the moment someone adds a field.
 *
 * `initial` must be stable — a snapshot taken once (`useState(() => …)`) or a
 * memo over the server's copy — or every render looks dirty.
 */
export function useIsDirty<T>(form: T, initial: T): boolean {
  const snapshot = useMemo(() => JSON.stringify(initial), [initial])
  return useMemo(() => JSON.stringify(form) !== snapshot, [form, snapshot])
}
