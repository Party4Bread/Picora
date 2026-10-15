use picora::{
    media::decode,
    model::{PackRef, Provider},
    providers::{Repository, StickerProvider},
};
#[tokio::test]
#[ignore = "contacts DCInside; run explicitly"]
async fn live_dcinside_search_pack_and_image() {
    let repo = Repository::new(Provider::Dcinside).unwrap();
    let packs = repo.browse("고양이", 1).await.unwrap();
    assert!(!packs.is_empty());
    let reference = PackRef::parse("https://dccon.dcinside.com/#41982").unwrap();
    let pack = repo.pack(&reference).await.unwrap();
    assert_eq!(pack.reference, reference);
    let bytes = repo.bytes(&pack.stickers[0].url).await.unwrap();
    assert!(!decode(&bytes).unwrap().frames.is_empty());
}
