//! grill-mcp — transport prototype (ticket 0002).
//!
//! Alignment spike: prove the best-of-breed model crates compile and LINK
//! against ONE shared ort (onnxruntime) build. `extern crate` forces each
//! crate into the link without depending on its (unstable) public API — a
//! successful build here means version-unification held all the way through
//! to a single native onnxruntime with no duplicate symbols.
extern crate kokoro_tts;
extern crate ort;
extern crate transcribe_rs;

fn main() {
    println!("grill-mcp: transcribe-rs + kokoro-tts + ort linked on one runtime");
}
