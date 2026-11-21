import { Loader2, X } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { TableCell, TableHead } from '@/components/ui/table'
import {
  runBulk,
  type BulkAction,
  type BulkActionKind,
  type BulkFailure,
} from '@/lib/bulk'
import { toast } from '@/lib/toast'
import type { Selection } from '@/lib/use-selection'

/** The "select all" box: ticked, a dash while only some rows are, or empty. */
export function SelectAllCheckbox<T>({ selection }: { selection: Selection<T> }) {
  const { t } = useTranslation()
  return (
    <Checkbox
      checked={selection.all ? true : selection.some ? 'indeterminate' : false}
      onCheckedChange={selection.toggleAll}
      aria-label={t('bulk.selectAll')}
    />
  )
}

interface SelectRowProps<T extends { id: number }> {
  selection: Selection<T>
  item: T
  /** What the row is called, for the checkbox's accessible name. */
  name: string
}

export function SelectRowCheckbox<T extends { id: number }>({
  selection,
  item,
  name,
}: SelectRowProps<T>) {
  const { t } = useTranslation()
  return (
    <Checkbox
      checked={selection.isSelected(item.id)}
      onCheckedChange={() => selection.toggle(item.id)}
      aria-label={t('bulk.selectRow', { name })}
    />
  )
}

/** Leading header cell of a selectable table. */
export function SelectAllHead<T>({ selection }: { selection: Selection<T> }) {
  return (
    <TableHead className="w-0">
      <SelectAllCheckbox selection={selection} />
    </TableHead>
  )
}

/** Leading cell of a selectable table row. */
export function SelectRowCell<T extends { id: number }>(props: SelectRowProps<T>) {
  return (
    <TableCell className="w-0">
      <SelectRowCheckbox {...props} />
    </TableCell>
  )
}

/** How many names a dialog spells out before it says "and N more". */
const NAMES_SHOWN = 8

interface BulkActionsBarProps<T extends { id: number }> {
  selection: Selection<T>
  /** How a row is named in the confirmation and in a failure report. */
  label: (item: T) => string
  actions: BulkAction<T>[]
  /** Called after a run in which at least one row changed — refetch here. */
  onChanged: () => void
  /** A line under the success message, for changes that still need applying. */
  successNote?: string
}

/**
 * The bar that appears under a list once rows are ticked: what is selected,
 * and what can be done to all of it at once.
 *
 * Under the list and sticky to the bottom of the viewport, rather than above
 * the rows: there it would push them down the moment the first box is
 * ticked, moving the next box out from under the pointer.
 */
export function BulkActionsBar<T extends { id: number }>({
  selection,
  label,
  actions,
  onChanged,
  successNote,
}: BulkActionsBarProps<T>) {
  const { t } = useTranslation()
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(
    null,
  )
  // A delete waits here for its confirmation. What a dialog shows and whether
  // it is open are kept apart on purpose: a dialog fades out for a moment
  // after it closes, and must go on showing what it was about rather than
  // blank out — "Delete the selected (0)?" — on its way off the screen.
  const [confirming, setConfirming] = useState<{
    action: BulkAction<T>
    targets: T[]
  } | null>(null)
  const [confirmOpen, setConfirmOpen] = useState(false)
  // What the last run could not do.
  const [report, setReport] = useState<{
    kind: BulkActionKind
    total: number
    failed: BulkFailure<T>[]
  } | null>(null)
  const [reportOpen, setReportOpen] = useState(false)

  const running = progress !== null
  const count = selection.selected.length

  const targetsOf = (action: BulkAction<T>) =>
    action.appliesTo
      ? selection.selected.filter(action.appliesTo)
      : selection.selected

  const execute = async (action: BulkAction<T>, targets: T[]) => {
    setProgress({ done: 0, total: targets.length })
    const result = await runBulk(targets, action.run, {
      fallbackMessage: t('common.error'),
      onProgress: (done) => setProgress({ done, total: targets.length }),
    })
    setProgress(null)

    if (result.succeeded.length > 0) {
      onChanged()
      toast({
        title: t(`bulk.done.${action.kind}`, { count: result.succeeded.length }),
        description: successNote,
      })
    }
    // What failed stays ticked, ready to retry once the cause is dealt with;
    // everything that went through is unticked.
    selection.set(result.failed.map((failure) => failure.item.id))
    if (result.failed.length > 0) {
      setReport({
        kind: action.kind,
        total: targets.length,
        failed: result.failed,
      })
      setReportOpen(true)
    }
  }

  const start = (action: BulkAction<T>) => {
    const targets = targetsOf(action)
    if (action.kind === 'delete') {
      setConfirming({ action, targets })
      setConfirmOpen(true)
    } else {
      void execute(action, targets)
    }
  }

  return (
    <>
      {(count > 0 || running) && (
        <div className="sticky bottom-4 z-30 flex justify-center">
          <div
            role="toolbar"
            aria-label={t('bulk.toolbar')}
            className="flex flex-wrap items-center gap-2 rounded-xl border bg-background px-3 py-2 shadow-lg"
          >
            <span className="px-1 text-sm font-medium" aria-live="polite">
              {progress !== null ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="size-4 animate-spin" aria-hidden="true" />
                  {t('bulk.progress', progress)}
                </span>
              ) : (
                t('bulk.selected', { count })
              )}
            </span>
            {actions.map((action) => (
              <Button
                key={action.kind}
                size="sm"
                variant={action.kind === 'delete' ? 'destructive' : 'outline'}
                // Nothing to do — e.g. "Enable" when every ticked row is on.
                disabled={running || targetsOf(action).length === 0}
                onClick={() => start(action)}
              >
                {t(`bulk.${action.kind}`)}
              </Button>
            ))}
            <Button
              size="icon-sm"
              variant="ghost"
              disabled={running}
              onClick={selection.clear}
              aria-label={t('bulk.clear')}
              title={t('bulk.clear')}
            >
              <X aria-hidden="true" />
            </Button>
          </div>
        </div>
      )}

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>
              {t('bulk.confirmDelete.title', {
                count: confirming?.targets.length ?? 0,
              })}
            </DialogTitle>
            <DialogDescription>{t('bulk.confirmDelete.body')}</DialogDescription>
          </DialogHeader>
          <NameList names={(confirming?.targets ?? []).map(label)} />
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (confirming !== null) {
                  setConfirmOpen(false)
                  void execute(confirming.action, confirming.targets)
                }
              }}
            >
              {t('bulk.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={reportOpen} onOpenChange={setReportOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {t(`bulk.failed.${report?.kind ?? 'delete'}`, {
                failed: report?.failed.length ?? 0,
                total: report?.total ?? 0,
              })}
            </DialogTitle>
            <DialogDescription>{t('bulk.failed.body')}</DialogDescription>
          </DialogHeader>
          <ul className="max-h-64 space-y-2 overflow-y-auto text-sm">
            {(report?.failed ?? []).map((failure) => (
              <li key={failure.item.id}>
                <span className="font-medium">{label(failure.item)}</span>
                <span className="text-muted-foreground"> — {failure.message}</span>
              </li>
            ))}
          </ul>
          <DialogFooter>
            <Button variant="outline" onClick={() => setReportOpen(false)}>
              {t('bulk.failed.ok')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

/** The names a destructive action is about to touch — the first few spelled
 *  out, so a long selection still fits a dialog. */
function NameList({ names }: { names: string[] }) {
  const { t } = useTranslation()
  const shown = names.slice(0, NAMES_SHOWN)
  const more = names.length - shown.length
  return (
    <ul className="space-y-1 font-mono text-xs">
      {shown.map((name, index) => (
        <li key={index}>{name}</li>
      ))}
      {more > 0 && (
        <li className="font-sans text-muted-foreground">
          {t('bulk.more', { count: more })}
        </li>
      )}
    </ul>
  )
}
