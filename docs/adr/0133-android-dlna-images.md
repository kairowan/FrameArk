# ADR 0133: Bounded Android DLNA still-image output

- Status: Accepted, Experimental
- Date: 2026-10-15

## Outcome and ownership

Add real static JPEG/PNG display to the existing opt-in peer-origin HTTP renderer.
Rust advertises these two MIME types, probes/pins the source, enforces the encoded
length limit, owns the shared Session and emits typed prepare command 8 with an
opaque byte handle and JPEG/PNG argument 1/2. This is an internal matching-build
JNI command extension, not a new public network protocol. No dependency is added.
Kotlin has no URL-fetch or protocol-state path: it performs platform decode and UI
display only. Existing video/audio command 1 and all other commands remain intact.

Images have zero position/duration, no automatic EOS and no positive-time seek.
Play shows the prepared bitmap, Pause holds it, Stop clears to black but retains
it for replay. Loading while stopped/paused prepares black; loading while playing
is followed by the existing Rust-owned Play. Replacing an image with video removes
the overlay only after MediaPlayer preparation. TextureView remains attached below
the overlay so its Surface survives image/video switches. Volume remains a shared
setting and applies to the next audio/video source; it does not alter the image.

## Bounds and lifetime

- Encoded length 1..16 MiB, rejected by Rust before command dispatch and checked
  again by Kotlin before allocation. Read through <=64 KiB revocable source calls.
- Verify JPEG/PNG signatures, platform-reported MIME and positive dimensions;
  original edges <=32768 and original area <=100 million pixels. Power-of-two
  sampling yields <=4096 per edge and <=4,194,304 pixels, ARGB8888 sRGB, <=16 MiB
  per output bitmap. Actual allocation/dimensions are checked after decoding.
- JPEG uses platform ExifInterface and all eight orientation transforms. PNG EXIF,
  animation playback, HDR/wide-gamut fidelity and slideshow timing are not provided.
  The platform's static PNG decode may show only the default frame of animated PNG.
- One worker owns source/decode and OS player; only the main thread mutates ImageView.
  Generation, runtime token, attachment and closed-state checks reject stale work.
  Output acknowledgment follows the main-thread mutation, not merely queued intent;
  screenshot tests separately verify eventual visible pixels.
- Replacement/close clears app bitmap references. Do not recycle a published bitmap
  while RenderThread may retain a display list; Android controls final reclamation.
  Never-published transform temporaries can be recycled immediately. Transformation
  may transiently hold two bitmaps plus the encoded buffer; this is not a bound on
  process-wide memory. OOM becomes a fixed, redacted output error.
- Preserve the five-second command deadline and source cancellation. Cancellation
  is checked between reads and before/after decode. Android decode cannot be forcibly
  interrupted mid-call; deadline expiry invalidates the Rust output/source, and late
  acknowledgment is rejected. Corrupt input fails closed and requires re-enabling
  the output backend. No location metadata, private URL or image content is logged.
- Finishing Activities release only their own image/surface ownership; old teardown
  cannot clear a newer page's surface or stop its receiver.

## Verification and limits

Rust tests cover typed preparation/length rejection, actual source probe and shared
Session controls, timeless feedback and published MIME capabilities over TCP.
Pure Kotlin tests cover limits, overflow and signatures. Isolated Android tests
decode original JPEG/PNG, verify eight orientations and downsampling, actual screen
pixels, Pause/Stop/replay/seek, repeated replacement, video round trip, malformed
input, source cleanup and stale/closed display ownership. See the
[verification record](../evidence/2026-10-15-dlna-images/README.md);
emulator evidence is not a named-client/hardware matrix.

M3 stays Partial: subtitles/playlists, broader authorized origins, Android HTTPS,
MP3/codec coverage, SSDP interoperability, third-party Android/iOS/desktop clients
and the two-hour playback/Seek gate remain. No certification or M0–M5 completion,
production key, release, new origin permission or migration is implied. Rollback
is a reviewed revert of this increment with a matching APK/native build.

## Primary sources

- [BitmapFactory.Options sampling and bounds](https://developer.android.com/reference/android/graphics/BitmapFactory.Options)
- [Android platform ExifInterface](https://developer.android.com/reference/android/media/ExifInterface)
- [Android bitmap memory ownership](https://developer.android.com/topic/performance/graphics/manage-memory)
