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

`animation.mp4` is generated test artwork (32×32 test pattern, 12 H.264 Main
frames with B-frame reordering), not downloaded sticker artwork:

```sh
ffmpeg -f lavfi -i 'testsrc2=size=32x32:rate=12:duration=1' -c:v libx264 -profile:v main -bf 2 -pix_fmt yuv420p -movflags +faststart -an animation.mp4
```

The video parser snippet reproduces the `video.emoticon` / `data-src` /
`poster` structure observed in authenticated pack 54950 on 2026-10-15.
Live artwork and session credentials are not committed. The optional live test
reads a private JSON array of Set-Cookie strings from `PICORA_ARCA_COOKIES_FILE`.

`animation-reference.png` is an independent FFmpeg decode, tiled horizontally
with `ffmpeg -i animation.mp4 -vf tile=12x1 -frames:v 1 animation-reference.png`.
It verifies display order (including B frames) and decoded colors.
