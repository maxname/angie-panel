import { describe, expect, it } from 'vitest'

import { isValidIp } from './ip'

describe('isValidIp', () => {
  it('accepts IPv4', () => {
    for (const value of ['1.1.1.1', '8.8.8.8', '0.0.0.0', '255.255.255.255', '127.0.0.53']) {
      expect(isValidIp(value), value).toBe(true)
    }
  })

  it('accepts IPv6', () => {
    for (const value of [
      '::1',
      '::',
      '2001:4860:4860::8888',
      'fe80::1',
      'fd00:1234:5678:9abc:def0:1234:5678:9abc',
      '::ffff:1.2.3.4',
    ]) {
      expect(isValidIp(value), value).toBe(true)
    }
  })

  it('rejects what is not an address', () => {
    for (const value of [
      '',
      'localhost',
      'example.com',
      '1.1.1',
      '1.1.1.1.1',
      '256.1.1.1',
      '1.1.1.256',
      '-1.1.1.1',
      '1.1.1.1/24', // a CIDR is not a resolver address
      'gggg::1',
      '::1::2', // two "::" is ambiguous
      '1.2.3.4:53', // a port is not part of the directive we generate
    ]) {
      expect(isValidIp(value), value).toBe(false)
    }
  })

  it('rejects leading zeros, which Rust rejects too', () => {
    // The backend parses with IpAddr; "01.1.1.1" would fail there, and a UI
    // that accepted it would just hand the user a server error.
    expect(isValidIp('01.1.1.1')).toBe(false)
    expect(isValidIp('1.1.1.010')).toBe(false)
  })

  it('ignores surrounding whitespace', () => {
    expect(isValidIp('  1.1.1.1  ')).toBe(true)
  })
})
