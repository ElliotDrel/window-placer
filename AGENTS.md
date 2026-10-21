# Window Placer

- Keep the core event driven, with one application event thread and no polling or async runtime.
- Load rules once from windows.conf; keep personal paths and credentials out of public files.
- Move only test-owned windows during automated tests.
- Run formatting, serial Windows tests, warnings-as-errors lint and the release build before publishing.
- Keep legacy/autohotkey archived; maintain the Rust implementation.
- Preserve installed user configuration and keep startup activation per user.
