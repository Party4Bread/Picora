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
fn favorite_is_durable_before_any_media_download() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let p = pack();
    {
        let store = Store::open(&path).unwrap();
        store.save_favorite_pack(&p).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert!(store.packs().unwrap()[0].favorite);
    assert_eq!(store.packs().unwrap()[0].pack.stickers.len(), 2);
    store.set_pack_favorite(&p.reference.key(), false).unwrap();
    store.save_pack(&p).unwrap();
    assert!(!store.packs().unwrap()[0].favorite);
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
    store.set_pack_favorite(&p.reference.key(), true).unwrap();
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

#[test]
fn recent_lists_used_first_then_newly_added_and_preserves_import_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let store = Store::open(&path).unwrap();
    let old = pack();
    store.save_pack(&old).unwrap();
    let mut new = pack();
    new.reference.id = "99".into();
    store.save_pack(&new).unwrap();
    store.record_recent("dcinside:42", "a").unwrap();
    store.record_recent("dcinside:99", "b").unwrap();
    let keys = |store: &Store| {
        store
            .recents()
            .unwrap()
            .iter()
            .map(|s| format!("{}:{}", s.pack_key, s.sticker.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&store),
        [
            "dcinside:99:b",
            "dcinside:42:a",
            "dcinside:99:a",
            "dcinside:42:b"
        ]
    );
    store.save_pack(&old).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        keys(&store),
        [
            "dcinside:99:b",
            "dcinside:42:a",
            "dcinside:99:a",
            "dcinside:42:b"
        ]
    );
}

#[test]
fn individual_favorites_override_pack_and_survive_refresh() {
    let store = Store::open_in_memory().unwrap();
    let p = pack();
    store.save_favorite_pack(&p).unwrap();
    store
        .set_sticker_favorite("dcinside:42", "a", false)
        .unwrap();
    store.save_favorite_pack(&p).unwrap();
    assert_eq!(
        store
            .favorite_items()
            .unwrap()
            .iter()
            .map(|s| s.sticker.id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    store.set_pack_favorite("dcinside:42", false).unwrap();
    store
        .set_sticker_favorite("dcinside:42", "a", true)
        .unwrap();
    assert_eq!(
        store
            .favorite_items()
            .unwrap()
            .iter()
            .map(|s| s.sticker.id.as_str())
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert!(
        store
            .set_sticker_favorite("dcinside:42", "missing", true)
            .is_err()
    );
}

#[test]
fn removing_individual_sticker_cleans_groups_and_history_without_removing_siblings() {
    let store = Store::open_in_memory().unwrap();
    store.save_favorite_pack(&pack()).unwrap();
    let group = store.create_group("Group").unwrap();
    store.add_to_group(group, "dcinside:42", "a").unwrap();
    store.record_recent("dcinside:42", "a").unwrap();
    store.remove_sticker("dcinside:42", "a").unwrap();
    assert!(store.group_items(group).unwrap().is_empty());
    assert_eq!(store.recents().unwrap().len(), 1);
    assert_eq!(store.packs().unwrap()[0].pack.stickers[0].id, "b");
    store.remove_sticker("dcinside:42", "b").unwrap();
    assert!(store.packs().unwrap().is_empty());
}

#[test]
fn legacy_library_migrates_without_losing_stickers_or_favorites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE packs(key TEXT PRIMARY KEY,data TEXT NOT NULL,favorite INTEGER NOT NULL DEFAULT 0); CREATE TABLE stickers(pack TEXT NOT NULL REFERENCES packs(key) ON DELETE CASCADE,id TEXT NOT NULL,PRIMARY KEY(pack,id)); PRAGMA user_version=1;").unwrap();
    conn.execute(
        "INSERT INTO packs VALUES('dcinside:42',?1,1)",
        [serde_json::to_string(&pack()).unwrap()],
    )
    .unwrap();
    conn.execute_batch("INSERT INTO stickers VALUES('dcinside:42','a'),('dcinside:42','b');")
        .unwrap();
    drop(conn);
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.recents().unwrap().len(), 2);
        assert_eq!(store.favorite_items().unwrap().len(), 2);
    }
}

#[test]
fn reopening_after_old_binary_does_not_repeat_column_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    {
        let store = Store::open(&path).unwrap();
        store.save_favorite_pack(&pack()).unwrap();
        store
            .set_sticker_favorite("dcinside:42", "a", false)
            .unwrap();
    }
    // v0.1.0/early v0.1.1 reset this marker whenever they open a database.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    drop(conn);
    let store = Store::open(&path).expect("existing columns must not be added twice");
    assert_eq!(store.packs().unwrap().len(), 1);
    assert_eq!(store.favorite_items().unwrap().len(), 1);
    assert_eq!(store.recents().unwrap().len(), 2);
}

#[test]
fn updates_require_existing_records_and_removals_are_idempotent() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.set_pack_favorite("missing", true).is_err());
    assert!(
        store
            .set_sticker_favorite("missing", "missing", true)
            .is_err()
    );
    assert!(store.rename_group(42, "name").is_err());
    store.remove_pack("missing").unwrap();
    store.delete_group(42).unwrap();
    store.remove_from_group(42, "missing", "missing").unwrap();
    let group = store.create_group("old").unwrap();
    store.rename_group(group, "new").unwrap();
    assert_eq!(store.groups().unwrap()[0].name, "new");
}
