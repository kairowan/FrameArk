# DLNA event fixtures

`rcs-volume-73.xml` is an original synthetic InstanceID 0 / Master volume 73
notification for the implemented RenderingControl profile. It contains no user
media, address, key or device capture. Rust compares its complete generated
body; `scripts/test_dlna_event_fixtures.py` parses both XML layers independently
with Python's standard XML parser. It is not named-controller or decoder evidence.

Wire references and profile limits are in [ADR 0130](../../../../docs/adr/0130-dlna-controller-eventing.md).
