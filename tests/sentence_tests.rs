use english_companion_lite::{
    config::Config,
    sentence::{candidate, Trigger},
};
use std::time::{Duration, Instant};

#[test]
fn nearest_sentence_and_unicode_limits() {
    assert_eq!(
        candidate("昨天已经完成。今天工作比较忙，晚上想早点休息。", 6).as_deref(),
        Some("今天工作比较忙，晚上想早点休息。")
    );
    assert_eq!(candidate("hello world", 6), None);
    assert_eq!(candidate("你好。", 6), None);
    assert!(candidate(&"中".repeat(301), 6).is_none());
    assert!(candidate(&"中".repeat(300), 6).is_some());
    assert_eq!(
        candidate("前一句话结束！明天 review 一下这个方案", 6).as_deref(),
        Some("明天 review 一下这个方案")
    );
}
#[test]
fn baseline_debounce_and_dedupe() {
    let c = Config::default();
    let t = Instant::now();
    let mut s = Trigger::default();
    s.baseline("原来已经有的文字。");
    assert_eq!(s.take_due(t + Duration::from_secs(4)), None);
    s.observe("今天工作比较忙。", false, t, &c);
    assert_eq!(s.take_due(t + Duration::from_millis(299)), None);
    assert_eq!(
        s.take_due(t + Duration::from_millis(300)).as_deref(),
        Some("今天工作比较忙。")
    );
    s.observe("今天工作比较忙。", false, t + Duration::from_secs(1), &c);
    assert!(s.deadline().is_none());
}
#[test]
fn idle_and_composition_and_cancellation() {
    let c = Config::default();
    let t = Instant::now();
    let mut s = Trigger::default();
    s.baseline("");
    s.observe("今天晚上早点休息", false, t, &c);
    assert!(s.take_due(t + Duration::from_millis(1499)).is_none());
    s.observe("今天晚上早点休息吧", false, t + Duration::from_secs(1), &c);
    assert!(s.take_due(t + Duration::from_millis(1500)).is_none());
    s.cancel();
    assert!(s.take_due(t + Duration::from_secs(9)).is_none());
    s.observe("今天晚上早点休息。", true, t, &c);
    assert!(s.deadline().is_none());
    s.composition_finished();
    s.observe("今天晚上早点休息。", false, t, &c);
    assert!(s.take_due(t + Duration::from_millis(300)).is_some());
}
#[test]
fn latest_sentence_wins() {
    let c = Config::default();
    let t = Instant::now();
    let mut s = Trigger::default();
    s.baseline("");
    s.observe("今天工作比较忙。", false, t, &c);
    s.observe(
        "今天工作比较忙。晚上想要早点休息。",
        false,
        t + Duration::from_millis(100),
        &c,
    );
    assert!(s.take_due(t + Duration::from_millis(300)).is_none());
    assert_eq!(
        s.take_due(t + Duration::from_millis(400)).as_deref(),
        Some("晚上想要早点休息。")
    );
}
#[test]
fn selection_before_or_after_text_does_not_consume_the_edit() {
    let cfg = Config::default();
    let now = Instant::now();
    let mut s = Trigger::default();
    s.baseline("");
    s.selection_changed("今天晚上早点休息。");
    assert!(s.take_due(now + Duration::from_secs(2)).is_none());
    s.observe("今天晚上早点休息。", false, now, &cfg);
    s.selection_changed("今天晚上早点休息。");
    assert_eq!(
        s.take_due(now + Duration::from_millis(300)).as_deref(),
        Some("今天晚上早点休息。")
    );
    s.observe("今天晚上早点休息。明天继续努力工作。", false, now, &cfg);
    s.selection_changed("今天晚上早点休息。");
    assert!(s.take_due(now + Duration::from_secs(2)).is_none());
}
#[test]
fn composition_cancels_even_when_visible_text_is_unchanged() {
    let cfg = Config::default();
    let now = Instant::now();
    let mut s = Trigger::default();
    s.baseline("");
    s.observe("今天晚上早点休息。", false, now, &cfg);
    s.observe("今天晚上早点休息。", true, now, &cfg);
    assert!(s.take_due(now + Duration::from_secs(2)).is_none());
}
#[test]
fn punctuation_runs_stay_with_the_sentence() {
    assert_eq!(
        candidate("前一句结束。今天晚上早点休息！！", 6).as_deref(),
        Some("今天晚上早点休息！！")
    );
}
#[test]
fn delayed_caret_baseline_never_translates_existing_selection() {
    let cfg = Config::default();
    let now = Instant::now();
    let mut s = Trigger::default();
    // Focus initially reports a selection and produces no readable snapshot.
    // Its later SelectionChanged snapshot must initialize, not detach or translate history.
    assert!(s.establish_if_missing("输入框中原来的内容。"));
    assert!(s.take_due(now + Duration::from_secs(3)).is_none());
    assert!(!s.establish_if_missing("输入框中原来的内容。今天晚上早点休息。"));
    s.observe("输入框中原来的内容。今天晚上早点休息。", false, now, &cfg);
    assert_eq!(
        s.take_due(now + Duration::from_millis(300)).as_deref(),
        Some("今天晚上早点休息。")
    );
}
