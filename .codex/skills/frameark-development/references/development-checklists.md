# FrameArk development checklists

Read only the checklist matching the current work. These are completion gates, not a requirement to add irrelevant files.

## New feature or module

- Define the user-visible behavior, ownership boundary, cancellation, timeout, and failure states.
- Reuse shared device, capability, session, track, event, and error types.
- Add unit tests and at least one meaningful integration path.
- Add metrics or redacted diagnostics sufficient to locate setup and runtime failures.
- Update configuration, API, user documentation, compatibility notes, and migration guidance where affected.
- Verify resource release after success, rejection, cancellation, disconnect, and error.
- Commit the completed logical unit using `feat(<scope>): <summary>`.

## New protocol or protocol increment

- State the specification/evidence source, legal status, supported subset, and maturity label.
- Define discovery records, transport, state mapping, authentication, capability negotiation, timeouts, and teardown.
- Bound every untrusted field, collection, nested structure, media packet, queue, and retry path.
- Add valid, boundary, malformed, replay, downgrade, and resource-exhaustion fixtures.
- Add parser property/fuzz coverage before calling a network-facing parser Stable.
- Add packet replay and redacted diagnostics without storing private media by default.
- Test reconnect, out-of-order data, loss, duplicate messages, version skew, and unsupported capabilities.
- Update the protocol specification, compatibility matrix, security notes, and user-facing limitations.
- Commit each independently interoperable protocol increment using `feat(protocol-<name>): <summary>`.

## New template or scaffold

- Keep placeholders explicit and fail validation if required values remain unresolved.
- Avoid embedded secrets, personal paths, mutable latest-version assumptions, or copied identifiers.
- Generate at least one sample and validate the generated output, not just the template text.
- Document required inputs, generated paths, ownership, and upgrade expectations.
- Commit the finished template using `feat(template-<name>): <summary>` or `chore(template-<name>): <summary>`.

## Bug fix

- Reproduce the fault with a focused test or fixture before changing behavior when practical.
- Fix the shared layer instead of masking the failure in one UI where ownership belongs elsewhere.
- Test adjacent lifecycle paths and preserve the reproducer as a regression case.
- Update compatibility or workaround data if the fault is device-specific.
- Commit using `fix(<scope>): <summary>`.

## Security-sensitive change

- Identify assets, trust boundary, attacker capability, abuse limits, and secrets involved.
- Reject insecure fallback and downgrade behavior unless explicitly documented and isolated.
- Verify log redaction, key storage, authorization revocation, replay protection, and cleanup.
- Add negative tests and update `SECURITY.md` or the threat model.
- Do not publish exploit detail or push a security branch without following the private reporting process.
- Commit non-embargoed work using `security(<scope>): <summary>` or an appropriate `fix` type.

## FFI or media hot-path change

- Define ownership, lifetime, thread affinity, callback thread, and release order.
- Avoid repeated copies across FFI; measure rather than assume improvement.
- Test repeated connect/disconnect, reconfiguration, background/foreground, decoder failure, and shutdown.
- Record latency, allocation, memory, CPU, frame-drop, or A/V sync impact where relevant.

## Documentation-only change

- Keep product naming, protocol maturity, links, commands, and branch policy consistent.
- Run repository policy and local-link checks.
- Commit using `docs(<scope>): <summary>`.

