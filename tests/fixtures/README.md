# Provider fixtures

`dc-catalog.html` and `dc-detail.json` were captured from the public DCInside catalog and `/index/package_detail` response for [pack 41982](https://dccon.dcinside.com/#41982) on 2026-10-14. The catalog is trimmed to one pack. No session cookies or login data are included. Tests do not download sticker artwork by default.

Arca snippets in `tests/providers.rs` are synthetic parser fixtures. They do not represent a verified authenticated response; live verification requires signing into Arca through the app.

The Arca detail regression snippets use the pack header selector
`article .emoticon-header .emoticon-info .title` and body selector
`article .article-body.emoticon-body` documented by the
[ArcaLive downloader source](https://greasyfork.org/en/scripts/575390/code)
(inspected 2026-10-15). They are structural reproductions, not captured
responses from pack 54950. Live verification remains required: this environment
receives a Cloudflare challenge. Catalog headings and thumbnails are deliberately
included outside the pack body to catch accidental catalog-as-pack parsing.
