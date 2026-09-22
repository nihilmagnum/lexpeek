use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::domain::dictionary::Dictionary;

pub trait DictionaryReader: Send + Sync {
    fn read(&self, path: &Path) -> Result<Dictionary>;
}

pub trait DictionaryStore: Send + Sync {
    /// Сохраняет словарь; словарь с тем же названием заменяется.
    fn save(&self, dictionary: &Dictionary) -> Result<()>;
}

pub struct ImportedDictionary {
    pub name: String,
    pub articles: usize,
}

/// Импорт словаря из файла в библиотеку.
pub struct DictionaryImport {
    reader: Arc<dyn DictionaryReader>,
    store: Arc<dyn DictionaryStore>,
}

impl DictionaryImport {
    pub fn new(reader: Arc<dyn DictionaryReader>, store: Arc<dyn DictionaryStore>) -> Self {
        Self { reader, store }
    }

    pub fn import(&self, path: &Path) -> Result<ImportedDictionary> {
        let dictionary = self.reader.read(path)?;
        self.store.save(&dictionary)?;
        Ok(ImportedDictionary { name: dictionary.name, articles: dictionary.entries.len() })
    }
}
