//! Keys belong to the current Windows user and configuration profile.
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{Foundation::ERROR_NOT_FOUND, Security::Credentials::*},
};

static REVISION: AtomicU64 = AtomicU64::new(0);
pub fn revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}
fn target(path: &Path) -> String {
    format!(
        "EnglishCompanionLite/DeepSeek/{}",
        path.to_string_lossy().to_lowercase()
    )
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub fn validate_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty()
        || key.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize
        || !key.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err("API Key 格式无效，请粘贴完整密钥，不要包含空格或换行。");
    }
    Ok(())
}
struct Credential(*mut CREDENTIALW);
impl Drop for Credential {
    fn drop(&mut self) {
        unsafe {
            CredFree(self.0.cast());
        }
    }
}
pub fn load(path: &Path) -> Result<Option<String>, &'static str> {
    let name = wide(&target(path));
    let mut raw = std::ptr::null_mut();
    match unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, 0, &mut raw) } {
        Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
        Err(_) => return Err("无法读取已保存的 API Key，请检查 Windows 凭据管理器。"),
        Ok(()) => {}
    }
    let credential = Credential(raw);
    if credential.0.is_null() {
        return Err("已保存的 API Key 无效，请重新填写并保存。");
    }
    let entry = unsafe { &*credential.0 };
    if entry.CredentialBlob.is_null()
        || entry.CredentialBlobSize == 0
        || entry.CredentialBlobSize > CRED_MAX_CREDENTIAL_BLOB_SIZE
    {
        return Err("已保存的 API Key 无效，请重新填写并保存。");
    }
    let blob = unsafe {
        std::slice::from_raw_parts(entry.CredentialBlob, entry.CredentialBlobSize as usize)
    };
    let key = std::str::from_utf8(blob).map_err(|_| "已保存的 API Key 无效，请重新填写并保存。")?;
    validate_key(key)?;
    Ok(Some(key.to_owned()))
}
pub fn save(path: &Path, key: &str) -> Result<(), &'static str> {
    validate_key(key)?;
    let mut name = wide(&target(path));
    let mut username = wide("DeepSeek API Key");
    let mut blob = key.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };
    let result = unsafe { CredWriteW(&credential, 0) };
    for byte in &mut blob {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    result.map_err(|_| "API Key 保存失败，请检查 Windows 凭据管理器后重试。")?;
    REVISION.fetch_add(1, Ordering::AcqRel);
    Ok(())
}
pub fn remove(path: &Path) -> Result<(), &'static str> {
    let name = wide(&target(path));
    match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, 0) } {
        Err(error) if error.code() != ERROR_NOT_FOUND.to_hresult() => {
            return Err("无法恢复 API Key，请检查 Windows 凭据管理器。");
        }
        _ => {}
    }
    REVISION.fetch_add(1, Ordering::AcqRel);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_keys_are_rejected_without_echoing_secrets() {
        for key in [
            "",
            "sk-secret\r\nInjected: value",
            "sk secret",
            "密钥",
            &"x".repeat(2561),
        ] {
            let error = validate_key(key).unwrap_err();
            assert!(!error.contains("sk-secret"));
        }
        assert!(validate_key("sk-synthetic-not-a-real-key").is_ok());
        assert_ne!(
            target(Path::new("C:/profiles/a/config.toml")),
            target(Path::new("C:/profiles/b/config.toml"))
        );
    }
    #[test]
    #[ignore = "Writes synthetic credentials in a unique test profile; requires a Windows credential session"]
    fn credentials_roundtrip_in_isolated_profile() {
        let profile = std::env::current_dir()
            .unwrap()
            .join("artifacts")
            .join(format!(
                "credential-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("config.toml");
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = remove(&self.0);
            }
        }
        let _cleanup = Cleanup(profile.clone());
        assert!(load(&profile).unwrap().is_none());
        save(&profile, "sk-synthetic-first").unwrap();
        assert!(load(&profile).unwrap().as_deref() == Some("sk-synthetic-first"));
        save(&profile, "sk-synthetic-replacement").unwrap();
        assert!(load(&profile).unwrap().as_deref() == Some("sk-synthetic-replacement"));
        remove(&profile).unwrap();
        assert!(load(&profile).unwrap().is_none());
    }
}
