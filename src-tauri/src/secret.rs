//! 凭据存储抽象（SPEC §11）。
//!
//! Windows → 系统 keyring；Android → Keystore + AES-GCM（android_secret.rs，
//! 经 JNI 调 Kotlin 助手 TokenStore；keyring 在 Android 上只会回退到内存
//! mock，重启丢令牌，故 Android 不走 keyring）。Linux 开发机上 keyring 后端可能不可用（无 gnome-keyring
//! 之类的守护进程），此时回退到应用数据目录下权限 0600 的文件，
//! 并记一条警告——生产 Windows 路径不受影响。令牌永不打印。

use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
const SERVICE: &str = "todo-app";
#[cfg(not(target_os = "android"))]
const ACCOUNT: &str = "sync_token";

pub trait SecretStore: Send + Sync {
    fn get_token(&self) -> Result<Option<String>, String>;
    fn set_token(&self, token: &str) -> Result<(), String>;
    fn delete_token(&self) -> Result<(), String>;
    fn has_token(&self) -> bool {
        self.get_token()
            .map(|t| t.map(|t| !t.is_empty()).unwrap_or(false))
            .unwrap_or(false)
    }
}

#[cfg(not(target_os = "android"))]
pub struct KeyringStore;

#[cfg(not(target_os = "android"))]
impl SecretStore for KeyringStore {
    fn get_token(&self) -> Result<Option<String>, String> {
        match keyring::Entry::new(SERVICE, ACCOUNT) {
            Ok(entry) => match entry.get_password() {
                Ok(pw) => Ok(Some(pw)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        }
    }

    fn set_token(&self, token: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())?;
        entry.set_password(token).map_err(|e| e.to_string())
    }

    fn delete_token(&self) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// 仅当系统 keyring 不可用时使用的回退存储（Linux 开发机）。
/// 文件权限 0600；这不是长期方案，Windows / Android 走系统凭据库。
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            path: data_dir.join("secrets").join("sync_token"),
        }
    }
}

impl SecretStore for FileStore {
    fn get_token(&self) -> Result<Option<String>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set_token(&self, token: &str) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, token).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn delete_token(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// 先试系统 keyring，失败再回退文件存储；回退时记警告（不含凭据内容）。
/// Android 上 keyring 会回退到内存 mock（重启丢令牌），所以主存储改用
/// Keystore（见 android_secret.rs）；桌面端继续用系统 keyring。
pub struct SecretStoreImpl {
    #[cfg(target_os = "android")]
    primary: crate::android_secret::AndroidKeystoreStore,
    #[cfg(not(target_os = "android"))]
    primary: KeyringStore,
    fallback: FileStore,
}

impl SecretStoreImpl {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            #[cfg(target_os = "android")]
            primary: crate::android_secret::AndroidKeystoreStore,
            #[cfg(not(target_os = "android"))]
            primary: KeyringStore,
            fallback: FileStore::new(data_dir),
        }
    }
}

impl SecretStore for SecretStoreImpl {
    fn get_token(&self) -> Result<Option<String>, String> {
        // 主存储返回"无条目"时也要看回退文件：Linux 开发机上 set_token 走回退文件
        // 存过令牌，而 keyring 的 get_password 此时返回 NoEntry（不是 Err），
        // 只在 Err 时回退会让文件里的令牌永远读不到。
        match self.primary.get_token() {
            Ok(Some(token)) => Ok(Some(token)),
            _ => self.fallback.get_token(),
        }
    }

    fn set_token(&self, token: &str) -> Result<(), String> {
        match self.primary.set_token(token) {
            Ok(()) => Ok(()),
            Err(_) => {
                eprintln!("[todo] 系统 keyring 不可用，令牌改存应用数据目录（0600）");
                self.fallback.set_token(token)
            }
        }
    }

    fn delete_token(&self) -> Result<(), String> {
        let _ = self.primary.delete_token();
        self.fallback.delete_token()
    }
}
