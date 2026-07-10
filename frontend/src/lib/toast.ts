import { useSyncExternalStore } from 'react'

export type ToastVariant = 'default' | 'success' | 'destructive'

export interface ToastItem {
  id: number
  title: string
  description?: string
  variant: ToastVariant
}

export interface ToastOptions {
  title: string
  description?: string
  variant?: ToastVariant
  /** Auto-dismiss delay in ms. Pass 0 to keep it until dismissed manually. */
  duration?: number
}

const DEFAULT_DURATION = 5000

let items: ToastItem[] = []
let nextId = 1
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) {
    listener()
  }
}

export function toast(options: ToastOptions): number {
  const id = nextId++
  const item: ToastItem = {
    id,
    title: options.title,
    description: options.description,
    variant: options.variant ?? 'default',
  }
  items = [...items, item]
  emit()

  const duration = options.duration ?? DEFAULT_DURATION
  if (duration > 0) {
    setTimeout(() => dismissToast(id), duration)
  }
  return id
}

export function dismissToast(id: number): void {
  const next = items.filter((item) => item.id !== id)
  if (next.length !== items.length) {
    items = next
    emit()
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): ToastItem[] {
  return items
}

export function useToasts(): ToastItem[] {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

/** Test-only: clears every active toast. */
export function resetToasts(): void {
  items = []
  emit()
}
