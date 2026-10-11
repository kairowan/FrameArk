# ADR 0066: Bounded AirPlay HLS playlist contract

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay URL/HLS media boundary

## Decision

Add `HlsMediaPlaylist` as a deterministic parser for the media-playlist
subset needed by a future AirPlay URL receiver. The parser requires
`#EXTM3U`, a bounded integer target duration, optional media sequence and
end-list tags, and duration/URI pairs for no more than 512 segments. Playlist
input is capped at 64 KiB and each URI at 2 KiB.

Relative segment paths and absolute HTTP(S) URIs are accepted. Credentials,
scheme-relative paths, filesystem-rooted paths, backslash traversal, control
characters, and `..` path components are rejected. The crate does not fetch,
cache, decrypt, decode, or claim HLS/Apple device interoperability; those
remain caller-owned layers.

## Verification

AirPlay tests cover versioned/end-listed playlists, fractional durations,
media sequence preservation, query-bearing segment URIs, missing target
duration, credential-bearing URLs, and path traversal rejection.
