//! Embeds `WEFT_VERSION_NAME` and `WEFT_VERSION_CODE` at compile time so
//! release builds and local builds report consistent version metadata.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WEFT_VERSION");
    println!("cargo:rerun-if-env-changed=WEFT_VERSION_CODE");

    let version_name = std::env::var("WEFT_VERSION")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").expect("set by cargo"));
    let version_code: u64 = match std::env::var("WEFT_VERSION_CODE") {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|&code| code > 0)
            .expect("WEFT_VERSION_CODE must be a positive integer"),
        _ => 1,
    };
    println!("cargo:rustc-env=WEFT_VERSION_NAME={version_name}");
    println!("cargo:rustc-env=WEFT_VERSION_CODE={version_code}");
}
