/// Port of `apps/frank-gui/src/i18n.ts`'s frozen English string table. An
/// enum of keys is stronger than the TS string-literal union it replaces:
/// a typo'd key is a compile error here, not a runtime lookup miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    AppName,
    AppLoading,
    AppRefresh,
    AppRefreshSnapshot,
    NavOverview,
    NavPersonas,
    NavIntegrations,
    NavSettings,
}

pub fn t(key: Key) -> &'static str {
    match key {
        Key::AppName => "Frank",
        Key::AppLoading => "Loading Frank…",
        Key::AppRefresh => "Refresh",
        Key::AppRefreshSnapshot => "Refresh snapshot",
        Key::NavOverview => "Overview",
        Key::NavPersonas => "Personas",
        Key::NavIntegrations => "Integrations",
        Key::NavSettings => "Settings",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_keys_return_english_strings() {
        assert_eq!(t(Key::NavOverview), "Overview");
        assert_eq!(t(Key::NavPersonas), "Personas");
        assert_eq!(t(Key::NavIntegrations), "Integrations");
        assert_eq!(t(Key::NavSettings), "Settings");
    }
}
