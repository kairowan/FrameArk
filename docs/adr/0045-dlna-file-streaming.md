# ADR 0045: explicit DLNA file streaming backend

- Status: Accepted
- Scope: M3 DLNA/UPnP MediaRenderer media HTTP boundary
- Date: 2026-10-11

## Context

The initial DLNA renderer used bounded in-memory fixtures. That is useful for
parser tests but cannot represent normal media files and encourages an unsafe
shortcut where an HTTP URL is fetched implicitly by the receiver.

## Decision

Add `FileMediaResource`, which the host explicitly constructs from a regular
file, canonicalizes at registration, validates its media metadata, and records
its length under a bounded file-size limit. The synchronous TCP adapter handles
GET requests for these registered paths directly: it validates one optional
Range, writes response headers, seeks to the requested offset, and copies at
most `MAX_MEDIA_STREAM_CHUNK_BYTES` at a time. It verifies that the file length
has not changed before streaming.

The generic `MediaRendererHttpService::handle` path and in-memory
`MediaResource` remain unchanged. No URL fetching, directory traversal,
implicit path selection, chunked transfer, TLS, authorization, or media format
probing is added.

## Consequences

- Large explicitly registered files no longer need to fit the in-memory HTTP
  response bound.
- Range playback has a real bounded streaming path that can be integrated by a
  daemon without changing the SOAP state model.
- Hosts still own authorization, file lifetime, TLS, format/codec policy, and
  cleanup; the compatibility level remains Experimental.
