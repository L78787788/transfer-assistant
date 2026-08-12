use std::sync::OnceLock;

use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JString, JValue},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
static BRIDGE: OnceLock<GlobalRef> = OnceLock::new();

#[derive(Debug, Serialize)]
pub struct TargetRequest<'a> {
    pub id: &'a str,
    pub relative_path: &'a str,
    pub is_directory: bool,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct PreparedTarget {
    pub id: String,
    pub fd: i32,
    pub temporary_uri: String,
    pub final_name: String,
    pub final_path: String,
    pub existed: bool,
    pub random_access: bool,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_transassist_transfer_1assistant_AndroidStorageBridge_nativeRegister(
    mut env: JNIEnv<'_>,
    bridge: JObject<'_>,
) {
    let result = (|| {
        let vm = env.get_java_vm()?;
        let global = env.new_global_ref(bridge)?;
        let _ = JAVA_VM.set(vm);
        let _ = BRIDGE.set(global);
        Ok::<(), jni::errors::Error>(())
    })();
    if result.is_err() {
        let _ = env.throw_new(
            "java/lang/IllegalStateException",
            "无法注册 Rust SAF 存储桥",
        );
    }
}

pub fn prepare_targets(
    tree_uri: &str,
    transfer_id: &str,
    requests: &[TargetRequest<'_>],
) -> Result<Vec<PreparedTarget>, AndroidStorageError> {
    let encoded = serde_json::to_string(requests)?;
    let json: String = with_bridge(|env, bridge| {
        let tree_uri = env.new_string(tree_uri)?;
        let transfer_id = env.new_string(transfer_id)?;
        let encoded = env.new_string(encoded)?;
        let result = env
            .call_method(
                bridge,
                "prepareTargets",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&tree_uri),
                    JValue::Object(&transfer_id),
                    JValue::Object(&encoded),
                ],
            )?
            .l()?;
        let result = JString::from(result);
        let value: String = env.get_string(&result)?.into();
        Ok(value)
    })?;
    serde_json::from_str(&json).map_err(Into::into)
}

pub fn finalize_target(temporary_uri: &str, final_name: &str) -> Result<(), AndroidStorageError> {
    let completed = with_bridge(|env, bridge| {
        let temporary_uri = env.new_string(temporary_uri)?;
        let final_name = env.new_string(final_name)?;
        env.call_method(
            bridge,
            "finalizeTarget",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            &[JValue::Object(&temporary_uri), JValue::Object(&final_name)],
        )?
        .z()
    })?;
    if completed {
        Ok(())
    } else {
        Err(AndroidStorageError::FinalizeFailed)
    }
}

pub fn delete_target(temporary_uri: &str) -> Result<(), AndroidStorageError> {
    let deleted = with_bridge(|env, bridge| {
        let temporary_uri = env.new_string(temporary_uri)?;
        env.call_method(
            bridge,
            "deleteTarget",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&temporary_uri)],
        )?
        .z()
    })?;
    if deleted {
        Ok(())
    } else {
        Err(AndroidStorageError::DeleteFailed)
    }
}

fn with_bridge<T>(
    operation: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, AndroidStorageError> {
    let vm = JAVA_VM.get().ok_or(AndroidStorageError::NotRegistered)?;
    let bridge = BRIDGE.get().ok_or(AndroidStorageError::NotRegistered)?;
    let mut env = vm.attach_current_thread()?;
    operation(&mut env, bridge.as_obj()).map_err(Into::into)
}

#[derive(Debug, Deserialize)]
pub struct PreparedSource {
    pub fd: i32,
    pub random_access: bool,
}

pub fn open_source(uri: &str) -> Result<PreparedSource, AndroidStorageError> {
    let json: String = with_bridge(|env, bridge| {
        let uri = env.new_string(uri)?;
        let result = env
            .call_method(
                bridge,
                "openSource",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&uri)],
            )?
            .l()?;
        let result = JString::from(result);
        let value: String = env.get_string(&result)?.into();
        Ok(value)
    })?;
    serde_json::from_str(&json).map_err(Into::into)
}

pub fn source_revision(uri: &str) -> Result<String, AndroidStorageError> {
    with_bridge(|env, bridge| {
        let uri = env.new_string(uri)?;
        let result = env
            .call_method(
                bridge,
                "sourceRevision",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&uri)],
            )?
            .l()?;
        let result = JString::from(result);
        let value: String = env.get_string(&result)?.into();
        Ok(value)
    })
}

#[derive(Debug, Error)]
pub enum AndroidStorageError {
    #[error("Android SAF 桥尚未注册")]
    NotRegistered,
    #[error("Android JNI 调用失败: {0}")]
    Jni(#[from] jni::errors::Error),
    #[error("Android SAF 数据编码失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Android 文档重命名失败")]
    FinalizeFailed,
    #[error("Android 临时文档删除失败")]
    DeleteFailed,
}
