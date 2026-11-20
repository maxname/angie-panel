import { describe, expect, it } from 'vitest'

import { formatAuthority } from './authority'

describe('formatAuthority', () => {
  it('brackets an IPv6 literal', () => {
    expect(formatAuthority('2a01:4f8::1', 8080)).toBe('[2a01:4f8::1]:8080')
    expect(formatAuthority('::1', 80)).toBe('[::1]:80')
    expect(formatAuthority('::ffff:10.0.0.1', 443)).toBe('[::ffff:10.0.0.1]:443')
  })

  it('leaves IPv4 and hostnames alone', () => {
    expect(formatAuthority('10.0.0.5', 8080)).toBe('10.0.0.5:8080')
    expect(formatAuthority('nas.lan', 443)).toBe('nas.lan:443')
    // All hex digits, but a hostname, not an address.
    expect(formatAuthority('cafe', 80)).toBe('cafe:80')
  })

  it('does not bracket twice', () => {
    // Only reachable from a form field mid-edit; the API never returns this.
    expect(formatAuthority('[2a01:4f8::1]', '8080')).toBe('[2a01:4f8::1]:8080')
  })
})
