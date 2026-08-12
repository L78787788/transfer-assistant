# Rust 内核（transfer_core）通过 JNI 按方法名反射调用 AndroidStorageBridge：
# prepareTargets / finalizeTarget / deleteTarget / openSource / sourceRevision。
# R8 混淆会重命名这些方法导致 NoSuchMethodError，必须保留桥类全貌。
-keep class com.transassist.transfer_assistant.AndroidStorageBridge { *; }
