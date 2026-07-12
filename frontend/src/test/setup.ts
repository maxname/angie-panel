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

afterEach(() => {
  cleanup()
})
