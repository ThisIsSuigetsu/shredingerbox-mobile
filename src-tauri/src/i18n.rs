//! Язык сообщений, которые Rust отдаёт в интерфейс (ошибки и т. п.).
//! Выбирается в настройках; до выбора - по языку системы (решает интерфейс).

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set(lang: &str) {
    ENGLISH.store(lang == "en", Ordering::Relaxed);
}

pub fn en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// `tr!("русский {x}", "English {x}")` - строка на текущем языке.
#[macro_export]
macro_rules! tr {
    ($ru:literal, $en:literal) => {
        if $crate::i18n::en() {
            format!($en)
        } else {
            format!($ru)
        }
    };
}
