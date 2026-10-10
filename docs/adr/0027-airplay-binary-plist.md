# ADR 0027: Bounded binary Property List decoding

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay/RAOP control metadata

## Decision

Add a dependency-free binary Property List decoder to `frameark-airplay` and
map its supported scalar, array, and dictionary objects to the existing
`PlistValue` model. The decoder validates the `bplist00` header, trailer,
object and reference widths, offset table, recursion depth, object count, and
field sizes before constructing values. Object-table cycles, duplicate
dictionary keys, invalid dictionary key types, and offsets outside the object
table are rejected.

The supported subset intentionally excludes null, real, date, and UID objects;
callers must negotiate or reject metadata requiring those types. This is a
control-plane parser only: it does not implement Apple pairing, FairPlay,
encryption, or trust persistence.

## Evidence and limits

Unit tests cover dictionary and UTF-16 strings, signed integers, cycles,
unsupported object types, and malformed offset tables. The implementation
remains Experimental until captured Apple sessions and a device compatibility
matrix exercise the parser in a real RTSP server.
