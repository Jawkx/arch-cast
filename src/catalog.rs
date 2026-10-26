use crate::{
    calculator::calculate,
    desktop,
    paths::Paths,
    storage::{read_object, take, write_json},
};
use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

pub const COLOR_KEYS: [&str; 8] = [
    "background",
    "surface",
    "selection",
    "border",
    "text",
    "muted",
    "accent",
    "success",
];
pub const THEME_NAMES: [&str; 4] = [
    "Catppuccin Mocha",
    "Catppuccin Macchiato",
    "Catppuccin Latte",
    "Tokyo Night",
];
const PALETTES: [[u32; 8]; 4] = [
    [
        0x1e1e2e, 0x181825, 0x313244, 0x45475a, 0xcdd6f4, 0xa6adc8, 0xcba6f7, 0xa6e3a1,
    ],
    [
        0x24273a, 0x1e2030, 0x363a4f, 0x494d64, 0xcad3f5, 0xa5adcb, 0xc6a0f6, 0xa6da95,
    ],
    [
        0xeff1f5, 0xe6e9ef, 0xccd0da, 0xbcc0cc, 0x4c4f69, 0x6c6f85, 0x8839ef, 0x40a02b,
    ],
    [
        0x1a1b26, 0x16161e, 0x292e42, 0x414868, 0xc0caf5, 0x9aa5ce, 0x7aa2f7, 0x9ece6a,
    ],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Application,
    Command,
    Calculator,
    #[serde(rename = "Web Search")]
    WebSearch,
}
impl EntryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::Command => "Command",
            Self::Calculator => "Calculator",
            Self::WebSearch => "Web Search",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    pub detail: String,
    pub icon: String,
    pub target: String,
    pub keywords: String,
}
impl Entry {
    pub fn app(id: &str, name: &str, keywords: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: EntryKind::Application,
            detail: String::new(),
            icon: "application-x-executable".into(),
            target: String::new(),
            keywords: keywords.into(),
        }
    }
    pub fn customizable(&self) -> bool {
        self.id.starts_with("app:") || self.id.starts_with("custom:")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default = "custom_description")]
    pub description: String,
    #[serde(default = "custom_icon")]
    pub icon: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
fn custom_description() -> String {
    "Custom command".into()
}
fn custom_icon() -> String {
    "utilities-terminal".into()
}

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub theme: String,
    pub colors: BTreeMap<String, String>,
    pub font: String,
    pub font_size: u32,
    pub width: u32,
    pub commands: Vec<CustomCommand>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            theme: THEME_NAMES[0].into(),
            colors: BTreeMap::new(),
            font: "Noto Sans".into(),
            font_size: 12,
            width: 840,
            commands: vec![],
            extra: Map::new(),
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Self {
        Self::from_object(read_object(path))
    }
    fn from_object(mut map: Map<String, Value>) -> Self {
        let default = Self::default();
        let theme = take(&mut map, "theme", default.theme);
        let colors: Map<String, Value> = take(&mut map, "colors", Map::new());
        let colors = colors
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k, v.to_owned())))
            .collect();
        let font = take(&mut map, "font", default.font);
        let font_size: u32 = take(&mut map, "font_size", default.font_size);
        let width: u32 = take(&mut map, "width", default.width);
        let commands: Vec<Value> = take(&mut map, "commands", vec![]);
        let commands = commands
            .into_iter()
            .filter_map(|v| serde_json::from_value::<CustomCommand>(v).ok())
            .filter(|c| !c.id.is_empty() && !c.name.is_empty() && !c.command.is_empty())
            .collect();
        Self {
            theme,
            colors,
            font,
            font_size: font_size.clamp(10, 20),
            width: width.clamp(640, 1200),
            commands,
            extra: map,
        }
    }
    pub fn palette(&self) -> Palette {
        let index = THEME_NAMES
            .iter()
            .position(|n| *n == self.theme)
            .unwrap_or(0);
        let mut colors = PALETTES[index];
        for (key, value) in &self.colors {
            if let Some(i) = COLOR_KEYS.iter().position(|k| k == key)
                && let Some(color) = parse_color(value)
            {
                colors[i] = color;
            }
        }
        Palette(colors)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette(pub [u32; 8]);
impl Palette {
    pub fn get(self, key: &str) -> u32 {
        self.0[COLOR_KEYS
            .iter()
            .position(|k| *k == key)
            .expect("known palette key")]
    }
}
pub fn parse_color(value: &str) -> Option<u32> {
    (value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit()))
    .then(|| u32::from_str_radix(&value[1..], 16).ok())
    .flatten()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Usage {
    pub count: u64,
    pub last: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub favorites: Vec<String>,
    pub hidden: Vec<String>,
    pub aliases: BTreeMap<String, String>,
    pub usage: BTreeMap<String, Usage>,
    pub learned: IndexMap<String, String>,
    pub history: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            favorites: [
                "app:google-chrome.desktop",
                "app:com.mitchellh.ghostty.desktop",
                "app:steam.desktop",
                "app:chrome-hnpfjngllnobngcgfapefoaidbinmjnm-Default.desktop",
            ]
            .map(str::to_owned)
            .into(),
            hidden: vec![],
            aliases: BTreeMap::new(),
            usage: BTreeMap::new(),
            learned: IndexMap::new(),
            history: vec![],
            extra: Map::new(),
        }
    }
}
impl State {
    fn load(path: &Path) -> Self {
        let mut map = read_object(path);
        let favorites = take(&mut map, "favorites", Self::default().favorites);
        let hidden = take(&mut map, "hidden", vec![]);
        let aliases = take(&mut map, "aliases", BTreeMap::new());
        let raw_usage: Map<String, Value> = take(&mut map, "usage", Map::new());
        let usage = raw_usage
            .into_iter()
            .filter_map(|(k, v)| {
                serde_json::from_value::<Usage>(v)
                    .ok()
                    .filter(|u| u.last.is_finite())
                    .map(|v| (k, v))
            })
            .collect();
        let learned = take(&mut map, "learned", IndexMap::new());
        let history = take(&mut map, "history", vec![]);
        Self {
            favorites,
            hidden,
            aliases,
            usage,
            learned,
            history,
            extra: map,
        }
    }
}

pub fn folded(s: &str) -> String {
    s.nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .replace('ß', "ss")
}
pub fn match_score(query: &str, entry: &Entry, alias: &str) -> f64 {
    let query = folded(query);
    let name = folded(&entry.name);
    let alias = folded(alias);
    if !alias.is_empty() && query == alias {
        return 1500.;
    }
    if query == name {
        return 1200.;
    }
    if name.starts_with(&query) {
        return 900. - name.chars().count() as f64 * 0.1;
    }
    let acronym: String = name
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| w.chars().next())
        .collect();
    if acronym.starts_with(&query) {
        return 780.;
    }
    if let Some(at) = name.find(&query) {
        return 650. - name[..at].chars().count() as f64;
    }
    let haystack = format!(
        "{} {} {} {}",
        name,
        alias,
        folded(&entry.keywords),
        folded(&entry.detail)
    );
    let tokens: Vec<_> = query.split_whitespace().collect();
    if !tokens.is_empty() && tokens.iter().all(|q| haystack.contains(q)) {
        return 400.;
    }
    let mut chars = name.chars();
    let mut gaps = 0;
    for target in query.chars() {
        loop {
            match chars.next() {
                Some(c) if c == target => break,
                Some(_) => gaps += 1,
                None => return 0.,
            }
        }
    }
    if query.is_empty() {
        0.
    } else {
        (250. - gaps as f64 * 8.).max(20.)
    }
}

pub struct Catalog {
    pub paths: Paths,
    pub config: Config,
    pub state: State,
    pub entries: Vec<Entry>,
    applications: Option<Vec<Entry>>,
}
impl Catalog {
    pub fn new(paths: Paths) -> Self {
        Self::with_applications(paths, None)
    }
    pub fn with_applications(paths: Paths, applications: Option<Vec<Entry>>) -> Self {
        let mut catalog = Self {
            paths,
            config: Config::default(),
            state: State::default(),
            entries: vec![],
            applications,
        };
        catalog.reload();
        catalog
    }
    pub fn reload(&mut self) {
        self.config = Config::load(&self.paths.config.join("config.json"));
        self.state = State::load(&self.paths.state.join("state.json"));
        self.entries = self.applications.clone().unwrap_or_else(|| {
            desktop::discover(
                &self.paths.applications(),
                &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
                &std::env::var("LC_MESSAGES")
                    .or_else(|_| std::env::var("LANG"))
                    .unwrap_or_default(),
            )
        });
        for (id, name, detail, icon) in [
            (
                "settings",
                "Launcher Settings",
                "Theme, appearance & hidden results",
                "preferences-desktop-theme",
            ),
            (
                "reload",
                "Refresh Applications",
                "Rescan installed apps and custom commands",
                "view-refresh",
            ),
            (
                "config",
                "Open Launcher Config",
                "Edit custom commands and theme colors",
                "text-x-generic",
            ),
        ] {
            self.entries.push(Entry {
                id: id.into(),
                name: name.into(),
                kind: EntryKind::Command,
                detail: detail.into(),
                icon: icon.into(),
                target: String::new(),
                keywords: String::new(),
            });
        }
        for c in &self.config.commands {
            self.entries.push(Entry {
                id: format!("custom:{}", c.id),
                name: c.name.clone(),
                kind: EntryKind::Command,
                detail: c.description.clone(),
                icon: c.icon.clone(),
                target: c.command.clone(),
                keywords: c.keywords.clone(),
            });
        }
    }
    pub fn save_config(&self) -> Result<()> {
        write_json(&self.paths.config.join("config.json"), &self.config)
    }
    pub fn save_state(&self) -> Result<()> {
        write_json(&self.paths.state.join("state.json"), &self.state)
    }
    pub fn search(&self, query: &str) -> Vec<Entry> {
        self.search_at(query, now())
    }
    pub fn search_at(&self, query: &str, now: f64) -> Vec<Entry> {
        let query = query.trim();
        let mut ranked = Vec::new();
        for entry in &self.entries {
            if self.state.hidden.contains(&entry.id) {
                continue;
            }
            let mut score = if query.is_empty() {
                1.
            } else {
                match_score(
                    query,
                    entry,
                    self.state
                        .aliases
                        .get(&entry.id)
                        .map(String::as_str)
                        .unwrap_or(""),
                )
            };
            let learned =
                !query.is_empty() && self.state.learned.get(&folded(query)) == Some(&entry.id);
            if !query.is_empty() && score == 0. && !learned {
                continue;
            }
            if let Some(usage) = self.state.usage.get(&entry.id) {
                let age = ((now - usage.last) / 86400.).max(0.);
                score += ((usage.count as f64 + 1.).log2() * 12.).min(80.) / (1. + age / 7.);
            }
            if let Some(index) = self.state.favorites.iter().position(|id| *id == entry.id) {
                score += if query.is_empty() {
                    500. - index as f64
                } else {
                    35.
                };
            }
            if learned {
                score += 1000.;
            }
            if query.is_empty() && entry.id == "settings" {
                score += 20.;
            }
            ranked.push((score, entry));
        }
        ranked.sort_by(|(a, ea), (b, eb)| {
            b.total_cmp(a)
                .then_with(|| folded(&ea.name).cmp(&folded(&eb.name)))
        });
        let mut results: Vec<Entry> = ranked
            .into_iter()
            .take(60)
            .map(|(_, e)| e.clone())
            .collect();
        if !query.is_empty() {
            if let Some(answer) = calculate(query) {
                results.insert(
                    0,
                    Entry {
                        id: "calculator".into(),
                        name: answer.clone(),
                        kind: EntryKind::Calculator,
                        detail: format!("{query} · Enter to copy"),
                        icon: "accessories-calculator".into(),
                        target: answer,
                        keywords: String::new(),
                    },
                );
            }
            let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
            for (id, name, base) in [
                (
                    "web:google",
                    "Search Google",
                    "https://www.google.com/search?q=",
                ),
                (
                    "web:duckduckgo",
                    "Search DuckDuckGo",
                    "https://duckduckgo.com/?q=",
                ),
            ] {
                results.push(Entry {
                    id: id.into(),
                    name: name.into(),
                    kind: EntryKind::WebSearch,
                    detail: query.into(),
                    icon: "internet-web-browser".into(),
                    target: format!("{base}{encoded}"),
                    keywords: String::new(),
                });
            }
        }
        results
    }
    pub fn favorite(&mut self, entry: &Entry) -> Result<()> {
        if !entry.customizable() {
            return Ok(());
        }
        if self.state.favorites.contains(&entry.id) {
            self.state.favorites.retain(|id| *id != entry.id);
        } else {
            self.state.favorites.push(entry.id.clone());
        }
        self.save_state()
    }
    pub fn alias(&mut self, entry: &Entry, alias: &str) -> Result<()> {
        if alias.trim().is_empty() {
            self.state.aliases.remove(&entry.id);
        } else {
            self.state
                .aliases
                .insert(entry.id.clone(), alias.trim().into());
        }
        self.save_state()
    }
    pub fn hide(&mut self, entry: &Entry) -> Result<()> {
        if entry.customizable() && !self.state.hidden.contains(&entry.id) {
            self.state.hidden.push(entry.id.clone());
        }
        self.save_state()
    }
    pub fn reset_ranking(&mut self, entry: &Entry) -> Result<()> {
        self.state.usage.remove(&entry.id);
        self.state.learned.retain(|_, id| *id != entry.id);
        self.save_state()
    }
    pub fn record(&mut self, entry: &Entry, query: &str) -> Result<()> {
        self.record_at(entry, query, now())
    }
    pub fn record_at(&mut self, entry: &Entry, query: &str, now: f64) -> Result<()> {
        if entry.customizable() {
            let usage = self.state.usage.entry(entry.id.clone()).or_default();
            usage.count = usage.count.saturating_add(1);
            usage.last = now;
            if !query.trim().is_empty() {
                let key = folded(query.trim());
                self.state.learned.shift_remove(&key);
                self.state.learned.insert(key, entry.id.clone());
                while self.state.learned.len() > 500 {
                    self.state.learned.shift_remove_index(0);
                }
            }
        }
        if !query.trim().is_empty() {
            self.state.history.retain(|q| q != query);
            self.state.history.insert(0, query.into());
            self.state.history.truncate(100);
        }
        self.save_state()
    }
    pub fn section(&self, entry: &Entry, query: &str) -> &'static str {
        if entry.kind == EntryKind::WebSearch {
            "Search the web"
        } else if entry.kind == EntryKind::Calculator {
            "Calculator"
        } else if !query.trim().is_empty() {
            "Results"
        } else if self.state.favorites.contains(&entry.id) {
            "Favorites"
        } else {
            "Suggestions"
        }
    }
}
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Catalog) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: tmp.path().join("config"),
            state: tmp.path().join("state"),
            runtime: tmp.path().join("run"),
            data: tmp.path().join("data"),
            home: tmp.path().into(),
        };
        let apps = vec![
            Entry::app("app:firefox.desktop", "Firefox", "browser web"),
            Entry::app("app:files.desktop", "Files", ""),
            Entry::app("app:terminal.desktop", "Ghostty", "terminal shell"),
        ];
        (tmp, Catalog::with_applications(paths, Some(apps)))
    }
    #[test]
    fn search_and_web_fallbacks() {
        let (_tmp, c) = fixture();
        for (q, name) in [
            ("fire", "Firefox"),
            ("ff", "Firefox"),
            ("browser", "Firefox"),
            ("ghsty", "Ghostty"),
        ] {
            assert_eq!(c.search(q)[0].name, name);
        }
        assert_eq!(
            c.search("nonexistent-query-999")
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>(),
            [EntryKind::WebSearch; 2]
        );
        let results = c.search("hello & goodbye; $HOME");
        assert!(results.last().unwrap().target.contains("%26"));
        assert!(results.last().unwrap().target.contains("%24HOME"));
        assert_eq!(c.search("12 * (3 + 4)")[0].target, "84");
    }
    #[test]
    fn favorites_aliases_hidden_and_persistence() {
        let (_tmp, mut c) = fixture();
        let entry = c.entries[0].clone();
        c.favorite(&entry).unwrap();
        c.alias(&entry, "browse").unwrap();
        assert_eq!(c.search("")[0], entry);
        assert_eq!(c.search("browse")[0], entry);
        c.hide(&entry).unwrap();
        assert!(!c.search("browse").contains(&entry));
        c.reload();
        assert!(c.state.hidden.contains(&entry.id));
        assert_eq!(c.state.aliases[&entry.id], "browse");
        c.state.hidden.clear();
        c.alias(&entry, "").unwrap();
        c.favorite(&entry).unwrap();
        assert!(!c.state.aliases.contains_key(&entry.id));
        assert!(!c.state.favorites.contains(&entry.id));
    }
    #[test]
    fn learning_history_recency_and_reset() {
        let (_tmp, mut c) = fixture();
        let entry = c.entries[0].clone();
        c.record_at(&entry, "work", 1000000.).unwrap();
        assert_eq!(c.search_at("work", 1000000.)[0], entry);
        assert_eq!(c.state.usage[&entry.id].count, 1);
        c.record_at(&entry, "work", 1000001.).unwrap();
        assert_eq!(c.state.history, ["work"]);
        c.reset_ranking(&entry).unwrap();
        assert!(!c.state.usage.contains_key(&entry.id));
        assert!(!c.state.learned.contains_key("work"));
    }
    #[test]
    fn config_validation_and_unknown_keys_survive() {
        let (_tmp, mut c) = fixture();
        write_json(&c.paths.config.join("config.json"),&serde_json::json!({"theme":"Tokyo Night","colors":{"accent":"#123456","text":"bad","unknown":"#ffffff"},"font_size":999,"width":1,"future":true,
            "commands":[{"id":"test","name":"Test action","command":"echo hi","future":42},{},"bad"]})).unwrap();
        c.reload();
        assert_eq!(c.config.palette().get("accent"), 0x123456);
        assert_eq!(c.config.palette().get("text"), 0xc0caf5);
        assert_eq!(c.config.font_size, 20);
        assert_eq!(c.config.width, 640);
        assert_eq!(c.search("Test action")[0].target, "echo hi");
        c.save_config().unwrap();
        c.reload();
        assert_eq!(c.config.extra["future"], true);
        assert_eq!(c.config.commands[0].extra["future"], 42);
    }
    #[test]
    fn corrupt_files_and_bad_usage_do_not_prevent_startup() {
        let (_tmp, mut c) = fixture();
        std::fs::create_dir_all(&c.paths.config).unwrap();
        std::fs::create_dir_all(&c.paths.state).unwrap();
        std::fs::write(c.paths.config.join("config.json"), "{broken").unwrap();
        std::fs::write(c.paths.state.join("state.json"),r#"{"favorites":false,"aliases":null,"usage":{"app:firefox.desktop":{"count":"bad","last":0}}}"#).unwrap();
        c.reload();
        assert_eq!(c.config.palette(), Config::default().palette());
        assert!(c.state.usage.is_empty());
        assert!(!c.state.favorites.is_empty());
        assert!(!c.search("fire").is_empty());
    }
    #[test]
    fn unicode_matching_and_bounded_history() {
        assert!(match_score("strasse", &Entry::app("a", "Straße", ""), "") > 0.);
        let (_tmp, mut c) = fixture();
        let entry = c.entries[0].clone();
        for i in 0..510 {
            c.record_at(&entry, &format!("query {i}"), i as f64)
                .unwrap();
        }
        assert_eq!(c.state.learned.len(), 500);
        assert_eq!(c.state.history.len(), 100);
        assert!(!c.state.learned.contains_key("query 0"));
        assert!(c.state.learned.contains_key("query 509"));
    }
}
