//! Selection-frequency history. Every time an entry is chosen its tally goes
//! up; `sort_entries` then orders the menu most-used-first, with the stable
//! sort keeping never-used entries in their original (alphabetical) order.
//! Because the fuzzy sorter tie-breaks by list position, the boost also
//! decides between equally-scored fuzzy matches.
//!
//! Persisted as plain `count<TAB>name` lines so the file is trivially
//! inspectable and editable. Writes go through a temp file + rename so a
//! crash mid-save can't truncate the history.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub const HISTORY_FILE: &str = "windmenu_history.txt";

/// Entries kept when saving; the long tail of rarely-used entries is dropped
/// once the file grows past this.
const MAX_SAVED_ENTRIES: usize = 500;

pub struct History {
    counts: HashMap<String, u32>,
    path: PathBuf,
}

impl History {
    pub fn load(path: PathBuf) -> History {
        let mut counts = HashMap::new();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                if let Some((count, name)) = line.split_once('\t') {
                    if let (Ok(count), false) = (count.trim().parse::<u32>(), name.is_empty()) {
                        counts.insert(name.to_string(), count);
                    }
                }
            }
        }
        History { counts, path }
    }

    /// Bump the tally for a chosen entry and persist. Save errors are ignored:
    /// history is a convenience and must never break launching.
    pub fn record(&mut self, name: &str) {
        let count = self.counts.entry(name.to_string()).or_insert(0);
        *count = count.saturating_add(1);
        self.save();
    }

    fn save(&self) {
        let mut ranked: Vec<(&String, &u32)> = self.counts.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        ranked.truncate(MAX_SAVED_ENTRIES);
        let body: String = ranked
            .into_iter()
            .map(|(name, count)| format!("{}\t{}\n", count, name))
            .collect();
        let tmp = self.path.with_extension("txt.tmp");
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &self.path);
        }
    }

    /// Stable-sort `entries` by usage count, descending. Entries with no
    /// history keep their relative order.
    pub fn sort_entries(&self, entries: &mut [String]) {
        if self.counts.is_empty() {
            return;
        }
        entries.sort_by_key(|name| std::cmp::Reverse(self.counts.get(name).copied().unwrap_or(0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("windmenu-history-test-{}", name))
    }

    #[test]
    fn unused_entries_keep_alphabetical_order() {
        let mut h = History { counts: HashMap::new(), path: temp_path("noop") };
        h.counts.insert("Notepad".into(), 3);
        h.counts.insert("Firefox".into(), 7);
        let mut entries: Vec<String> =
            ["Calculator", "Firefox", "Notepad", "Paint"].iter().map(|s| s.to_string()).collect();
        h.sort_entries(&mut entries);
        assert_eq!(entries, vec!["Firefox", "Notepad", "Calculator", "Paint"]);
    }

    #[test]
    fn empty_history_leaves_entries_untouched() {
        let h = History { counts: HashMap::new(), path: temp_path("empty") };
        let mut entries: Vec<String> = vec!["b".into(), "a".into()];
        h.sort_entries(&mut entries);
        assert_eq!(entries, vec!["b", "a"]);
    }

    #[test]
    fn record_persists_and_reloads() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        let mut h = History::load(path.clone());
        h.record("Firefox");
        h.record("Firefox");
        h.record("Visual Studio Code");
        let reloaded = History::load(path.clone());
        assert_eq!(reloaded.counts.get("Firefox"), Some(&2));
        assert_eq!(reloaded.counts.len(), 2);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let path = temp_path("malformed");
        fs::write(&path, "3\tGood\nnot a line\n\t\nx\tBad count\n").unwrap();
        let h = History::load(path.clone());
        assert_eq!(h.counts.get("Good"), Some(&3));
        assert_eq!(h.counts.len(), 1);
        let _ = fs::remove_file(&path);
    }
}
