//! RetroAchievements' web API (api-docs.retroachievements.org), behind a cache on disk:
//! its docs ask for the game lists to be cached hard, and they change slowly.
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::Value;

const API: &str = "https://retroachievements.org/API";
/// How long a fetched list is trusted before it is fetched again.
const FRESH: Duration = Duration::from_secs(3 * 24 * 60 * 60);

pub struct Game {
    pub id: u64,
    pub title: String,
    pub achievements: u64,
    pub hashes: Vec<String>,
}

/// One file RetroAchievements supports for a game.
pub struct Supported {
    pub name: String,
    /// Set when the file only works after applying this patch.
    pub patch: Option<String>,
}

pub struct Client {
    key: String,
    cache: PathBuf,
    refresh: bool,
}

impl Client {
    pub fn new(key: String, refresh: bool) -> Self {
        Client {
            key,
            cache: cache_dir(),
            refresh,
        }
    }

    /// Every game RetroAchievements has for a console, with the hashes it accepts.
    pub fn games(&self, console: u32) -> Result<Vec<Game>, String> {
        let body = self.get(
            &format!("games-{console}.json"),
            &format!("API_GetGameList.php?i={console}&h=1"),
        )?;
        let Value::Array(list) = body else {
            return Err(unexpected(&body));
        };
        Ok(list
            .iter()
            .map(|g| Game {
                id: number(field(g, "ID")),
                title: text(field(g, "Title")),
                achievements: number(field(g, "NumAchievements")),
                hashes: match field(g, "Hashes") {
                    Some(Value::Array(h)) => h
                        .iter()
                        .filter_map(|h| h.as_str())
                        .map(|h| h.to_ascii_lowercase())
                        .collect(),
                    _ => Vec::new(),
                },
            })
            .collect())
    }

    /// The files RetroAchievements supports for one game, by name.
    pub fn supported(&self, game: u64) -> Result<Vec<Supported>, String> {
        let body = self.get(
            &format!("hashes-{game}.json"),
            &format!("API_GetGameHashes.php?i={game}"),
        )?;
        let Some(Value::Array(list)) = field(&body, "Results") else {
            return Err(unexpected(&body));
        };
        Ok(list
            .iter()
            .map(|h| Supported {
                name: text(field(h, "Name")),
                patch: field(h, "PatchUrl")
                    .and_then(|p| p.as_str())
                    .filter(|p| !p.is_empty())
                    .map(str::to_string),
            })
            .collect())
    }

    fn get(&self, cache_name: &str, query: &str) -> Result<Value, String> {
        let cached = self.cache.join(cache_name);
        if !self.refresh {
            if let Some(body) = fresh(&cached) {
                return Ok(body);
            }
        }
        // RA_CHECK_API points at a stand-in server, for testing.
        let api = std::env::var("RA_CHECK_API").unwrap_or_else(|_| API.to_string());
        let url = format!("{api}/{query}&y={}", self.key);
        let response = slot_net::agent()
            .get(&url)
            .set("User-Agent", &user_agent())
            .call();
        let text = match response {
            Ok(r) => r
                .into_string()
                .map_err(|e| format!("RetroAchievements: {e}"))?,
            Err(slot_net::ureq::Error::Status(401 | 403, _)) => {
                return Err(
                    "RetroAchievements turned the web API key down. Copy it again \
                     from retroachievements.org/settings (Keys) and run with --key."
                        .into(),
                )
            }
            Err(slot_net::ureq::Error::Status(429, _)) => {
                return Err(
                    "RetroAchievements asked us to slow down; wait a minute and \
                     run again (what's fetched so far is kept)."
                        .into(),
                )
            }
            Err(e) => {
                // Never print the key, which is part of the URL in the error.
                let e = e.to_string().replace(&self.key, "…");
                // Offline: an old copy is better than nothing.
                if let Some(body) = read(&cached) {
                    eprintln!("ra-check: using an older copy, RetroAchievements: {e}");
                    return Ok(body);
                }
                return Err(format!("RetroAchievements: {e}"));
            }
        };
        let body: Value = serde_json::from_str(&text).map_err(|_| {
            format!(
                "RetroAchievements sent something unexpected: {}",
                short(&text)
            )
        })?;
        if let Some(error) = field(&body, "Error").and_then(|e| e.as_str()) {
            return Err(format!("RetroAchievements: {error}"));
        }
        let _ = std::fs::create_dir_all(&self.cache);
        let _ = std::fs::write(&cached, &text);
        Ok(body)
    }
}

fn user_agent() -> String {
    format!(
        "ra-check/{} (+https://github.com/machomano11/slot) rcheevos/{}",
        env!("CARGO_PKG_VERSION"),
        slot_cheevos::version()
    )
}

fn fresh(path: &PathBuf) -> Option<Value> {
    let age = std::fs::metadata(path).ok()?.modified().ok()?;
    if SystemTime::now().duration_since(age).unwrap_or_default() > FRESH {
        return None;
    }
    read(path)
}

fn read(path: &PathBuf) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// The field by name, either casing: the docs show both "Title" and "title".
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    object.get(name).or_else(|| {
        object
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    })
}

fn number(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn unexpected(body: &Value) -> String {
    format!(
        "RetroAchievements sent something unexpected: {}",
        short(&body.to_string())
    )
}

fn short(text: &str) -> String {
    text.chars().take(200).collect()
}

/// Where the fetched lists are kept: ~/Library/Caches/ra-check on a Mac.
fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(dir).join("ra-check");
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/ra-check")
    } else {
        home.join(".cache/ra-check")
    }
}

/// Where the API key is kept once given: ~/Library/Application Support/ra-check on a Mac.
pub fn key_file() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("ra-check/api-key");
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/ra-check/api-key")
    } else {
        home.join(".config/ra-check/api-key")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_in_either_casing() {
        let pascal: Value = serde_json::from_str(r#"{"NumAchievements": 44, "ID": "12"}"#).unwrap();
        let camel: Value = serde_json::from_str(r#"{"numAchievements": 44}"#).unwrap();
        assert_eq!(number(field(&pascal, "NumAchievements")), 44);
        assert_eq!(number(field(&camel, "NumAchievements")), 44);
        assert_eq!(number(field(&pascal, "ID")), 12);
    }
}
