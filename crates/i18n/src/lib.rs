//! Localization / internationalization (i18n) system.
//!
//! Provides string-table based translation with locale switching,
//! fallback support, and parameter substitution.

use std::collections::HashMap;
use std::path::Path;

/// A string table mapping translation keys to localized text.
pub type StringTable = HashMap<String, String>;

/// Represents a locale identifier (e.g. "en", "zh-CN", "ja").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale(pub String);

impl Locale {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn id(&self) -> &str {
        &self.0
    }
}

impl<S: Into<String>> From<S> for Locale {
    fn from(s: S) -> Self {
        Self(s.into())
    }
}

/// Central localization manager that holds all loaded string tables
/// and tracks the current locale.
#[derive(Debug, Clone)]
pub struct LocalizationManager {
    /// String tables indexed by locale identifier.
    tables: HashMap<String, StringTable>,
    /// The currently active locale.
    current_locale: String,
}

impl Default for LocalizationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalizationManager {
    /// Create a new empty `LocalizationManager` with no locale set.
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            current_locale: String::new(),
        }
    }

    /// Load a string table from a JSON file for the given locale.
    ///
    /// The JSON file must be a flat object with string key-value pairs,
    /// e.g. `{ "greeting": "Hello", "farewell": "Goodbye" }`.
    pub fn load_locale(
        &mut self,
        locale: impl Into<String>,
        json_path: impl AsRef<Path>,
    ) -> Result<(), I18nError> {
        let locale = locale.into();
        let content = std::fs::read_to_string(json_path.as_ref()).map_err(|e| {
            I18nError::IoError {
                locale: locale.clone(),
                source: e,
            }
        })?;
        let table: StringTable = serde_json::from_str(&content).map_err(|e| {
            I18nError::ParseError {
                locale: locale.clone(),
                source: e,
            }
        })?;
        self.tables.insert(locale, table);
        Ok(())
    }

    /// Load a string table directly from a JSON string (useful for tests
    /// and embedded resources).
    pub fn load_locale_from_str(
        &mut self,
        locale: impl Into<String>,
        json_str: &str,
    ) -> Result<(), I18nError> {
        let locale = locale.into();
        let table: StringTable = serde_json::from_str(json_str).map_err(|e| {
            I18nError::ParseError {
                locale: locale.clone(),
                source: e,
            }
        })?;
        self.tables.insert(locale, table);
        Ok(())
    }

    /// Switch the current locale. The locale must have been previously loaded.
    pub fn set_locale(&mut self, locale: impl Into<String>) -> Result<(), I18nError> {
        let locale = locale.into();
        if !self.tables.contains_key(&locale) {
            return Err(I18nError::LocaleNotLoaded(locale));
        }
        self.current_locale = locale;
        Ok(())
    }

    /// Returns the current locale identifier, or an empty string if none is set.
    pub fn current_locale(&self) -> &str {
        &self.current_locale
    }

    /// Translate a key using the current locale's string table.
    /// Returns the key itself if no translation is found.
    pub fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.tables
            .get(&self.current_locale)
            .and_then(|table| table.get(key))
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    /// Translate a key with an explicit fallback when the key is missing.
    pub fn tr_with_fallback<'a>(&'a self, key: &'a str, fallback: &'a str) -> &'a str {
        self.tables
            .get(&self.current_locale)
            .and_then(|table| table.get(key))
            .map(|s| s.as_str())
            .unwrap_or(fallback)
    }

    /// Translate a key and perform parameter substitution.
    ///
    /// Placeholders in the translated string use the form `{name}`, and each
    /// entry in `args` is a `(name, value)` pair that will be substituted.
    ///
    /// # Example
    /// ```ignore
    /// mgr.tr_format("greet", &[("name", "World")])
    /// // "Hello {name}" → "Hello World"
    /// ```
    pub fn tr_format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let template = self.tr(key);
        let mut result = template.to_string();
        for (name, value) in args {
            let placeholder = format!("{{{name}}}");
            result = result.replace(&placeholder, value);
        }
        result
    }

    /// List all loaded locale identifiers.
    pub fn available_locales(&self) -> Vec<&str> {
        let mut locales: Vec<&str> = self.tables.keys().map(|s| s.as_str()).collect();
        locales.sort();
        locales
    }

    /// Returns `true` if a string table has been loaded for the given locale.
    pub fn has_locale(&self, locale: &str) -> bool {
        self.tables.contains_key(locale)
    }
}

/// Errors produced by the i18n system.
#[derive(Debug)]
pub enum I18nError {
    IoError {
        locale: String,
        source: std::io::Error,
    },
    ParseError {
        locale: String,
        source: serde_json::Error,
    },
    LocaleNotLoaded(String),
}

impl std::fmt::Display for I18nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError { locale, source } => {
                write!(f, "i18n: failed to load locale '{locale}': {source}")
            }
            Self::ParseError { locale, source } => {
                write!(f, "i18n: failed to parse locale '{locale}': {source}")
            }
            Self::LocaleNotLoaded(locale) => {
                write!(f, "i18n: locale '{locale}' has not been loaded")
            }
        }
    }
}

impl std::error::Error for I18nError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> LocalizationManager {
        let mut mgr = LocalizationManager::new();
        mgr.load_locale_from_str("en", r#"{"greeting": "Hello", "farewell": "Goodbye", "welcome": "Welcome {name} to {place}"}"#).unwrap();
        mgr.load_locale_from_str(
            "zh-CN",
            r#"{"greeting": "你好", "farewell": "再见", "welcome": "欢迎 {name} 来到 {place}"}"#,
        )
        .unwrap();
        mgr.set_locale("en").unwrap();
        mgr
    }

    #[test]
    fn test_load_and_available_locales() {
        let mgr = make_manager();
        let mut locales = mgr.available_locales();
        locales.sort();
        assert_eq!(locales, vec!["en", "zh-CN"]);
    }

    #[test]
    fn test_set_locale_and_tr() {
        let mut mgr = make_manager();
        assert_eq!(mgr.tr("greeting"), "Hello");

        mgr.set_locale("zh-CN").unwrap();
        assert_eq!(mgr.tr("greeting"), "你好");
        assert_eq!(mgr.current_locale(), "zh-CN");
    }

    #[test]
    fn test_tr_missing_key_returns_key() {
        let mgr = make_manager();
        assert_eq!(mgr.tr("nonexistent_key"), "nonexistent_key");
    }

    #[test]
    fn test_tr_with_fallback() {
        let mgr = make_manager();
        assert_eq!(mgr.tr_with_fallback("greeting", "Hi"), "Hello");
        assert_eq!(mgr.tr_with_fallback("missing_key", "fallback text"), "fallback text");
    }

    #[test]
    fn test_tr_format_parameter_substitution() {
        let mgr = make_manager();
        let result = mgr.tr_format("welcome", &[("name", "Alice"), ("place", "Wonderland")]);
        assert_eq!(result, "Welcome Alice to Wonderland");
    }

    #[test]
    fn test_tr_format_chinese() {
        let mut mgr = make_manager();
        mgr.set_locale("zh-CN").unwrap();
        let result = mgr.tr_format("welcome", &[("name", "小明"), ("place", "北京")]);
        assert_eq!(result, "欢迎 小明 来到 北京");
    }

    #[test]
    fn test_set_locale_not_loaded() {
        let mut mgr = LocalizationManager::new();
        let err = mgr.set_locale("fr").unwrap_err();
        assert!(matches!(err, I18nError::LocaleNotLoaded(_)));
    }

    #[test]
    fn test_has_locale() {
        let mgr = make_manager();
        assert!(mgr.has_locale("en"));
        assert!(!mgr.has_locale("fr"));
    }

    #[test]
    fn test_locale_struct() {
        let locale = Locale::new("ja");
        assert_eq!(locale.id(), "ja");
        let locale2: Locale = "en".into();
        assert_eq!(locale2.id(), "en");
    }
}
