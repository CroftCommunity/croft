//! Live-rig helper: print the endpoint ids the rig secrets imply, so the
//! local admit can be told which endpoints its local camp key stands behind.
//!
//! Usage: `CROFT_LIVE_CALLEE_SECRET_HEX=… CROFT_LIVE_CALLER_SECRET_HEX=… \
//!   cargo run -p call-transport-iroh --example rig_ids`
//!
//! Prints `callee=<hex id>` and `caller=<hex id>`. Reads secrets, prints
//! only public halves.

use call_transport_iroh::CallEndpoint;

fn secret(name: &str) -> [u8; 32] {
    let hex = std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let bytes = data_encoding::HEXLOWER
        .decode(hex.trim().as_bytes())
        .unwrap_or_else(|e| panic!("{name} is not lowercase hex: {e}"));
    bytes
        .as_slice()
        .try_into()
        .unwrap_or_else(|_| panic!("{name} must be 32 bytes, got {}", bytes.len()))
}

fn main() {
    println!(
        "callee={}",
        CallEndpoint::endpoint_id_for(&secret("CROFT_LIVE_CALLEE_SECRET_HEX"))
    );
    println!(
        "caller={}",
        CallEndpoint::endpoint_id_for(&secret("CROFT_LIVE_CALLER_SECRET_HEX"))
    );
}
