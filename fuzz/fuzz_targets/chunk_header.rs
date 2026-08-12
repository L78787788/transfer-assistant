#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message;
use transfer_core::protocol::wire::ChunkHeader;

fuzz_target!(|data: &[u8]| {
    // 数据通道块头是网络中唯一直接解码的未知字节流，必须能在任意输入下
    // 不 panic、不越界。解码成功后做一次完整的字段读取以覆盖所有分支。
    if let Ok(header) = ChunkHeader::decode(data) {
        let _ = header.transfer_id;
        let _ = header.item_id;
        let _ = header.chunk_index;
        let _ = header.offset;
        let _ = header.length;
        let _ = header.blake3_hash;
    }
});
