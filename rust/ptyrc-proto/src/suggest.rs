// SPDX-License-Identifier: GPL-3.0-or-later
//! Nearest known word, for error messages.

/// Edit distance: insertions, deletions, substitutions, transpositions of
/// neighbours.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev2: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                row[j] = row[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = std::mem::replace(&mut prev, row);
    }
    prev[b.len()]
}

/// The candidate closest to `word`, case ignored, when close enough to be
/// a slip: a third of the length, at least one edit. A candidate that
/// `word` starts also counts.
pub fn nearest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let word = word.to_lowercase();
    if word.is_empty() {
        return None;
    }
    let limit = (word.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let lower = candidate.to_lowercase();
        let d = if word.len() >= 2 && lower.starts_with(&word) {
            1
        } else {
            distance(&word, &lower)
        };
        if d <= limit && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, candidate));
        }
    }
    best.map(|(_, c)| c)
}

/// `"; did you mean X?"`, or nothing.
pub fn did_you_mean<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    nearest(word, candidates).map_or_else(String::new, |c| format!("did you mean {c}? "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(distance("enter", "enter"), 0);
        assert_eq!(distance("entre", "enter"), 1);
        assert_eq!(distance("pgdn", "pagedown"), 4);
        assert_eq!(distance("", "ab"), 2);
    }

    #[test]
    fn nearest_word() {
        let keys = ["enter", "esc", "end", "pagedown"];
        assert_eq!(nearest("entr", keys), Some("enter"));
        assert_eq!(nearest("Enter", keys), Some("enter"));
        assert_eq!(nearest("escape", keys), None);
        assert_eq!(nearest("page", keys), Some("pagedown"));
        assert_eq!(nearest("zzz", keys), None);
        assert_eq!(nearest("", keys), None);
    }
}
