//! Telling which RetroAchievements game a file is meant to be when its hash is not one
//! RetroAchievements knows: by its title, the way No-Intro and Redump name files.
use unicode_normalization::UnicodeNormalization;

/// A title reduced to what both sides agree on: no region or version tags, accents, "The",
/// punctuation or case. "Legend of Zelda, The - A Link to the Past (USA)" and
/// "Legend of Zelda, The: A Link to the Past" both give "legendofzeldaalinktopast".
pub fn key(title: &str) -> String {
    let mut plain = String::new();
    let mut depth = 0usize;
    for c in title.nfd() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '&' => plain.push_str(" and "),
            c if c.is_alphanumeric() && c.is_ascii() => plain.push(c.to_ascii_lowercase()),
            c if c.is_alphanumeric() => {}
            // Accents come apart into marks under NFD; dropping them keeps the letter joined.
            c if unicode_mark(c) => {}
            _ => plain.push(' '),
        }
    }
    plain
        .split_whitespace()
        .filter(|w| *w != "the")
        .collect::<Vec<_>>()
        .concat()
}

fn unicode_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// The keys an RetroAchievements title answers to: "~Prototype~ X" also as "X", and each
/// side of "Mega Man | Rockman". None for subsets, which share their game's name.
pub fn title_keys(title: &str) -> Vec<String> {
    let mut title = title.trim();
    if title.contains("[Subset") {
        return Vec::new();
    }
    while let Some(rest) = title.strip_prefix('~') {
        match rest.find('~') {
            Some(end) => title = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    title
        .split(" | ")
        .map(key)
        .filter(|k| !k.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_intro_names_meet_ra_titles() {
        assert_eq!(
            key("Legend of Zelda, The - A Link to the Past (USA)"),
            key("Legend of Zelda, The: A Link to the Past")
        );
        assert_eq!(
            key("Pokemon - Emerald Version (USA, Europe)"),
            key("Pokémon Emerald Version")
        );
        assert_eq!(
            key("Final Fantasy VII (USA) (Disc 1)"),
            key("Final Fantasy VII")
        );
        assert_eq!(key("Sonic & Knuckles (World)"), key("Sonic and Knuckles"));
    }

    #[test]
    fn ra_title_forms() {
        assert_eq!(
            title_keys("~Prototype~ Star Fox 2"),
            vec![key("Star Fox 2")]
        );
        assert_eq!(
            title_keys("Mega Man | Rockman"),
            vec![key("Mega Man"), key("Rockman")]
        );
        assert!(title_keys("Super Mario Bros. [Subset - Bonus]").is_empty());
    }
}
