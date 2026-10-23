use crate::config::Config;
use std::time::{Duration, Instant};

pub fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x20000..=0x323af | 0xf900..=0xfaff)
}
fn boundary(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '!' | '?' | '\n' | '\r')
}
pub fn candidate(before_caret: &str, min_han: usize) -> Option<String> {
    let text = before_caret.trim();
    // Keep trailing punctuation runs (e.g. "！！") attached to the latest sentence.
    let end = text.trim_end_matches(['。', '！', '？', '!', '?']).len();
    let start = text[..end]
        .char_indices()
        .rev()
        .find(|(_, c)| boundary(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let value = text[start..].trim();
    let count = value.chars().count();
    // An overlong sentence is skipped, never silently sent as a misleading fragment.
    if count == 0 || count > 300 || value.chars().filter(|c| is_han(*c)).count() < min_han {
        return None;
    }
    Some(value.to_owned())
}

#[derive(Default)]
pub struct Trigger {
    baseline: Option<String>,
    pending: Option<(String, Instant)>,
    last_sent: Option<String>,
}
impl Trigger {
    /// Called on focus entry. Existing text is never considered newly typed text.
    pub fn baseline(&mut self, text: &str) {
        self.baseline = Some(text.into());
        self.pending = None;
        self.last_sent = None;
    }
    /// A provider can expose a selection before its caret settles on focus entry.
    /// The first valid snapshot establishes history, never a new translation request.
    pub fn establish_if_missing(&mut self, text: &str) -> bool {
        if self.baseline.is_none() {
            self.baseline(text);
            true
        } else {
            false
        }
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    pub fn matches_baseline(&self, text: &str) -> bool {
        self.baseline.as_deref() == Some(text)
    }
    /// Selection notifications can precede TextChanged. Do not consume its text delta.
    pub fn selection_changed(&mut self, before_caret: &str) {
        if !self.matches_baseline(before_caret) {
            self.cancel();
        }
    }
    pub fn observe(&mut self, text: &str, composing: bool, now: Instant, cfg: &Config) {
        if composing {
            self.baseline = Some(text.into());
            self.pending = None;
            return;
        }
        if self.baseline.as_deref() == Some(text) {
            return;
        }
        self.baseline = Some(text.into());
        self.pending = None;
        if let Some(sentence) = candidate(text, cfg.min_han) {
            if self.last_sent.as_deref() == Some(&sentence) {
                return;
            }
            let delay = if sentence.ends_with(['。', '！', '？']) {
                cfg.punctuation_delay_ms
            } else {
                cfg.idle_delay_ms
            };
            self.pending = Some((sentence, now + Duration::from_millis(delay)));
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|(_, at)| *at)
    }
    pub fn take_due(&mut self, now: Instant) -> Option<String> {
        if self.pending.as_ref().is_some_and(|(_, at)| now >= *at) {
            let (text, _) = self.pending.take()?;
            self.last_sent = Some(text.clone());
            Some(text)
        } else {
            None
        }
    }
    /// Composition completion may leave exactly the same committed string visible.
    pub fn composition_finished(&mut self) {
        self.baseline = None;
    }
}
