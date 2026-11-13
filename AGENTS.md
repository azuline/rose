# Testing conventions

- Do not use pytest `monkeypatch` or `mock.patch` to stub internals.
- Stub external interactions, such as the interactive editor, through explicit dependency injection parameters.
- Never stub internal functions such as `update_cache_for_releases` in tests.
