use picora::{
    model::{Pack, PackRef, Provider, Sticker},
    store::Store,
};

fn pack() -> Pack {
    Pack {
        reference: PackRef {
            provider: Provider::Dcinside,
            id: "42".into(),
        },
        title: "한글 pack".into(),
        author: "artist".into(),
        url: "https://dccon.dcinside.com/#42".into(),
        thumbnail: String::new(),
        stickers: vec![
            Sticker {
                id: "a".into(),
                label: "웃음".into(),
                url: "https://dcimg5.dcinside.com/dccon.php?no=a".into(),
            },
            Sticker {
                id: "b".into(),
                label: "hello".into(),
                url: "https://dcimg5.dcinside.com/dccon.php?no=b".into(),
            },
        ],
    }
}
#[test]
fn validates_pack_urls_and_namespaces_ids() {
    assert_eq!(
        PackRef::parse("https://dccon.dcinside.com/#42")
            .unwrap()
            .key(),
        "dcinside:42"
    );
    assert_eq!(
        PackRef::parse("https://arca.live/e/42").unwrap().key(),
        "arca:42"
    );
    for url in [
        "https://arca.live.evil.test/e/42",
        "http://localhost/e/42",
        "https://arca.live/e/../42",
        "https://dccon.dcinside.com/#oops",
        "https://user:pass@arca.live/e/42",
    ] {
        assert!(PackRef::parse(url).is_err(), "accepted {url}");
    }
}
#[test]
fn imports_are_idempotent_and_groups_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let store = Store::open(&path).unwrap();
    let p = pack();
    store.save_pack(&p).unwrap();
    store.save_pack(&p).unwrap();
    store.favorite(&p.reference.key(), true).unwrap();
    let group = store.create_group("Reactions").unwrap();
    for id in ["a", "b", "a"] {
        store.add_to_group(group, &p.reference.key(), id).unwrap();
    }
    store
        .move_in_group(group, &p.reference.key(), "b", -1)
        .unwrap();
    store.record_recent(&p.reference.key(), "a").unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.packs().unwrap().len(), 1);
    assert!(store.packs().unwrap()[0].favorite);
    let items = store.group_items(group).unwrap();
    assert_eq!(
        items
            .iter()
            .map(|x| x.sticker.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(store.recents().unwrap()[0].sticker.id, "a");
    store
        .remove_from_group(group, &p.reference.key(), "b")
        .unwrap();
    assert_eq!(store.group_items(group).unwrap().len(), 1);
}
#[test]
fn refreshing_pack_preserves_group_membership_and_favorites() {
    let store = Store::open_in_memory().unwrap();
    let mut p = pack();
    store.save_pack(&p).unwrap();
    let group = store.create_group("test").unwrap();
    store.add_to_group(group, &p.reference.key(), "a").unwrap();
    p.title = "renamed".into();
    store.save_pack(&p).unwrap();
    assert_eq!(store.group_items(group).unwrap()[0].pack_title, "renamed");
    assert!(store.create_group("   ").is_err());
    assert!(
        store
            .add_to_group(group, &p.reference.key(), "missing")
            .is_err()
    );
}
