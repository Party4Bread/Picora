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
    let pack = arca::parse_detail("123", r#"<h1>테스트</h1><div class="emoticon-view"><img src="//ac-p.namu.la/2026/a.gif" data-id="1"><img src="//ac-p.namu.la/2026/b.png" data-id="2"></div>"#).unwrap();
    assert_eq!(pack.stickers.len(), 2);
    assert!(pack.stickers[0].url.starts_with("https://"));
}
