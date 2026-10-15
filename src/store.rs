use crate::model::*;
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct Store {
    conn: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }
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
        PRAGMA user_version=1;")?;
        Ok(Self { conn })
    }
    pub fn save_pack(&self, pack: &Pack) -> Result<()> {
        self.save_pack_with_favorite(pack, false)
    }
    pub fn save_favorite_pack(&self, pack: &Pack) -> Result<()> {
        self.save_pack_with_favorite(pack, true)
    }
    fn save_pack_with_favorite(&self, pack: &Pack, favorite: bool) -> Result<()> {
        ensure!(!pack.stickers.is_empty(), "Pack has no stickers");
        let tx = self.conn.unchecked_transaction()?;
        let key = pack.reference.key();
        tx.execute("INSERT INTO packs(key,data,favorite) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET data=excluded.data,favorite=CASE WHEN ?3 THEN 1 ELSE packs.favorite END", params![key, serde_json::to_string(pack)?, favorite])?;
        for s in &pack.stickers {
            tx.execute(
                "INSERT OR IGNORE INTO stickers(pack,id) VALUES(?1,?2)",
                params![key, s.id],
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
    pub fn favorite(&self, key: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE packs SET favorite=?2 WHERE key=?1",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn remove_pack(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM packs WHERE key=?1", [key])?;
        Ok(())
    }
    pub fn groups(&self) -> Result<Vec<Group>> {
        let mut q = self
            .conn
            .prepare("SELECT id,name FROM groups ORDER BY id")?;
        Ok(q.query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
    }
    pub fn create_group(&self, name: &str) -> Result<i64> {
        ensure!(!name.trim().is_empty(), "Name the group first");
        self.conn
            .execute("INSERT INTO groups(name) VALUES(?1)", [name.trim()])?;
        Ok(self.conn.last_insert_rowid())
    }
    pub fn rename_group(&self, id: i64, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Name cannot be empty");
        self.conn.execute(
            "UPDATE groups SET name=?2 WHERE id=?1",
            params![id, name.trim()],
        )?;
        Ok(())
    }
    pub fn delete_group(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM groups WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn add_to_group(&self, group: i64, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO group_items(group_id,pack,sticker,position) VALUES(?1,?2,?3,COALESCE((SELECT MAX(position)+1 FROM group_items WHERE group_id=?1),0))",params![group,pack,sticker])?;
        Ok(())
    }
    pub fn remove_from_group(&self, group: i64, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM group_items WHERE group_id=?1 AND pack=?2 AND sticker=?3",
            params![group, pack, sticker],
        )?;
        Ok(())
    }
    pub fn move_in_group(&self, group: i64, pack: &str, sticker: &str, delta: i32) -> Result<()> {
        let mut items = self.group_items(group)?;
        if let Some(i) = items
            .iter()
            .position(|s| s.pack_key == pack && s.sticker.id == sticker)
        {
            let j = (i as i32 + delta).clamp(0, items.len() as i32 - 1) as usize;
            items.swap(i, j);
            let tx = self.conn.unchecked_transaction()?;
            for (n, s) in items.iter().enumerate() {
                tx.execute("UPDATE group_items SET position=?4 WHERE group_id=?1 AND pack=?2 AND sticker=?3",params![group,s.pack_key,s.sticker.id,n as i64])?;
            }
            tx.commit()?;
        }
        Ok(())
    }
    fn resolve_rows(&self, rows: Vec<(String, String)>) -> Result<Vec<LibrarySticker>> {
        let packs = self.packs()?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, id)| {
                let p = packs.iter().find(|p| p.pack.reference.key() == key)?;
                let sticker = p.pack.stickers.iter().find(|s| s.id == id)?.clone();
                Some(LibrarySticker {
                    pack_key: key,
                    pack_title: p.pack.title.clone(),
                    provider: p.pack.reference.provider,
                    sticker,
                })
            })
            .collect())
    }
    pub fn group_items(&self, group: i64) -> Result<Vec<LibrarySticker>> {
        let mut q = self.conn.prepare(
            "SELECT pack,sticker FROM group_items WHERE group_id=?1 ORDER BY position,pack,sticker",
        )?;
        let rows = q
            .query_map([group], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        self.resolve_rows(rows)
    }
    pub fn record_recent(&self, pack: &str, sticker: &str) -> Result<()> {
        self.conn.execute("INSERT INTO recents(pack,sticker,used) VALUES(?1,?2,COALESCE((SELECT MAX(used)+1 FROM recents),1)) ON CONFLICT(pack,sticker) DO UPDATE SET used=excluded.used",params![pack,sticker])?;
        self.conn.execute("DELETE FROM recents WHERE rowid NOT IN (SELECT rowid FROM recents ORDER BY used DESC LIMIT 100)",[])?;
        Ok(())
    }
    pub fn recents(&self) -> Result<Vec<LibrarySticker>> {
        let mut q = self
            .conn
            .prepare("SELECT pack,sticker FROM recents ORDER BY used DESC LIMIT 100")?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        self.resolve_rows(rows)
    }
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
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES('app',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        Ok(())
    }
}
