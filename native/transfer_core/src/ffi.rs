use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr,
    sync::{Mutex, OnceLock},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    core::{CoreConfig, SourceHandle, TransferCore},
    model::TransferCommand,
};

static ENGINE: OnceLock<Mutex<Option<FfiEngine>>> = OnceLock::new();

/// 写文件的日志器：部分厂商（如 vivo）通过 `log.tag` 系统属性抑制第三方应用的
/// logcat 日志，文件日志可绕开该限制，便于真机诊断。
struct FileLogger {
    file: std::sync::Mutex<std::fs::File>,
}

impl log::Log for FileLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        use std::io::Write;
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        use std::io::Write;
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// 初始化文件日志：优先写文件；Android 上失败则回退 logcat。
fn init_file_logger(log_directory: Option<&std::path::Path>) {
    if let Some(dir) = log_directory {
        let path = dir.join("transassist.log");
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let logger: &'static FileLogger =
                Box::leak(Box::new(FileLogger {
                    file: std::sync::Mutex::new(file),
                }));
            let _ = log::set_logger(logger);
            let _ = log::set_max_level(log::LevelFilter::Info);
            return;
        }
    }
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("transassist"),
    );
}

struct FfiEngine {
    core: TransferCore,
    config: CoreConfig,
}

#[derive(Deserialize)]
struct InitializeRequest {
    settings: SettingsRequest,
    data_directory: PathBuf,
    #[serde(default)]
    identity_wrap_key: Option<String>,
    // 仅 Android 用于文件日志，桌面端暂不使用。
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    #[serde(default)]
    log_directory: Option<PathBuf>,
}

#[derive(Deserialize)]
struct SettingsRequest {
    device_name: String,
    receive_directory: PathBuf,
    background_receive: bool,
    auto_accept_trusted: bool,
    #[serde(default = "default_theme_mode")]
    theme_mode: String,
}

fn default_theme_mode() -> String {
    "system".to_owned()
}

/// Initializes the process-wide transfer core.
///
/// # Safety
/// `request` must point to a valid, NUL-terminated UTF-8 string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn transassist_initialize(request: *const c_char) -> *mut c_char {
    ffi_boundary(|| {
        let request: InitializeRequest = decode_request(request)?;
        // 日志优先写文件（跨平台），目录缺失时回退到数据目录。
        let log_dir = request
            .log_directory
            .as_deref()
            .or(Some(request.data_directory.as_path()));
        init_file_logger(log_dir);
        let config = CoreConfig {
            data_directory: request.data_directory,
            device_name: request.settings.device_name,
            receive_directory: request.settings.receive_directory,
            background_receive: request.settings.background_receive,
            auto_accept_trusted: request.settings.auto_accept_trusted,
            theme_mode: request.settings.theme_mode,
            identity_wrap_key: request
                .identity_wrap_key
                .map(|encoded| {
                    BASE64
                        .decode(encoded)
                        .map_err(|error| format!("身份包装密钥无效: {error}"))
                })
                .transpose()?,
        };
        let core = TransferCore::initialize(config.clone()).map_err(|error| error.to_string())?;
        let mut slot = engine_slot()
            .lock()
            .map_err(|_| "FFI 核心锁已损坏".to_owned())?;
        if let Some(previous) = slot.take() {
            previous
                .core
                .shutdown()
                .map_err(|error| error.to_string())?;
        }
        *slot = Some(FfiEngine { core, config });
        Ok(json!({"ok": true}))
    })
}

/// Invokes one command on the process-wide transfer core.
///
/// # Safety
/// `request` must point to a valid, NUL-terminated UTF-8 string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn transassist_invoke(request: *const c_char) -> *mut c_char {
    ffi_boundary(|| {
        let request: Value = decode_request(request)?;
        let command = request
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| "命令缺少 command 字段".to_owned())?;
        let mut slot = engine_slot()
            .lock()
            .map_err(|_| "FFI 核心锁已损坏".to_owned())?;
        let engine = slot
            .as_mut()
            .ok_or_else(|| "传输内核尚未初始化".to_owned())?;

        let response = match command {
            "refresh_peers" => {
                engine
                    .core
                    .refresh_peers()
                    .map_err(|error| error.to_string())?;
                json!({"ok": true})
            }
            "connect_address" => {
                let address = required_string(&request, "address")?;
                let peer_id = engine
                    .core
                    .connect_address(address)
                    .map_err(|error| error.to_string())?;
                json!({"ok": true, "peer_id": peer_id})
            }
            "send" => {
                let peer_id = required_string(&request, "peer_id")?;
                let sources: Vec<SourceHandle> = serde_json::from_value(
                    request
                        .get("sources")
                        .cloned()
                        .ok_or_else(|| "发送命令缺少 sources".to_owned())?,
                )
                .map_err(|error| format!("sources 无法解析: {error}"))?;
                let transfer_id = engine
                    .core
                    .send(peer_id, sources)
                    .map_err(|error| error.to_string())?;
                json!({"ok": true, "transfer_id": transfer_id})
            }
            "answer_offer" => {
                let offer_id = parse_uuid(&request, "offer_id")?;
                let accept = required_bool(&request, "accept")?;
                let remember = required_bool(&request, "remember_peer")?;
                engine
                    .core
                    .answer_offer(offer_id, accept, remember)
                    .map_err(|error| error.to_string())?;
                json!({"ok": true})
            }
            "command_transfer" => {
                let transfer_id = parse_uuid(&request, "transfer_id")?;
                let action = match required_string(&request, "action")? {
                    "pause" => TransferCommand::Pause,
                    "resume" => TransferCommand::Resume,
                    "cancel" => TransferCommand::Cancel,
                    "retry" => TransferCommand::Retry,
                    other => return Err(format!("未知任务操作: {other}")),
                };
                engine
                    .core
                    .command_transfer(transfer_id, action)
                    .map_err(|error| error.to_string())?;
                json!({"ok": true})
            }
            "update_settings" => {
                let settings: SettingsRequest = serde_json::from_value(
                    request
                        .get("settings")
                        .cloned()
                        .ok_or_else(|| "设置命令缺少 settings".to_owned())?,
                )
                .map_err(|error| format!("settings 无法解析: {error}"))?;
                engine.config.device_name = settings.device_name;
                engine.config.receive_directory = settings.receive_directory;
                engine.config.background_receive = settings.background_receive;
                engine.config.auto_accept_trusted = settings.auto_accept_trusted;
                engine.config.theme_mode = settings.theme_mode;
                engine
                    .core
                    .update_settings(engine.config.clone())
                    .map_err(|error| error.to_string())?;
                json!({"ok": true})
            }
            "shutdown" => {
                engine.core.shutdown().map_err(|error| error.to_string())?;
                *slot = None;
                json!({"ok": true})
            }
            "remove_trusted_peer" => {
                let peer_id = required_string(&request, "peer_id")?;
                let removed = engine
                    .core
                    .remove_trusted_peer(peer_id)
                    .map_err(|error| error.to_string())?;
                json!({"ok": true, "removed": removed})
            }
            other => return Err(format!("未知命令: {other}")),
        };
        Ok(response)
    })
}

/// Returns the next serialized core event, or null when the queue is empty.
#[unsafe(no_mangle)]
pub extern "C" fn transassist_poll_event() -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let slot = engine_slot().lock().ok()?;
        let event = slot.as_ref()?.core.next_event()?;
        serde_json::to_value(event).ok()
    }));
    match result {
        Ok(Some(event)) => into_c_string(event),
        _ => ptr::null_mut(),
    }
}

/// Releases a string returned by another `transassist_*` function.
///
/// # Safety
/// `value` must be null or a pointer returned by this library that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn transassist_free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: The caller contract requires a pointer returned by CString::into_raw here.
        unsafe { drop(CString::from_raw(value)) };
    }
}

fn engine_slot() -> &'static Mutex<Option<FfiEngine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

fn ffi_boundary(operation: impl FnOnce() -> Result<Value, String>) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => into_c_string(value),
        Ok(Err(error)) => into_c_string(json!({"ok": false, "error": error})),
        Err(_) => into_c_string(json!({"ok": false, "error": "传输内核发生内部错误"})),
    }
}

fn into_c_string(value: Value) -> *mut c_char {
    let encoded = serde_json::to_string(&value)
        .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"响应编码失败\"}".to_owned());
    CString::new(encoded)
        .expect("JSON strings cannot contain NUL bytes")
        .into_raw()
}

fn decode_request<T: for<'de> Deserialize<'de>>(request: *const c_char) -> Result<T, String> {
    if request.is_null() {
        return Err("请求指针不能为空".to_owned());
    }
    // SAFETY: Exported functions document that callers must pass a valid NUL-terminated string.
    let bytes = unsafe { CStr::from_ptr(request) }.to_bytes();
    let text = std::str::from_utf8(bytes).map_err(|error| format!("请求不是 UTF-8: {error}"))?;
    serde_json::from_str(text).map_err(|error| format!("请求 JSON 无法解析: {error}"))
}

fn required_string<'a>(request: &'a Value, field: &str) -> Result<&'a str, String> {
    request
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("命令缺少字符串字段 {field}"))
}

fn required_bool(request: &Value, field: &str) -> Result<bool, String> {
    request
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("命令缺少布尔字段 {field}"))
}

fn parse_uuid(request: &Value, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(required_string(request, field)?)
        .map_err(|error| format!("{field} 不是有效 UUID: {error}"))
}

#[allow(dead_code)]
fn _reserved_for_forward_compatible_arguments(_: HashMap<String, Value>) {}
