//! Chinese script rendering: the script subtag picks the side, the region
//! picks the variant within it. Glyph-level OpenCC configs only — the
//! phrase configs rewrite vocabulary, which ADR 0006 rules out.

use std::sync::{Arc, OnceLock};

use ferrous_opencc::OpenCC;
use ferrous_opencc::config::BuiltinConfig;

use super::{LanguageModule, Locale, TextTransform};

pub(super) struct Mandarin;

impl LanguageModule for Mandarin {
    fn base(&self) -> &'static str {
        "zh"
    }

    fn transforms(&self, locale: &Locale) -> Vec<Arc<dyn TextTransform>> {
        match select(locale) {
            Some(c) => vec![Arc::new(ScriptConversion(c))],
            None => Vec::new(),
        }
    }
}

/// Map a locale onto an OpenCC config.
///
/// The region wins where both are present: `zh-Hant-TW` wants Taiwan
/// character preferences (裡, 著, 麵), which the generic `s2t` does not
/// apply. A script alone lands on the generic config; a region alone
/// implies its usual script.
fn select(locale: &Locale) -> Option<Config> {
    let script = locale.script().map(|s| s.to_ascii_lowercase());
    match (script.as_deref(), locale.region()) {
        (_, Some("TW")) => Some(Config::S2tw),
        (_, Some("HK")) => Some(Config::S2hk),
        (_, Some("MO")) => Some(Config::S2hk),
        (_, Some("CN") | Some("SG")) => Some(Config::T2s),
        (Some("hant"), _) => Some(Config::S2t),
        (Some("hans"), _) => Some(Config::T2s),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Config {
    S2t,
    S2tw,
    S2hk,
    T2s,
}

impl Config {
    fn id(self) -> &'static str {
        match self {
            Config::S2t => "script:s2t",
            Config::S2tw => "script:s2tw",
            Config::S2hk => "script:s2hk",
            Config::T2s => "script:t2s",
        }
    }

    /// Dictionaries are embedded in the binary but parsing them is not
    /// free, so each converter is built once and shared.
    fn converter(self) -> Option<&'static OpenCC> {
        fn cell(c: Config) -> &'static OnceLock<Option<OpenCC>> {
            static S2T: OnceLock<Option<OpenCC>> = OnceLock::new();
            static S2TW: OnceLock<Option<OpenCC>> = OnceLock::new();
            static S2HK: OnceLock<Option<OpenCC>> = OnceLock::new();
            static T2S: OnceLock<Option<OpenCC>> = OnceLock::new();
            match c {
                Config::S2t => &S2T,
                Config::S2tw => &S2TW,
                Config::S2hk => &S2HK,
                Config::T2s => &T2S,
            }
        }
        let builtin = match self {
            Config::S2t => BuiltinConfig::S2t,
            Config::S2tw => BuiltinConfig::S2tw,
            Config::S2hk => BuiltinConfig::S2hk,
            Config::T2s => BuiltinConfig::T2s,
        };
        cell(self)
            .get_or_init(|| match OpenCC::from_config(builtin) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::error!(config = ?builtin, error = %e, "OpenCC init failed; text will pass through unrendered");
                    None
                }
            })
            .as_ref()
    }
}

struct ScriptConversion(Config);

impl TextTransform for ScriptConversion {
    fn id(&self) -> &'static str {
        self.0.id()
    }

    /// Rendering is optional; a successful transcript is not. A converter
    /// that failed to build returns the text untouched rather than an
    /// error, so a dictionary problem degrades the output instead of
    /// discarding it.
    fn apply(&self, text: &str) -> String {
        match self.0.converter() {
            Some(c) => c.convert(text),
            None => text.to_string(),
        }
    }
}
