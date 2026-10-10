# ADR 0025: Bounded XML Property List contracts

- Status: Accepted
- Date: 2026-10-10
- Scope: M4 AirPlay/RAOP metadata foundation

## Decision

Add a dependency-free XML Property List subset to `frameark-airplay`. It
supports bounded strings, signed integers, booleans, base64 data, arrays, and
deterministic `BTreeMap` dictionaries. Parsing requires a single `<plist>` root,
rejects duplicate keys, unknown values, malformed entities/base64, excessive
depth, and oversized fields. Encoding emits a fixed UTF-8 XML declaration and
stable dictionary order.

The implementation is a metadata contract only. It does not implement binary
Property Lists, FairPlay keys, pairing cryptography, or trust persistence.

## Evidence and limits

Round-trip tests cover XML escaping, base64 data, arrays/dictionaries, duplicate
keys, unknown entities, and deep nesting. The parser and encoder enforce the
document, field, depth, and entry limits before exposing data to protocol code.
