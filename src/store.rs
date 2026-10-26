use crate::model::{Group, LibrarySticker, Pack, SavedPack, Settings, Sticker};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

/// A sticker's `(pack key, sticker id)`.
type StickerKey = (String, String);

pub struct Store {
    conn: Connection,
}
impl Store {
    /// # Errors
    /// Returns an error if `SQLite` cannot open the file, initialize the schema, or migrate the library.
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }
    /// # Errors
    /// Returns an error if `SQLite` cannot initialize the in-memory database or schema.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS packs(key TEXT PRIMARY KEY, data TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS stickers(pack TEXT NOT NULL REFERENCES packs(key) ON DELETE CASCADE, id TEXT NOT NULL, PRIMARY KEY(pack,id));
        CREATE TABLE IF NOT EXISTS groups(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS group_items(group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE, pack TEXT, sticker TEXT, position INTEGER NOT NULL, PRIMARY KEY(group_id,pack,sticker), FOREIGN KEY(pack,sticker) REFERENCES stickers(pack,id) ON DELETE CASCADE);
        CREATE TABLE IF NOT EXISTS recents(pack TEXT, sticker TEXT, used INTEGER NOT NULL, PRIMARY KEY(pack,sticker), FOREIGN KEY(pack,sticker) REFERENCES stickers(pack,id) ON DELETE CASCADE);
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 3 {
            let tx = conn.unchecked_transaction()?;
            // Older binaries reset user_version to 1 on every open, including
            // after an updater rollback. Inspect the schema before migrating.
            let columns: Vec<String> = {
                let mut q = tx.prepare("PRAGMA table_info(stickers)")?;
                q.query_map([], |r| r.get(1))?
                    .collect::<rusqlite::Result<_>>()?
            };
            if !columns.iter().any(|name| name == "added") {
                tx.execute_batch(
                    "ALTER TABLE stickers ADD COLUMN added INTEGER NOT NULL DEFAULT 0;
                    UPDATE stickers SET added=rowid;",
                )?;
            }
            if !columns.iter().any(|name| name == "favorite") {
                tx.execute_batch("ALTER TABLE stickers ADD COLUMN favorite INTEGER;")?;
            }
            tx.pragma_update(None, "user_version", 3)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }
    /// # Errors
    /// Rejects empty packs and reports serialization or transactional database failures.
    pub fn save_pack(&self, pack: &Pack) -> Result<()> {
        self.save_pack_with_favorite(pack, false)
    }
    /// # Errors
    /// Rejects empty packs and reports serialization or transactional database failures.
    pub fn save_favorite_pack(&self, pack: &Pack) -> Result<()> {
        self.save_pack_with_favorite(pack, true)
    }
    fn save_pack_with_favorite(&self, pack: &Pack, favorite: bool) -> Result<()> {
        ensure!(!pack.stickers.is_empty(), "Pack has no stickers");
        let tx = self.conn.unchecked_transaction()?;
        let key = pack.reference.key();
        tx.execute("INSERT INTO packs(key,data,favorite) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET data=excluded.data,favorite=CASE WHEN ?3 THEN 1 ELSE packs.favorite END", params![key, serde_json::to_string(pack)?, favorite])?;
        let added: i64 =
            tx.query_row("SELECT COALESCE(MAX(added),0)+1 FROM stickers", [], |r| {
                r.get(0)
            })?;
        for s in &pack.stickers {
            tx.execute(
                "INSERT OR IGNORE INTO stickers(pack,id,added) VALUES(?1,?2,?3)",
                params![key, s.id, added],
            )?;
        }
        // Delete removed stickers after insertion, without replacing surviving rows or their memberships.
        let ids: Vec<String> = {
            let mut q = tx.prepare("SELECT id FROM stickers WHERE pack=?1")?;
            q.query_map([&key], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in ids {
            if !pack.stickers.iter().any(|s| s.id == id) {
                tx.execute(
                    "DELETE FROM stickers WHERE pack=?1 AND id=?2",
                    params![key, id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    /// Adds stickers to a pack, creating it from `collection` when absent.
    /// Existing stickers keep their favorite state, group memberships, and recency.
    /// # Errors
    /// Reports deserialization or transactional database failures.
    pub fn add_stickers(&self, collection: Pack, stickers: &[Sticker]) -> Result<()> {
        let key = collection.reference.key();
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM packs WHERE key=?1", [&key], |r| r.get(0))
            .optional()?;
        let mut pack = match data {
            Some(data) => serde_json::from_str(&data)?,
            None => collection,
        };
        let before = pack.stickers.len();
        for sticker in stickers {
            if !pack.stickers.iter().any(|s| s.id == sticker.id) {
                pack.stickers.push(sticker.clone());
            }
        }
        if pack.stickers.len() == before {
            return Ok(());
        }
        if pack.thumbnail.is_empty() {
            pack.thumbnail.clone_from(&pack.stickers[0].url);
        }
        self.save_pack(&pack)
    }
    /// # Errors
    /// Returns an error if the query fails or a persisted pack cannot be deserialized.
    pub fn packs(&self) -> Result<Vec<SavedPack>> {
        let mut q = self
            .conn
            .prepare("SELECT data,favorite FROM packs ORDER BY favorite DESC, key")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?;
        rows.map(|r| {
            let (s, favorite) = r?;
            Ok(SavedPack {
                pack: serde_json::from_str(&s)?,
                favorite,
            })
        })
        .collect()
    }
    /// # Errors
    /// Returns an error if the pack is absent or the database update fails.
    pub fn set_pack_favorite(&self, key: &str, value: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE packs SET favorite=?2 WHERE key=?1",
            params![key, value],
        )?;
        ensure!(changed == 1, "Pack is not in your library");
        Ok(())
    }
    /// # Errors
    /// Returns an error on query failure; an absent sticker returns `None`.
    pub fn is_sticker_favorite(&self, pack: &str, sticker: &str) -> Result<Option<bool>> {
        Ok(self.conn.query_row("SELECT COALESCE(s.favorite,p.favorite) FROM stickers s JOIN packs p ON p.key=s.pack WHERE s.pack=?1 AND s.id=?2", params![pack,sticker], |r|r.get(0)).optional()?)
    }
    /// # Errors
    /// Returns an error if the sticker is absent or the database update fails.
    pub fn set_sticker_favorite(&self, pack: &str, sticker: &str, favorite: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE stickers SET favorite=?3 WHERE pack=?1 AND id=?2",
            params![pack, sticker, favorite],
        )?;
        ensure!(changed == 1, "Sticker is not in your library");
        Ok(())
    }
    /// Favorite stickers as `(pack key, sticker id)`, without loading pack data.
    /// # Errors
    /// Returns an error if the query fails.
    pub fn favorite_keys(&self) -> Result<std::collections::HashSet<StickerKey>> {
        let mut q = self.conn.prepare("SELECT s.pack,s.id FROM stickers s JOIN packs p ON p.key=s.pack WHERE COALESCE(s.favorite,p.favorite)=1")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    /// # Errors
    /// Returns an error if favorite rows or their saved pack data cannot be read.
    pub fn favorite_items(&self) -> Result<Vec<LibrarySticker>> {
        let mut q = self.conn.prepare("SELECT s.pack,s.id FROM stickers s JOIN packs p ON p.key=s.pack WHERE COALESCE(s.favorite,p.favorite)=1 ORDER BY s.pack,s.rowid")?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        self.resolve_rows(rows)
    }
    /// # Errors
    /// Returns an error if the pack or sticker is absent, its data is invalid, or the removal transaction fails.
    pub fn remove_sticker(&self, key: &str, sticker: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let data: String =
            tx.query_row("SELECT data FROM packs WHERE key=?1", [key], |r| r.get(0))?;
        let mut pack: Pack = serde_json::from_str(&data)?;
        let before = pack.stickers.len();
        pack.stickers.retain(|s| s.id != sticker);
        ensure!(
            pack.stickers.len() < before,
            "Sticker is not in your library"
        );
        tx.execute(
            "DELETE FROM stickers WHERE pack=?1 AND id=?2",
            params![key, sticker],
        )?;
        if pack.stickers.is_empty() {
            tx.execute("DELETE FROM packs WHERE key=?1", [key])?;
        } else {
            tx.execute(
                "UPDATE packs SET data=?2 WHERE key=?1",
                params![key, serde_json::to_string(&pack)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Succeeds if the requested record is already absent.
    /// # Errors
    /// Returns an error if the database deletion fails.
    pub fn remove_pack(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM packs WHERE key=?1", [key])?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if the group query, row decoding, or cover pack data fails.
    pub fn groups(&self) -> Result<Vec<Group>> {
        let mut q = self.conn.prepare(
            "SELECT g.id,g.name,c.pack,c.sticker FROM groups g LEFT JOIN (
                SELECT group_id,pack,sticker,ROW_NUMBER() OVER (PARTITION BY group_id ORDER BY position,pack,sticker) AS n
                FROM group_items) c ON c.group_id=g.id AND c.n=1
            ORDER BY g.id",
        )?;
        let rows: Vec<(i64, String, Option<StickerKey>)> = q
            .query_map([], |r| {
                let cover = match (r.get(2)?, r.get(3)?) {
                    (Some(pack), Some(sticker)) => Some((pack, sticker)),
                    _ => None,
                };
                Ok((r.get(0)?, r.get(1)?, cover))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let covers = self.resolve_rows(rows.iter().filter_map(|row| row.2.clone()).collect())?;
        Ok(rows
            .into_iter()
            .map(|(id, name, cover)| Group {
                id,
                name,
                cover: cover.and_then(|(pack, sticker)| {
                    covers
                        .iter()
                        .find(|c| c.pack_key == pack && c.sticker.id == sticker)
                        .cloned()
                }),
            })
            .collect())
    }
    /// # Errors
    /// Rejects blank names and reports database insertion failures.
    pub fn create_group(&self, name: &str) -> Result<i64> {
        ensure!(!name.trim().is_empty(), "Name the group first");
        self.conn
            .execute("INSERT INTO groups(name) VALUES(?1)", [name.trim()])?;
        Ok(self.conn.last_insert_rowid())
    }
    /// # Errors
    /// Rejects blank names or missing groups and reports database update failures.
    pub fn rename_group(&self, id: i64, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Name cannot be empty");
        let changed = self.conn.execute(
            "UPDATE groups SET name=?2 WHERE id=?1",
            params![id, name.trim()],
        )?;
        ensure!(changed == 1, "Group does not exist");
        Ok(())
    }
    /// Succeeds if the requested record is already absent.
    /// # Errors
    /// Returns an error if the database deletion fails.
    pub fn delete_group(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM groups WHERE id=?1", [id])?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if the group or sticker does not exist, or the database insertion fails. Existing membership is accepted.
    pub fn add_to_group(&self, group: i64, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO group_items(group_id,pack,sticker,position) VALUES(?1,?2,?3,COALESCE((SELECT MAX(position)+1 FROM group_items WHERE group_id=?1),0))",params![group,pack,sticker])?;
        Ok(())
    }
    /// Succeeds if the requested record is already absent.
    /// # Errors
    /// Returns an error if the database deletion fails.
    pub fn remove_from_group(&self, group: i64, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM group_items WHERE group_id=?1 AND pack=?2 AND sticker=?3",
            params![group, pack, sticker],
        )?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if group contents cannot be read or the new order cannot be committed. Absent membership is unchanged.
    pub fn move_in_group(&self, group: i64, pack: &str, sticker: &str, delta: i32) -> Result<()> {
        let mut items = self.group_items(group)?;
        if let Some(i) = items
            .iter()
            .position(|s| s.pack_key == pack && s.sticker.id == sticker)
        {
            let j = i
                .saturating_add_signed(isize::try_from(delta)?)
                .min(items.len() - 1);
            items.swap(i, j);
            let tx = self.conn.unchecked_transaction()?;
            for (n, s) in items.iter().enumerate() {
                tx.execute("UPDATE group_items SET position=?4 WHERE group_id=?1 AND pack=?2 AND sticker=?3",params![group,s.pack_key,s.sticker.id,i64::try_from(n)?])?;
            }
            tx.commit()?;
        }
        Ok(())
    }
    fn resolve_rows(&self, rows: Vec<StickerKey>) -> Result<Vec<LibrarySticker>> {
        let mut stickers = std::collections::HashMap::new();
        for saved in self.packs()? {
            let key = saved.pack.reference.key();
            for sticker in saved.pack.stickers {
                stickers.insert(
                    (key.clone(), sticker.id.clone()),
                    LibrarySticker {
                        pack_key: key.clone(),
                        pack_title: saved.pack.title.clone(),
                        provider: saved.pack.reference.provider,
                        sticker,
                    },
                );
            }
        }
        Ok(rows
            .into_iter()
            .filter_map(|key| stickers.remove(&key))
            .collect())
    }

    /// # Errors
    /// Returns an error if group membership or its saved pack data cannot be read.
    pub fn group_items(&self, group: i64) -> Result<Vec<LibrarySticker>> {
        let mut q = self.conn.prepare(
            "SELECT pack,sticker FROM group_items WHERE group_id=?1 ORDER BY position,pack,sticker",
        )?;
        let rows = q
            .query_map([group], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        self.resolve_rows(rows)
    }
    /// # Errors
    /// Returns an error if the sticker is absent or its usage order cannot be stored.
    pub fn record_recent(&self, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute("INSERT INTO recents(pack,sticker,used) VALUES(?1,?2,COALESCE((SELECT MAX(used)+1 FROM recents),1)) ON CONFLICT(pack,sticker) DO UPDATE SET used=excluded.used",params![pack,sticker])?;

        Ok(())
    }
    /// # Errors
    /// Returns an error if usage ordering or its saved pack data cannot be read.
    pub fn recents(&self) -> Result<Vec<LibrarySticker>> {
        let mut q = self
            .conn
            .prepare("SELECT s.pack,s.id FROM stickers s LEFT JOIN recents r ON r.pack=s.pack AND r.sticker=s.id
                ORDER BY r.used DESC, s.added DESC, s.rowid ASC")?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        self.resolve_rows(rows)
    }
    /// # Errors
    /// Returns an error if saved settings cannot be queried or deserialized; missing settings use defaults.
    pub fn settings(&self) -> Result<Settings> {
        let s: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match s {
            Some(s) => serde_json::from_str(&s)?,
            None => Settings::default(),
        })
    }
    /// # Errors
    /// Returns an error if settings cannot be serialized or written to `SQLite`.
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES('app',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        Ok(())
    }
}
