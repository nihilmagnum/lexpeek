use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;

use crate::application::dictionaries::DictionaryReader;
use crate::domain::dictionary::{Dictionary, DictionaryEntry};

/// Чтение словарей в формате StarDict по пути к .ifo.
pub struct StarDictReader;

impl DictionaryReader for StarDictReader {
    fn read(&self, path: &Path) -> Result<Dictionary> {
        Self::open(path)
    }
}

impl StarDictReader {
    /// Словоформы из .syn становятся синонимами статей, на которые ведут.
    fn open(ifo_path: &Path) -> Result<Dictionary> {
        let info = Self::read_info(ifo_path)?;
        let name = info.get("bookname").cloned().unwrap_or_else(|| Self::file_stem(ifo_path));
        let offset_bytes = match info.get("idxoffsetbits").map(String::as_str) {
            Some("64") => 8,
            _ => 4,
        };
        let types = info.get("sametypesequence").cloned();

        let index = Self::read_optional(ifo_path, &["idx", "idx.gz"])?
            .with_context(|| format!("index file for {} not found", ifo_path.display()))?;
        let data = Self::read_optional(ifo_path, &["dict", "dict.dz"])?
            .with_context(|| format!("dict file for {} not found", ifo_path.display()))?;

        let mut entries = Vec::new();
        let mut reader = ByteReader::new(&index);
        while !reader.is_empty() {
            let headword = reader.string()?;
            let offset = reader.number(offset_bytes)? as usize;
            let size = reader.number(4)? as usize;
            let article = data
                .get(offset..offset + size)
                .with_context(|| format!("article {headword:?} is out of dict bounds"))?;
            entries.push(DictionaryEntry {
                headword,
                aliases: Vec::new(),
                body: Self::article_text(article, types.as_deref())?,
            });
        }

        if let Some(synonyms) = Self::read_optional(ifo_path, &["syn"])? {
            let mut reader = ByteReader::new(&synonyms);
            while !reader.is_empty() {
                let alias = reader.string()?;
                let position = reader.number(4)? as usize;
                let entry = entries
                    .get_mut(position)
                    .with_context(|| format!("synonym {alias:?} points past the index"))?;
                entry.aliases.push(alias);
            }
        }

        Ok(Dictionary { name, entries })
    }

    fn read_info(ifo_path: &Path) -> Result<HashMap<String, String>> {
        let text = fs::read_to_string(ifo_path).with_context(|| format!("cannot read {}", ifo_path.display()))?;
        let mut lines = text.lines();
        ensure!(
            lines.next().map(str::trim) == Some("StarDict's dict ifo file"),
            "{} is not a StarDict .ifo file",
            ifo_path.display()
        );
        Ok(lines
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
            .collect())
    }

    /// Файл рядом с .ifo с одним из расширений; .gz и .dz распаковываются.
    fn read_optional(ifo_path: &Path, extensions: &[&str]) -> Result<Option<Vec<u8>>> {
        for extension in extensions {
            let path = Self::sibling(ifo_path, extension);
            if !path.exists() {
                continue;
            }
            let raw = fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            if !extension.ends_with("gz") && !extension.ends_with("dz") {
                return Ok(Some(raw));
            }
            let mut unpacked = Vec::new();
            GzDecoder::new(raw.as_slice())
                .read_to_end(&mut unpacked)
                .with_context(|| format!("cannot unpack {}", path.display()))?;
            return Ok(Some(unpacked));
        }
        Ok(None)
    }

    fn sibling(ifo_path: &Path, extension: &str) -> PathBuf {
        ifo_path.with_file_name(format!("{}.{extension}", Self::file_stem(ifo_path)))
    }

    fn file_stem(ifo_path: &Path) -> String {
        ifo_path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
    }

    /// Текстовые поля статьи. Без sametypesequence каждое поле начинается с буквы типа.
    fn article_text(article: &[u8], types: Option<&str>) -> Result<String> {
        let mut reader = ByteReader::new(article);
        let mut texts = Vec::new();
        match types {
            Some(types) => {
                let count = types.chars().count();
                for (position, kind) in types.chars().enumerate() {
                    let is_last = position + 1 == count;
                    texts.extend(Self::field_text(kind, reader.field(kind, is_last)?));
                }
            }
            None => {
                while !reader.is_empty() {
                    let kind = reader.byte()? as char;
                    texts.extend(Self::field_text(kind, reader.field(kind, false)?));
                }
            }
        }
        texts.retain(|text| !text.is_empty());
        Ok(texts.join("\n"))
    }

    fn field_text(kind: char, bytes: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(bytes);
        match kind {
            'm' | 'l' | 't' | 'y' | 'k' => Some(text.trim().to_owned()),
            'g' | 'h' | 'x' => Some(Self::strip_markup(&text)),
            _ => None,
        }
    }

    /// Разметка HTML, XDXF и Pango сводится к тексту: переносы строк сохраняются, теги убираются.
    fn strip_markup(markup: &str) -> String {
        let mut text = String::with_capacity(markup.len());
        let mut rest = markup;
        while let Some(open) = rest.find('<') {
            text.push_str(&rest[..open]);
            let Some(close) = rest[open..].find('>') else {
                rest = &rest[open..];
                break;
            };
            let tag = rest[open + 1..open + close].trim().to_lowercase();
            let name = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or_default();
            if matches!(name, "br" | "p" | "div" | "li" | "tr" | "def") {
                text.push('\n');
            }
            rest = &rest[open + close + 1..];
        }
        text.push_str(rest);
        let decoded = text
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ")
            .replace("&amp;", "&");
        decoded
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Последовательное чтение двоичных полей StarDict (числа в big-endian).
struct ByteReader<'a> {
    bytes: &'a [u8],
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn number(&mut self, width: usize) -> Result<u64> {
        Ok(self.take(width)?.iter().fold(0, |number, &byte| number << 8 | u64::from(byte)))
    }

    fn string(&mut self) -> Result<String> {
        Ok(String::from_utf8_lossy(self.terminated()?).into_owned())
    }

    /// Поле статьи: строчный тип заканчивается нулём, прописной предварён размером,
    /// последнее поле sametypesequence занимает остаток.
    fn field(&mut self, kind: char, is_last: bool) -> Result<&'a [u8]> {
        if is_last {
            return self.take(self.bytes.len());
        }
        if kind.is_ascii_uppercase() {
            let size = self.number(4)? as usize;
            return self.take(size);
        }
        self.terminated()
    }

    fn terminated(&mut self) -> Result<&'a [u8]> {
        let Some(end) = self.bytes.iter().position(|&byte| byte == 0) else {
            bail!("unterminated string in StarDict data");
        };
        let value = &self.bytes[..end];
        self.bytes = &self.bytes[end + 1..];
        Ok(value)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        ensure!(count <= self.bytes.len(), "truncated StarDict data");
        let (value, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_dictionary(directory: &Path) -> PathBuf {
        let articles = [("game", "игра"), ("play", "<b>играть</b><br>пьеса")];
        let mut index = Vec::new();
        let mut data = Vec::new();
        for (headword, body) in articles {
            index.extend_from_slice(headword.as_bytes());
            index.push(0);
            index.extend_from_slice(&(data.len() as u32).to_be_bytes());
            index.extend_from_slice(&(body.len() as u32).to_be_bytes());
            data.extend_from_slice(body.as_bytes());
        }
        let synonyms = [b"played\0".as_slice(), &1u32.to_be_bytes()].concat();

        let ifo = directory.join("test.ifo");
        fs::write(&ifo, "StarDict's dict ifo file\nversion=2.4.2\nbookname=Test EN-RU\nsametypesequence=h\n").unwrap();
        fs::write(directory.join("test.idx"), index).unwrap();
        fs::write(directory.join("test.dict"), data).unwrap();
        fs::write(directory.join("test.syn"), synonyms).unwrap();
        ifo
    }

    #[test]
    fn reads_articles_and_synonyms() {
        let directory = tempfile::tempdir().unwrap();
        let dictionary = StarDictReader::open(&write_dictionary(directory.path())).unwrap();

        assert_eq!(dictionary.name, "Test EN-RU");
        let play = &dictionary.entries[1];
        assert_eq!(play.headword, "play");
        assert_eq!(play.body, "играть\nпьеса");
        assert_eq!(play.aliases, ["played"]);
    }
}
