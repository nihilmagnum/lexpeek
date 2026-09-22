use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::application::dictionaries::DictionaryStore;
use crate::domain::dictionary::{Article, Dictionary, HeadwordKey};
use crate::domain::lookup::Dictionaries;

/// Импортированные словари в SQLite.
pub struct SqliteLibrary {
    connection: Mutex<Connection>,
}

impl SqliteLibrary {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).with_context(|| format!("cannot create {}", directory.display()))?;
        }
        let connection = Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS dictionaries (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS articles (
                 id INTEGER PRIMARY KEY,
                 dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
                 headword TEXT NOT NULL,
                 body TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS headwords (
                 key TEXT NOT NULL,
                 article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
                 PRIMARY KEY (key, article_id)
             ) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS articles_by_dictionary ON articles(dictionary_id);
             CREATE INDEX IF NOT EXISTS headwords_by_article ON headwords(article_id);",
        )?;
        Ok(Self { connection: Mutex::new(connection) })
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        // Соединение не остаётся в сломанном состоянии: транзакция при панике откатывается.
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl DictionaryStore for SqliteLibrary {
    fn save(&self, dictionary: &Dictionary) -> Result<()> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM dictionaries WHERE name = ?1", [&dictionary.name])?;
        transaction.execute("INSERT INTO dictionaries (name) VALUES (?1)", [&dictionary.name])?;
        let dictionary_id = transaction.last_insert_rowid();
        {
            let mut insert_article =
                transaction.prepare("INSERT INTO articles (dictionary_id, headword, body) VALUES (?1, ?2, ?3)")?;
            let mut insert_headword =
                transaction.prepare("INSERT OR IGNORE INTO headwords (key, article_id) VALUES (?1, ?2)")?;
            for entry in &dictionary.entries {
                insert_article.execute(params![dictionary_id, entry.headword, entry.body])?;
                let article_id = transaction.last_insert_rowid();
                for headword in std::iter::once(&entry.headword).chain(&entry.aliases) {
                    insert_headword.execute(params![HeadwordKey::new(headword).as_str(), article_id])?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

impl Dictionaries for SqliteLibrary {
    fn articles(&self, key: &HeadwordKey) -> Result<Vec<Article>> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(
            "SELECT d.name, a.headword, a.body
             FROM headwords h
             JOIN articles a ON a.id = h.article_id
             JOIN dictionaries d ON d.id = a.dictionary_id
             WHERE h.key = ?1
             ORDER BY d.id, a.id",
        )?;
        let rows = statement.query_map([key.as_str()], |row| {
            Ok(Article { dictionary: row.get(0)?, headword: row.get(1)?, body: row.get(2)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dictionary::DictionaryEntry;

    #[test]
    fn finds_article_by_alias_and_replaces_reimported_dictionary() {
        let directory = tempfile::tempdir().unwrap();
        let library = SqliteLibrary::open(&directory.path().join("library.sqlite")).unwrap();
        let dictionary = |body: &str| Dictionary {
            name: "Test".into(),
            entries: vec![DictionaryEntry { headword: "Play".into(), aliases: vec!["played".into()], body: body.into() }],
        };

        library.save(&dictionary("играть")).unwrap();
        library.save(&dictionary("играть, пьеса")).unwrap();

        let articles = library.articles(&HeadwordKey::new("played")).unwrap();
        assert_eq!(
            articles,
            [Article { dictionary: "Test".into(), headword: "Play".into(), body: "играть, пьеса".into() }]
        );
    }
}
