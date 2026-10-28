# English Companion Lite

[简体中文](README.md) · [Changelog](CHANGELOG.md) · [Build guide](docs/BUILD.md) · [Validation](TESTING.md)

A small Windows companion that shows English learning hints after newly typed Chinese settles in an explicitly authorized editor. It does not replace, insert, or modify the original text. Keep using Microsoft Pinyin.

**Source version 0.1.15 — early preview. Windows 10/11 x64.** Compatibility depends on the target application's Windows UI Automation implementation.

## Features

- DeepSeek non-thinking translation, MyMemory without an API key, or Azure Translator, selected explicitly by the user. New configurations select DeepSeek; existing configurations retain their provider. No automatic provider switching.
- Clearly labeled offline Mock mode for testing detection and display; it does not produce translations.
- Explicit Notepad authorization and additional application authorization by full EXE path.
- Native tray and four-page settings, keyboard navigation, DPI scaling, save feedback, and diagnostic export.
- Select and copy hint text with Ctrl+C, the context menu, or Copy All. Hovering or interacting pauses dismissal.
- Configurable sentence/idle delay, minimum Chinese character count, and display duration.
- A bounded in-memory cache; a single EXE without a Python or Rust runtime or embedded browser.

## Download and run

Repository: [zsp1029/english-companion-lite](https://github.com/zsp1029/english-companion-lite). First public preview: [v0.1.14](https://github.com/zsp1029/english-companion-lite/releases/tag/v0.1.14).

[Download the Windows x64 ZIP](https://github.com/zsp1029/english-companion-lite/releases/download/v0.1.15/english-companion-lite-v0.1.15-windows-x64.zip), extract it, and run `english-companion-lite.exe`. [SHA-256 checksums](https://github.com/zsp1029/english-companion-lite/releases/download/v0.1.15/SHA256SUMS.txt) are provided with the release. The unsigned executable may trigger SmartScreen. Check the source and compare `Get-FileHash` with `SHA256SUMS.txt`; a checksum does not replace a trusted source or signing.

The first launch creates `%LOCALAPPDATA%\EnglishCompanionLite\config.toml` and opens settings. Capture, Notepad authorization, and networking are disabled by default. Close settings to keep the tray running; use the tray Exit command to quit. Exit the old version and back up your configuration before upgrading. There is no installer or automatic updater. Removing the extracted directory leaves your user configuration intact.

## Quick start

1. Enable Mock mode and leave networking disabled.
2. Authorize Microsoft Notepad, enable automatic translation, and save/apply.
3. Open a new Notepad document and type `今天工作比较忙，晚上想早点休息。` with Microsoft Pinyin.
4. A hint marked `[MOCK / 测试模式]` should appear after input settles. It normally disappears after four seconds, or when typing resumes.
5. Check diagnostics if nothing appears. Unsupported TextPattern/TextEditPattern interfaces cause the editor to be skipped. Do not elevate the application to bypass restrictions.

For DeepSeek translation, select DeepSeek in settings, enter your API key in the masked field, and save/apply. It takes effect immediately without a restart. The key is saved in Windows Credential Manager for the current user and configuration profile; leave the field blank to keep it or enter a new value to replace it. Disable Mock, authorize an application, and allow networking to translate. The fixed official HTTPS endpoint uses `deepseek-flash` with thinking disabled. Only the current sentence is sent and one natural English translation is returned. API usage is billed separately; the key is not stored in TOML, URLs, or diagnostics. `DEEPSEEK_API_KEY` remains a fallback when no key is saved. See [DeepSeek setup](docs/DEEPSEEK.md).

For free translation, select MyMemory, disable Mock, allow networking, and save. The optional contact email is sent to the provider if supplied. Requests are subject to the provider's [API rules](https://mymemory.translated.net/doc/spec.php) and [quotas](https://mymemory.translated.net/doc/usagelimits.php). Sentences over 500 UTF-8 bytes are skipped without splitting. Quota exhaustion causes a one-hour cooldown, rate limiting 60 seconds, and network/service errors 10 seconds.

Azure requires `AZURE_TRANSLATOR_KEY` and, when appropriate, `AZURE_TRANSLATOR_REGION`. Configure the endpoint in TOML if needed. See [Azure setup](docs/AZURE.md). Azure live translation has not been validated with a working key.

## Privacy and compatibility

Online translation sends triggered Chinese text to the selected third party. DeepSeek receives only the current sentence in an HTTPS POST, without chat history, window titles, or unrelated application text. MyMemory includes text and the optional email in HTTPS URL query parameters. The app does not call the translation-contribution `/set` endpoint, but provider logging and data use remain subject to its terms. Do not use sensitive conversations or confidential documents.

Authorization covers qualifying editors throughout the authorized process. Password protection relies on correctly reported accessibility properties; other sensitive fields cannot always be identified. There are no keyboard hooks or clipboard polling. The cache holds up to 128 entries in memory and is cleared on exit. Diagnostics avoid input text but may contain process paths and application metadata; review them before sharing. Pausing prevents new collection/display, while in-flight calls must return and sent requests cannot be recalled.

QQ compatibility varies across machines. The program needs the target application to expose the required accessibility interfaces. Existing text is baselined on focus; merely moving the caret does not trigger translation.

## Configuration

Use settings or edit the user TOML and choose Reload Configuration from the tray. Never commit the actual user configuration.

| Setting | Default | Allowed range |
| --- | --- | --- |
| Sentence delay | 300 ms | 200–5000 ms |
| Idle delay | 1500 ms | 500–15000 ms |
| Display time | 4000 ms | 500–30000 ms |
| Minimum Chinese characters | 6 | 1–300 |

Global limit: 300 Unicode characters per sentence; provider limits can be smaller. If idle requests consume too much quota, try 3000–5000 ms. Extending a sentence can trigger another request.

## Build from source

Install Rust Stable and the MSVC C++ desktop workload plus Windows SDK from Visual Studio Build Tools on Windows x64. The declared minimum Rust version is 1.85, but the minimum itself has not been tested separately.

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\english-companion-lite.exe
```

Dependencies are declared in `Cargo.toml` and locked in `Cargo.lock`. No Node.js, Python, or separate resource compiler is required to build. The current machine validates the GNU x64 toolchain; MSVC is still unverified. See [build and packaging](docs/BUILD.md).

## Source layout

| Path | Purpose |
| --- | --- |
| `src/` | UIA, translation worker, settings, tray, overlay, cache |
| `assets/` | Editable SVG, ICO, Windows manifest |
| `tests/` | Configuration and sentence behavior tests |
| `examples/` | Controlled diagnostic/translation probes |
| `scripts/` | Development checks, desktop tests, packaging |
| `docs/` | Build, Azure setup, release notes |
| `third-party/` | Dependency licenses and notices |
| `build.rs` | Embedded icon and manifest resources |
| `config.example.toml` | Safe configuration example |

Local toolchains, build outputs, historical test records and release ZIPs are excluded from Git.

## Troubleshooting and feedback

Start with Notepad and offline Mock mode. Confirm authorization, enablement, saved settings, at least six Chinese characters, and finished composition. Mock is not translation. Live requests require networking and the correct provider configuration. Use tray Exit to stop the application.

For bug reports, include Windows/application versions, steps, and sanitized diagnostics. Never attach keys, real chats, personal configuration, or screenshots containing private information. See [security reporting](SECURITY.md) and [validation limits](TESTING.md). Genuine screenshots can be added later using only sample text and sanitized settings.

## License

[MIT](LICENSE), Copyright © 2026 zsp1029. Dependency and toolchain notices are retained in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Translation services have their own terms. Windows system components are not redistributed.

