# ADR 0071: DLNA media URI boundary validation

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA MediaRenderer control plane

## Decision

Validate the complete authority of every `SetAVTransportURI` value before
changing renderer state. Only `http` and `https` are accepted. The bounded
validator rejects control or whitespace characters, fragments, empty hosts,
credentials, backslashes, invalid DNS labels, unbracketed IPv6 literals, and
zero or malformed ports. Bracketed IPv6 literals are accepted only when the
address parses and an optional non-zero `u16` port is valid.

This is syntax and injection protection, not a DNS resolver or a claim that a
remote URL is safe to fetch. The renderer still never fetches a URL itself;
future caller-owned fetchers must apply an explicit DNS-aware private-network,
redirect, size, MIME, and DRM policy.

## Verification

- Existing URI, SOAP duplicate-argument, and file-scheme regressions remain
  covered.
- New regression coverage rejects user-info credentials, malformed/zero ports,
  invalid IPv6 authority syntax, fragments, and missing hosts without mutating
  `current_uri`.
- `frameark-dlna` unit tests and clippy pass with warnings denied.
