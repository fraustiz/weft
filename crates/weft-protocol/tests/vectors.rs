//! The committed vectors must match what the generator produces today.

#[path = "../examples/gen_vectors.rs"]
#[allow(dead_code)]
mod gen_vectors;

#[test]
fn committed_vectors_match_generator() {
    let committed = include_str!("../vectors/v1.json");
    let generated = gen_vectors::vectors_json();
    assert!(
        committed == generated,
        "vectors/v1.json is out of date: run `cargo run -p weft-protocol --example gen_vectors`"
    );
}
