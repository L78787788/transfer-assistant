fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc is available");
    // Build scripts run in their own process before compilation starts.
    unsafe { std::env::set_var("PROTOC", protoc) };
    prost_build::compile_protos(&["proto/transfer.proto"], &["proto"])
        .expect("transfer protocol must compile");
}
