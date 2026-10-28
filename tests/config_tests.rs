use english_companion_lite::config::{Config, Provider};
#[test]
fn defaults_do_not_authorize_capture_or_network() {
    let c = Config::default();
    assert!(!c.enabled && !c.allow_network && !c.allow_notepad && !c.mock_mode);
    assert!(c.allowed_executables.is_empty());
    assert!(c.validate().is_ok());
    assert_eq!(c.provider, Provider::Deepseek);
    assert!(c.mymemory_email.is_empty());
}
#[test]
fn old_configs_keep_azure_and_new_service_selection_roundtrips() {
    let old: Config = toml::from_str("enabled = false\nmock_mode = true").unwrap();
    assert_eq!(old.provider, Provider::Azure);
    assert!(old.mock_mode);
    let c = Config {
        provider: Provider::Mymemory,
        mymemory_email: "contact@example.com".into(),
        ..Config::default()
    };
    c.validate().unwrap();
    let decoded: Config = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
    assert_eq!(decoded.provider, Provider::Mymemory);
    assert_eq!(decoded.mymemory_email, c.mymemory_email);
    assert!(toml::from_str::<Config>("provider = 'unknown'").is_err());
    for provider in [Provider::Deepseek, Provider::Azure, Provider::Mymemory] {
        let config = Config {
            provider,
            ..Config::default()
        };
        let saved = toml::to_string(&config).unwrap();
        let loaded: Config = toml::from_str(&saved).unwrap();
        assert_eq!(loaded.provider, provider);
    }
    // Existing explicit selections must not silently send text to a new recipient.
    for (name, provider) in [("mymemory", Provider::Mymemory), ("azure", Provider::Azure)] {
        let old: Config =
            toml::from_str(&format!("provider = '{name}'\nallow_network = true")).unwrap();
        assert_eq!(old.provider, provider);
        assert!(old.allow_network);
    }
    for email in [
        "missing-at",
        "@example.com",
        "a@example.com\r\nX-Header:test",
        "a@b@c.com",
    ] {
        assert!(Config {
            mymemory_email: email.into(),
            ..Config::default()
        }
        .validate()
        .is_err());
    }
}
#[test]
fn enabling_requires_a_whitelist_and_settings_are_bounded() {
    let mut c = Config {
        enabled: true,
        ..Config::default()
    };
    assert!(c.validate().is_err());
    c.allow_notepad = true;
    assert!(c.validate().is_ok());
    c.punctuation_delay_ms = 0;
    assert!(c.validate().is_err());
    c.punctuation_delay_ms = 300;
    c.allowed_executables.push("notepad.exe".into());
    assert!(c.validate().is_err());
}
#[test]
fn shipped_example_roundtrips_without_secrets() {
    let c: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
    c.validate().unwrap();
    assert!(!c.enabled && !c.allow_network && !c.allow_notepad);
    assert_eq!(c.provider, Provider::Deepseek);
    let encoded = toml::to_string(&c).unwrap();
    assert!(!encoded.to_ascii_lowercase().contains("key"));
}
