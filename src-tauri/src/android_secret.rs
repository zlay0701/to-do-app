//! Android 凭据存储（SPEC §11）：AndroidKeyStore + AES-256-GCM。
//!
//! 密钥在系统 Keystore 里，令牌经 AES-GCM 加密后才落盘到应用私有目录，
//! 明文永不写文件。加密/解密在 Kotlin 助手类 `TokenStore`
//!（`src-tauri/gen/android/.../TokenStore.kt`）里做，Rust 侧经 JNI 调它。
//!
//! JVM 指针不用链接期符号：运行时用 `dlsym` 找 `JNI_GetCreatedJavaVMs`
//!（找不到时回退文件存储并记警告——应用私有目录本身已按应用隔离，
//! 且回退路径只在 JVM 不可达的异常环境出现）。`jni` 已是 Tauri 的
//! 传递依赖（见 Cargo.lock），这里只是直接引用，不引入新 crate。

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use crate::secret::SecretStore;

const TOKEN_STORE_CLASS: &str = "com/example/todo/TokenStore";

// libdl 在 Android 上一定存在，显式链接，避免启动期符号解析失败。
#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_NOW: c_int = 2;
const RTLD_GLOBAL: c_int = 0x100;

type GetCreatedJavaVMsFn = unsafe extern "C" fn(
    *mut *mut jni::sys::JavaVM,
    jni::sys::jsize,
    *mut jni::sys::jsize,
) -> jni::sys::jint;

/// 进程内唯一的 JavaVM 地址（存 usize：裸指针类型不满足 Sync，整数地址可以；
/// 每次调用现包成 JavaVM，JavaVM 本身不可 Clone，故不直接存它）。
fn java_vm() -> Option<JavaVM> {
    static VM_PTR: OnceLock<Option<usize>> = OnceLock::new();
    let addr = (*VM_PTR.get_or_init(|| unsafe {
        let handle = dlopen(std::ptr::null(), RTLD_NOW | RTLD_GLOBAL);
        if handle.is_null() {
            return None;
        }
        let sym = dlsym(handle, c"JNI_GetCreatedJavaVMs".as_ptr());
        if sym.is_null() {
            return None;
        }
        let get_vms: GetCreatedJavaVMsFn = std::mem::transmute(sym);
        let mut vms: [*mut jni::sys::JavaVM; 1] = [std::ptr::null_mut()];
        let mut count: jni::sys::jsize = 0;
        if get_vms(vms.as_mut_ptr(), 1, &mut count) != 0 || count < 1 || vms[0].is_null() {
            return None;
        }
        Some(vms[0] as usize)
    }))?;
    unsafe { JavaVM::from_raw(addr as *mut jni::sys::JavaVM).ok() }
}

/// 拿到应用 Context（ActivityThread.currentApplication()）。
fn app_context<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
    let thread = env.find_class("android/app/ActivityThread")?;
    let app = env
        .call_static_method(
            thread,
            "currentApplication",
            "()Landroid/app/Application;",
            &[],
        )?
        .l()?;
    env.call_method(
        &app,
        "getApplicationContext",
        "()Landroid/content/Context;",
        &[],
    )?
    .l()
}

/// 在当前线程 attach 到 JVM 上执行一次 JNI 调用；Java 侧抛异常时先清掉再返回错误。
fn with_jni<T>(
    f: impl FnOnce(&mut JNIEnv, JObject) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let vm = java_vm().ok_or_else(|| "无法获取 JVM，Keystore 不可用".to_string())?;
    let mut guard = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let env: &mut JNIEnv = &mut guard;
    let result = (|| {
        let context = app_context(env)?;
        f(env, context)
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let _ = env.exception_clear();
            Err(e.to_string())
        }
    }
}

/// AndroidKeyStore + AES-GCM 的令牌存储；JVM 不可达时调用方回退文件存储。
pub struct AndroidKeystoreStore;

impl SecretStore for AndroidKeystoreStore {
    fn get_token(&self) -> Result<Option<String>, String> {
        with_jni(|env, context| {
            let ret = env
                .call_static_method(
                    TOKEN_STORE_CLASS,
                    "getToken",
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(&context)],
                )?
                .l()?;
            if ret.is_null() {
                return Ok(None);
            }
            let s: String = env.get_string(&JString::from(ret))?.into();
            Ok(Some(s))
        })
    }

    fn set_token(&self, token: &str) -> Result<(), String> {
        with_jni(|env, context| {
            let token = env.new_string(token)?;
            env.call_static_method(
                TOKEN_STORE_CLASS,
                "setToken",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(&context), JValue::Object(&token)],
            )?
            .v()?;
            Ok(())
        })
    }

    fn delete_token(&self) -> Result<(), String> {
        with_jni(|env, context| {
            env.call_static_method(
                TOKEN_STORE_CLASS,
                "deleteToken",
                "(Landroid/content/Context;)V",
                &[JValue::Object(&context)],
            )?
            .v()?;
            Ok(())
        })
    }
}
