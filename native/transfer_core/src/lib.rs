#[cfg(target_os = "android")]
pub mod android_storage;
pub mod chunk;
pub mod core;
pub mod ffi;
pub mod identity;
pub mod incoming;
pub mod lan;
pub mod manifest;
pub mod mdns;
pub mod model;
pub mod network;
pub mod outgoing;
pub mod path_safety;
pub mod persistence;
pub mod protocol;
pub mod storage;
pub mod transfer;
pub mod wire;

// cargo-fuzz 在 Windows 上链接 cdylib 依赖时会注入 /include:main，
// 需要一个可解析的 main 符号；仅在 fuzz 特性构建时导出，无副作用。
#[cfg(all(windows, feature = "fuzz"))]
#[unsafe(no_mangle)]
pub extern "system" fn main() {}
