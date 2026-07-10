import { X } from "lucide-react"

import { cn } from "@/lib/utils"
import { dismissToast, useToasts, type ToastVariant } from "@/lib/toast"

const variantClasses: Record<ToastVariant, string> = {
  default: "bg-popover text-popover-foreground ring-1 ring-foreground/10",
  success:
    "bg-emerald-600/10 text-emerald-800 ring-1 ring-emerald-600/30 dark:bg-emerald-400/10 dark:text-emerald-200 dark:ring-emerald-400/30",
  destructive:
    "bg-destructive/10 text-destructive ring-1 ring-destructive/30",
}

export function Toaster() {
  const toasts = useToasts()

  if (toasts.length === 0) {
    return null
  }

  return (
    <div
      className="pointer-events-none fixed inset-x-0 bottom-0 z-[100] flex flex-col items-center gap-2 p-4 sm:right-0 sm:left-auto sm:items-end"
      role="region"
      aria-label="Notifications"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role="status"
          className={cn(
            "pointer-events-auto flex w-full max-w-sm items-start gap-3 rounded-xl px-4 py-3 shadow-lg",
            variantClasses[toast.variant]
          )}
        >
          <div className="min-w-0 flex-1 space-y-1">
            <p className="text-sm font-medium">{toast.title}</p>
            {toast.description !== undefined && (
              <p className="text-sm opacity-80">{toast.description}</p>
            )}
          </div>
          <button
            type="button"
            onClick={() => dismissToast(toast.id)}
            className="-mr-1 shrink-0 rounded-md opacity-70 transition-opacity hover:opacity-100"
            aria-label="Dismiss"
          >
            <X className="size-4" aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>
  )
}
