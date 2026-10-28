use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Azure,
    Deepseek,
    #[serde(rename = "mymemory")]
    Mymemory,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Azure => "Microsoft Azure Translator",
            Self::Deepseek => "DeepSeek（非推理翻译）",
            Self::Mymemory => "MyMemory（免费，免 Key）",
        }
    }
}
// Configurations written before service selection always mean Azure.
fn legacy_provider() -> Provider {
    Provider::Azure
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub allow_notepad: bool,
    pub allow_network: bool,
    pub mock_mode: bool,
    #[serde(default = "legacy_provider")]
    pub provider: Provider,
    pub mymemory_email: String,
    /// Explicit full executable paths. Window titles and bare process names are never trusted.
    pub allowed_executables: Vec<String>,
    pub punctuation_delay_ms: u64,
    pub idle_delay_ms: u64,
    pub display_ms: u32,
    pub min_han: usize,
    pub endpoint: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_notepad: false,
            allow_network: false,
            mock_mode: false,
            provider: Provider::Deepseek,
            mymemory_email: String::new(),
            allowed_executables: vec![],
            punctuation_delay_ms: 300,
            idle_delay_ms: 1500,
            display_ms: 4000,
            min_han: 6,
            endpoint: "https://api.cognitive.microsofttranslator.com".into(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(200..=5000).contains(&self.punctuation_delay_ms)
            || !(500..=15000).contains(&self.idle_delay_ms)
            || !(500..=30000).contains(&self.display_ms)
            || !(1..=300).contains(&self.min_han)
        {
            return Err("延迟或字数超出允许范围（详见配置示例）。".into());
        }
        for path in &self.allowed_executables {
            if !Path::new(path).is_absolute() || !path.to_ascii_lowercase().ends_with(".exe") {
                return Err("白名单必须使用完整的 EXE 绝对路径。".into());
            }
        }
        if self.enabled && !self.allow_notepad && self.allowed_executables.is_empty() {
            return Err("启用前请先授权记事本或填写应用白名单。".into());
        }
        crate::translator::endpoint_parts(&self.endpoint)?;
        let email = &self.mymemory_email;
        if !email.is_empty()
            && (email.len() > 254
                || !email.is_ascii()
                || email
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
                || email.split_once('@').is_none_or(|(local, domain)| {
                    local.is_empty() || !domain.contains('.') || domain.contains('@')
                }))
        {
            return Err("MyMemory 联系邮箱格式无效；不填写也可以免费使用。".into());
        }
        Ok(())
    }
    pub fn path() -> Result<PathBuf, String> {
        let root = std::env::var_os("LOCALAPPDATA").ok_or("无法定位 LocalAppData。")?;
        Ok(PathBuf::from(root)
            .join("EnglishCompanionLite")
            .join("config.toml"))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            let value = Self::default();
            value.save(path)?;
            return Ok(value);
        }
        let raw = fs::read_to_string(path).map_err(|_| "无法读取配置文件。")?;
        // Parse errors can contain source lines: never expose their contents.
        let value: Self = toml::from_str(&raw).map_err(|_| "配置 TOML 无效，请检查格式与字段。")?;
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "无法创建配置目录。")?;
        }
        let raw = toml::to_string_pretty(self).map_err(|_| "无法生成配置。")?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, raw).map_err(|_| "无法写入配置。")?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::{
                core::PCWSTR,
                Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                },
            };
            let a: Vec<_> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(a.as_ptr()),
                    PCWSTR(b.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|_| "无法原子替换配置文件。")?;
        }
        #[cfg(not(windows))]
        fs::rename(tmp, path).map_err(|_| "无法替换配置文件。")?;
        Ok(())
    }
}
