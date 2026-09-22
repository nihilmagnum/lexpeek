use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::application::reading::{Translated, Translator};
use crate::domain::language::Language;
use crate::domain::text::SourceText;

/// Google Translate через endpoint словарного расширения Chrome, без ключа.
pub struct GoogleTranslator {
    agent: ureq::Agent,
}

impl GoogleTranslator {
    const ENDPOINT: &str = "https://clients5.google.com/translate_a/t";

    pub fn new() -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build();
        Self { agent: config.into() }
    }

    fn code(language: &Language) -> &str {
        match &*language.code {
            "zh" => "zh-CN",
            code => code,
        }
    }

    /// Код языка без региона: zh-CN становится zh.
    fn language_code(code: &str) -> String {
        code.split('-').next().unwrap_or(code).to_owned()
    }

    /// С автоопределением ответ приходит массивом пар «перевод, язык оригинала», по одной на каждый q.
    fn translated(response: &Value) -> Option<Translated> {
        let pair = response.get(0)?;
        Some(Translated {
            text: pair.get(0)?.as_str()?.to_owned(),
            source: pair.get(1).and_then(Value::as_str).map(Self::language_code),
        })
    }
}

impl Translator for GoogleTranslator {
    fn translate(&self, text: &SourceText, target: &Language) -> Result<Translated> {
        let response: Value = self
            .agent
            .post(Self::ENDPOINT)
            .query_pairs([("client", "dict-chrome-ex"), ("sl", "auto"), ("tl", Self::code(target))])
            .send_form([("q", text.as_str())])
            .context("Google Translate request failed")?
            .body_mut()
            .read_json()
            .context("Google Translate returned malformed response")?;
        Self::translated(&response).context("Google Translate response has no translation")
    }
}
