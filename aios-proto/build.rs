//! Build script for `aios-proto`.
//!
//! Generates Rust types from `core/life/proto/aios/v1/*.proto` using
//! `tonic-prost-build`. The proto root is three levels up from the crate
//! manifest dir (`crates/aios/aios-proto/` → `core/life/`).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let proto_root = manifest_dir
        .parent() // crates/aios/
        .and_then(|p| p.parent()) // crates/
        .and_then(|p| p.parent()) // core/life/
        .ok_or("walking up to core/life/")?
        .join("proto");

    let aios_dir = proto_root.join("aios").join("v1");
    let mut proto_files: Vec<std::path::PathBuf> = std::fs::read_dir(&aios_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("proto"))
        .collect();
    proto_files.sort();

    for proto in &proto_files {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    tonic_prost_build::configure()
        .build_server(false) // aios.v1 has no services — vocabulary only
        .build_client(false)
        .compile_protos(&proto_files, &[proto_root])?;

    Ok(())
}
