import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

// jsdom lacks ResizeObserver, which some Radix primitives (Switch, Select)
// mount on render. Provide a no-op polyfill so components can render in tests.
if (!('ResizeObserver' in globalThis)) {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver =
    ResizeObserverStub as unknown as typeof ResizeObserver
}

// jsdom implements neither the Pointer Capture API nor scrollIntoView, which
// Radix Select uses when opening its listbox. Stub them so a Select can be
// driven in tests (open → pick an option).
const proto = globalThis.HTMLElement?.prototype
if (proto && !proto.hasPointerCapture) {
  proto.hasPointerCapture = () => false
  proto.setPointerCapture = () => {}
  proto.releasePointerCapture = () => {}
  proto.scrollIntoView = () => {}
}

// jsdom has no matchMedia. ThemeProvider reads it to follow the OS, and
// useIsMobile — which SidebarProvider mounts — calls it on every render, so any
// component tree containing a Sidebar throws without this. Reports "not
// matching": tests then see the desktop layout, which is what their queries
// assume.
if (!globalThis.matchMedia) {
  globalThis.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia
}

afterEach(() => {
  cleanup()
})
