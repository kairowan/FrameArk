# frameark-diagnostics

`frameark-diagnostics` is the bounded, platform-neutral diagnostics sink for
FrameArk. It records redaction-safe lifecycle and error summaries so an
Android, desktop, or CLI adapter can explain which session stage failed
without retaining packets, URLs, credentials, device identifiers, or media.

The recorder is an in-memory ring. Callers choose a capacity up to the bounded
maximum, and exporting text is deterministic for support bundles and tests.
It does not write files, ship telemetry, or collect sensitive values. A caller
must provide an already safe `DiagnosticEvent`/`FrameArkError` message.
