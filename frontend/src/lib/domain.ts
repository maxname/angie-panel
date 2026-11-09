// Lenient client-side check; the server is authoritative. Accepts FQDNs and a
// single leading "*." wildcard label.
const DOMAIN_RE =
  /^(\*\.)?([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$/i

export function isValidDomain(value: string): boolean {
  return DOMAIN_RE.test(value.trim())
}
