// Lenient client-side check, like domain.ts: it exists to catch a typo before
// a round trip, not to be the authority. The backend parses every resolver
// token with Rust's IpAddr and rejects the request otherwise, so anything that
// slips through here still cannot reach the generated config.

function isIpv4(value: string): boolean {
  const parts = value.split('.')
  return (
    parts.length === 4 &&
    parts.every(
      (part) =>
        /^\d{1,3}$/.test(part) &&
        // No leading zeros: Rust's parser rejects "01", and a silently
        // accepted 010 would mean something else again to anyone reading it.
        (part === '0' || !part.startsWith('0')) &&
        Number(part) <= 255,
    )
  )
}

function isIpv6(value: string): boolean {
  // At most one "::", hex groups of 1-4 digits, with an optional trailing IPv4
  // (::ffff:1.2.3.4). Deliberately loose about group counts — the server is
  // the judge; this only rejects things that are obviously not addresses.
  if (!value.includes(':')) return false
  if ((value.match(/::/g) ?? []).length > 1) return false
  const [head, tail] = value.split('::', 2)
  const groups = [head, tail ?? '']
    .flatMap((half) => half.split(':'))
    .filter((group) => group !== '')
  if (groups.length === 0) return value === '::'
  const last = groups[groups.length - 1]
  const rest = last.includes('.') ? groups.slice(0, -1) : groups
  if (last.includes('.') && !isIpv4(last)) return false
  return rest.every((group) => /^[0-9a-f]{1,4}$/i.test(group))
}

export function isValidIp(value: string): boolean {
  const trimmed = value.trim()
  return isIpv4(trimmed) || isIpv6(trimmed)
}
