# Provider fixtures

`dc-catalog.html` and `dc-detail.json` were captured from the public DCInside catalog and `/index/package_detail` response for [pack 41982](https://dccon.dcinside.com/#41982) on 2026-10-14. The catalog is trimmed to one pack. No session cookies or login data are included. Tests do not download sticker artwork by default.

Arca snippets in `tests/providers.rs` are synthetic parser fixtures. They do not represent a verified authenticated response; live verification requires signing into Arca through the app.
