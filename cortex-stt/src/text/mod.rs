//! Output rendering — how a transcript is written, as distinct from what
//! the model heard.
//!
//! A request's `language` carries both questions. The base subtag goes to
//! the engine, which matches it against the model's declared set; every
//! subtag after it selects a rendering applied to the transcript on the
//! way out. See `docs/adr/0006-language-subtags-are-output-instructions.md`
//! for why one parameter carries both and what a rendering may not do.
//!
//! Which subtags carry meaning is each module's decision: Mandarin reads
//! script and region, a Serbian module would read only script, an English
//! one only region.

use std::sync::Arc;

mod mandarin;

#[cfg(test)]
mod tests;

/// A parsed BCP-47 tag, split only as far as this crate needs.
///
/// Deliberately not a full BCP-47 parser: variants, extensions and private
/// use are preserved in neither field because no rendering keys off them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locale {
    base: String,
    script: Option<String>,
    region: Option<String>,
}

impl Locale {
    /// Split a tag into base / script / region.
    ///
    /// A 4-letter subtag is a script (`Hant`, `Latn`), a 2-letter or
    /// 3-digit one is a region (`TW`, `419`). Anything else is ignored
    /// rather than rejected: an unparseable subtag means "no rendering",
    /// which is the same outcome as omitting it.
    pub fn parse(tag: &str) -> Self {
        let mut parts = tag.split(['-', '_']).filter(|p| !p.is_empty());
        let base = parts.next().unwrap_or_default().to_ascii_lowercase();
        let (mut script, mut region) = (None, None);
        for p in parts {
            if p.len() == 4 && p.chars().all(|c| c.is_ascii_alphabetic()) {
                script.get_or_insert_with(|| titlecase(p));
            } else if (p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic()))
                || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit()))
            {
                region.get_or_insert_with(|| p.to_ascii_uppercase());
            }
        }
        Self {
            base,
            script,
            region,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn script(&self) -> Option<&str> {
        self.script.as_deref()
    }

    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// Whether anything beyond the base language was requested. A bare
    /// base code is the opt-out: it renders nothing, which is what every
    /// caller got before rendering existed.
    pub fn has_subtags(&self) -> bool {
        self.script.is_some() || self.region.is_some()
    }
}

fn titlecase(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase(),
        None => String::new(),
    }
}

/// One rewriting step. `id` names it on the wire, so it is part of the
/// contract and changing one is a wire change.
pub trait TextTransform: Send + Sync {
    fn id(&self) -> &'static str;
    fn apply(&self, text: &str) -> String;
}

/// Everything one language knows about rendering its own output.
///
/// A module is free to return nothing for a locale it does not recognise;
/// an unknown region is not an error, it just means no rendering.
trait LanguageModule: Send + Sync {
    /// The base language subtag this module answers for.
    fn base(&self) -> &'static str;
    fn transforms(&self, locale: &Locale) -> Vec<Arc<dyn TextTransform>>;
}

fn modules() -> &'static [&'static dyn LanguageModule] {
    &[&mandarin::Mandarin]
}

/// The renderings a request asked for, resolved once so the per-utterance
/// path is a slice walk.
#[derive(Clone)]
pub struct Renderer {
    transforms: Vec<Arc<dyn TextTransform>>,
}

impl Renderer {
    /// Resolve a request's `language` into a rendering, or `None` when the
    /// request asked for none.
    ///
    /// Keyed on what the caller requested, not on what the engine resolved
    /// it to: the engine drops every subtag (no catalog model declares
    /// one), which is precisely why they are free to mean this instead.
    pub fn for_language(requested: Option<&str>) -> Option<Self> {
        let locale = Locale::parse(requested?);
        if !locale.has_subtags() {
            return None;
        }
        let module = modules().iter().find(|m| m.base() == locale.base())?;
        let transforms = module.transforms(&locale);
        (!transforms.is_empty()).then_some(Self { transforms })
    }

    pub fn render(&self, text: &str) -> String {
        self.transforms
            .iter()
            .fold(text.to_string(), |acc, t| t.apply(&acc))
    }

    /// Applied transform ids, in order. Reported on the wire so a response
    /// says what was done to it rather than leaving a consumer to guess.
    pub fn ids(&self) -> Vec<&'static str> {
        self.transforms.iter().map(|t| t.id()).collect()
    }
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("ids", &self.ids())
            .finish()
    }
}
