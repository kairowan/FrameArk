# ADR 0135: Rust-owned sidecar subtitles in the Android DLNA player

- Status: Accepted, Experimental
- Date: 2026-10-17
- Scope: M3 subtitle fetching, timing, selection and real Android text output

## Decision and ownership

Complete the subtitle path through the existing acknowledged DLNA player. Rust
reads bounded DIDL-Lite declarations, authorizes/fetches sidecars, parses their
timelines and selects text using the OS player's millisecond media position.
Android does not receive subtitle URLs, parse caption files, maintain a second
media clock or fetch fonts/images. Its existing player Looper reports position;
a main-thread TextView renders plain text with white glyphs and a dark shadow.

The generic parser lives in `frameark-media::subtitles`. DIDL selection remains
in `frameark-dlna`; the host's opaque media-source lifetime owns the parsed tracks
and selected index. This adds three internal JNI methods, not a public C ABI or
FANP wire-format change. No dependency or third-party code is added.

## Interoperability profile

- UTF-8 SRT (`text/srt`, `application/x-subrip`) and WebVTT (`text/vtt`), with BOM,
  CR/LF variants, multiline Unicode, cue IDs, overlapping cues and millisecond
  intervals. WebVTT NOTE blocks are skipped. Common inline annotations are
  flattened; entities become literal text. Unknown markup is displayed literally,
  never executed. Cue order must be nondecreasing and intervals are [start, end).
- One DIDL item with a primary `res` whose URI equals `CurrentURI`. Up to four
  direct `res` subtitle declarations are accepted as an explicit sidecar
  convention. The ContentDirectory:4 `upnp:resExt` / `componentInfo` / Caption
  component path is also accepted, bound by its ID to the selected primary res.
  Namespace prefixes are arbitrary; namespace URIs are not. No guessed `.srt`
  URL, HTML scrape, external entity, unknown extension or arbitrary XML URL fetch.
- All subtitle sources obey the same peer-pinned private-IPv4 HTTP Range policy
  as media. No ambient DNS, alternate host, redirect, compressed response or
  Android HTTPS fallback. A valid declaration is not network authorization.
- The first supplied track is selected after load. The Subtitles menu can select
  another track or Off; no language/URL is persisted. Pause retains the cue at the
  player's position, Seek works in either direction, Stop/EOS clear the overlay,
  and replay uses the restarted player position. Selection is scoped to the exact
  source ID; a stale menu cannot change its replacement.

This is a plain-text sidecar profile, not complete WebVTT layout, embedded tracks,
ASS/SSA, bitmap subtitles, karaoke, HLS timestamp maps or Samsung-specific caption
extensions. STYLE, REGION, cue-layout settings and timestamp-map headers fail
explicitly. Text is bottom-centered and limited to twelve visible lines by the
platform view; no styled caption layout or accessibility certification is claimed.
Still images with declared sidecars are rejected. Named-controller compatibility
and physical subtitle synchronization remain unverified.

## Safety, deadlines and lifecycle

- DIDL remains <=4096 bytes, <=128 DOM nodes and <=8 lexical levels checked before
  recursive parsing; DTD/entities are disabled. At most four distinct sources;
  language labels are <=35 ASCII language-tag characters.
- At most 1 MiB total encoded subtitle bytes across all tracks, checked against
  declared lengths before allocating/downloading their bodies. Each track has
  <=10,000 cues, <=4096 encoded cue bytes, <=8 simultaneous cues, <=8192 combined
  rendered UTF-8 bytes and a seven-day timeline limit. Input UTF-8 and controls,
  timestamp arithmetic and overlap budgets are checked before use.
- All sidecars share a three-second monotonic download/parse budget, with each
  network operation also capped at two seconds. `RemoteMediaSource::open_until`
  caps its initial probe, later connections and cache reads to the same deadline;
  slow bytes and phase changes cannot renew it. OS scheduling is not hard real
  time. A probe still has its bounded connect/I/O window; an opened loading source
  is registered with the output bridge so shutdown revokes in-flight reads.
- Fetch/parse failures before OS dispatch leave the previous output intact.
  After dispatch, the existing acknowledgment/fatal-cleanup rules apply. Caption
  text and track choice disappear with the media handle. Revision/owner checks
  reject stale UI posts; old output retirement cannot clear a newer owner.
- Caption lookup is polled at 100 ms from the real player's position, not the
  coarser whole-second UPnP progress field. This is an update cadence, not a
  physical synchronization or latency measurement. Diagnostics contain counts
  and selected indices only, never caption text, metadata URLs or query tokens.

## Migration, verification and rollback

Nonempty Android `CurrentURIMetaData` must now be well-formed, bounded DIDL-Lite;
empty metadata remains valid. Unsupported/unsafe declared sidecars reject the
new load, rather than silently showing missing or unrelated captions. Keep app
and JNI from the same build. Other callers of the DLNA protocol crate retain
their own backend policy; ordinary long-lived `RemoteMediaSource::open` behavior
is unchanged.

The [test guide](../testing/ANDROID_DLNA_PLAYBACK.md) covers the new input profile
and owner-run checks. Verification must include Rust malformed/resource-boundary
cases and real H.264 playback with committed-window pixels, pause/seek, menu selection,
Off, rejection, replacement, EOS and shutdown on an isolated emulator. It does
not replace the owner's physical-device/controller/two-hour acceptance matrix.
The API 29+ render fixture waits for a hardware frame-commit callback, then uses
PixelCopy on the real window; reading TextView text alone or capturing an older
compositor frame does not prove that the selected cue has been rendered.
Final runs, corrected fixture races and unedited window capture are recorded in
the [evidence log](../evidence/2026-10-17-dlna-subtitles/README.md).
Rollback is a reviewed revert; never enable an insecure URL fallback to make a
controller pass. No production key, signing environment secret, tag or release
is created by this increment.

## Primary format references

- [UPnP ContentDirectory:4, section 2.6.12](https://upnp.org/specs/av/UPnP-av-ContentDirectory-v4-Service-20130331.pdf)
  defines resource-associated Caption components and external caption resources.
- [W3C WebVTT](https://www.w3.org/TR/webvtt1/) defines timed cue structure. This
  implementation deliberately exposes the plain-text subset above.
- [Library of Congress SRT format description](https://www.loc.gov/preservation/digital/formats/fdd/fdd000569.shtml)
  describes SubRip blocks and timing conventions. Fixtures here are original.
- [Android frame-commit callback](https://developer.android.com/reference/android/view/ViewTreeObserver#registerFrameCommitCallback(java.lang.Runnable))
  documents submitted-frame capture with PixelCopy; this is test evidence, not a
  hardware display-latency measurement.
