// Display only. The API stores an upstream host bare — an IP or a hostname,
// never bracketed — and forms keep sending it that way. Shown next to a port,
// though, an IPv6 literal needs its brackets back: "2a01:4f8::1:8080" does not
// say where the address ends.
//
// A colon is enough to recognise one, since nothing else the API accepts as a
// host contains it. The bracket check only matters for the host editor, which
// previews a field that is still being typed.
export function formatAuthority(host: string, port: number | string): string {
  const ipv6 = host.includes(':') && !host.startsWith('[')
  return ipv6 ? `[${host}]:${port}` : `${host}:${port}`
}
