use picora::providers::{arca, dcinside};
#[test]
fn dcinside_real_responses_parse() {
    let page = dcinside::parse_catalog(include_str!("fixtures/dc-catalog.html")).unwrap();
    assert_eq!(page[0].reference.id, "41982");
    assert_eq!(page[0].title, "슬픈 고양이콘");
    let pack = dcinside::parse_detail(include_str!("fixtures/dc-detail.json")).unwrap();
    assert_eq!(pack.stickers.len(), 33);
    assert!(
        pack.stickers[0]
            .url
            .starts_with("https://dcimg5.dcinside.com/dccon.php?no=")
    );
    assert!(dcinside::parse_detail("error").is_err());
    assert!(dcinside::parse_catalog("<html>Service unavailable</html>").is_err());
}
#[test]
fn arca_login_and_unrecognized_pages_are_not_empty_catalogs() {
    assert!(arca::parse_catalog("<form action='/u/login'>로그인</form>").is_err());
    assert!(arca::parse_catalog("<html>Checking your browser</html>").is_err());
}
#[test]
fn arca_extracts_packs_and_assets_from_documented_shape() {
    // Synthetic fixture: live authenticated Arca verification is separate.
    let page = arca::parse_catalog(r#"<div class="emoticon-list"><a href="/e/123"><img src="//ac-p.namu.la/thumb.png"><span class="title">테스트</span></a></div>"#).unwrap();
    assert_eq!(page[0].reference.id, "123");
    let pack = arca::parse_detail("123", r#"<article><div class="emoticon-header"><div class="emoticon-info"><div class="title">테스트</div></div></div><div class="article-body emoticon-body"><img class="emoticon" src="//ac-p.namu.la/2026/a.gif" data-id="1"><img class="emoticon" src="//ac-p.namu.la/2026/b.png" data-id="2"></div></article>"#).unwrap();
    assert_eq!(pack.stickers.len(), 2);
    assert!(pack.stickers[0].url.starts_with("https://"));
}

#[test]
fn arca_detail_uses_selected_pack_not_catalog_heading_or_thumbnails() {
    // Reduced structural fixture based on Arca's pack-specific header/body selectors.
    // Deliberately includes the store heading and unrelated catalog media first.
    let html = r#"
      <h1>전체 아카콘</h1>
      <div class="emoticon-list"><a href="/e/999"><img src="//ac-p.namu.la/catalog.png"></a></div>
      <article>
        <div class="emoticon-header"><div class="emoticon-info"><span class="title">선택한 팩</span></div></div>
        <div class="article-body emoticon-body"><div class="emoticons-wrapper">
          <img class="emoticon" data-id="11" src="//ac-p.namu.la/selected.png">
        </div></div>
      </article>
      <img class="emoticon" src="//ac-p.namu.la/unrelated.png">
    "#;
    let pack = arca::parse_detail("54950", html).unwrap();
    assert_eq!(pack.reference.id, "54950");
    assert_eq!(pack.title, "선택한 팩");
    assert_eq!(pack.stickers.len(), 1);
    assert_eq!(pack.stickers[0].url, "https://ac-p.namu.la/selected.png");
}

#[test]
fn arca_detail_rejects_catalog_returned_instead_of_pack() {
    let html = r#"<h1>전체 아카콘</h1><div class="emoticon-list">
        <a href="/e/999"><img src="//ac-p.namu.la/catalog.png"></a>
        </div>"#;
    assert!(arca::parse_detail("54950", html).is_err());
}

#[test]
fn arca_video_stickers_are_taken_from_pack_body() {
    let html = r#"<article><div class="emoticon-header"><div class="emoticon-info"><div class="title">실비아 정실력측정</div></div></div>
        <div class="article-body emoticon-body"><div class="emoticons-wrapper">
        <video class="emoticon" data-id="308769222" data-src="//ac.arca.live/clip.mp4?expires=123&amp;key=test" poster="//ac.arca.live/clip.thumbnail.webp"></video>
        <video class="emoticon" data-id="2"><source src="//ac.arca.live/second.mp4"></video>
        <img class="emoticon" data-id="3" src="//ac.arca.live/still.png">
        </div><img src="/static/assets/images/tag.svg"></div></article>"#;
    let pack = arca::parse_detail("54950", html).unwrap();
    assert_eq!(pack.stickers.len(), 3);
    assert_eq!(pack.stickers[0].id, "308769222");
    assert_eq!(
        pack.stickers[0].url,
        "https://ac.arca.live/clip.mp4?expires=123&key=test"
    );
    assert_eq!(pack.stickers[1].url, "https://ac.arca.live/second.mp4");
}

#[test]
fn arca_catalog_video_thumbnails_are_readable() {
    let packs = arca::parse_catalog(r#"<div class="emoticon-list"><a href="/e/54950?p=1"><video data-src="//ac.arca.live/thumbnail.mp4"></video><div class="title">실비아 정실력측정</div></a></div>"#).unwrap();
    assert_eq!(packs[0].thumbnail, "https://ac.arca.live/thumbnail.mp4");
}

#[test]
fn arca_catalog_uses_sticker_media_instead_of_rating_badges() {
    for media in [
        r#"<img loading="lazy" src="//ac.arca.live/sticker.png">"#,
        r#"<video data-src="//ac.arca.live/sticker.mp4"></video>"#,
    ] {
        let html = format!(
            r#"<div class="emoticon-list"><a href="/e/54566?p=1"><div class="emoticon"><span class="stars"><img alt="Star" src="/static/assets/images/star-yellow.svg?t=1"></span>{media}<div class="title">Pack</div></div></a></div>"#
        );
        let packs = arca::parse_catalog(&html).unwrap();
        assert!(
            packs[0]
                .thumbnail
                .starts_with("https://ac.arca.live/sticker."),
            "{}",
            packs[0].thumbnail
        );
    }
}

#[test]
fn arca_search_excludes_ranked_packs_without_losing_matching_duplicates() {
    let html = r#"
    <a href="/e/999"><img src="//ac.arca.live/navigation.png"></a>
    <div class="emoticon-list"><div class="row rank-list">
      <a href="/e/1"><img src="//ac.arca.live/popular.png"><span class="title">Popular</span></a>
      <a href="/e/2"><img src="//ac.arca.live/match.png"><span class="title">Match</span></a>
    </div></div>
    <div class="emoticon-list"><div class="row">
      <a href="/e/3"><img src="//ac.arca.live/first.png"><span class="title">First match</span></a>
      <a href="/e/2"><img src="//ac.arca.live/match.png"><span class="title">Match</span></a>
    </div></div>"#;
    let packs = arca::parse_catalog(html).unwrap();
    assert_eq!(
        packs
            .iter()
            .map(|p| p.reference.id.as_str())
            .collect::<Vec<_>>(),
        vec!["3", "2"]
    );
}

#[test]
fn arca_empty_search_does_not_return_popular_packs() {
    let ranking = r#"<div class="emoticon-list"><div class="row rank-list"><a href="/e/1"><img src="//ac.arca.live/popular.png"></a></div></div>"#;
    let html = format!("{ranking}<div class='emoticon-list'><div class='row'></div></div>");
    assert!(arca::parse_catalog(&html).unwrap().is_empty());
    assert!(arca::parse_catalog(ranking).is_err());
}
