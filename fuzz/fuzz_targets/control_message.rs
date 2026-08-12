#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message;
use transfer_core::protocol::wire;

fuzz_target!(|data: &[u8]| {
    let _ = wire::Envelope::decode(data);
});
