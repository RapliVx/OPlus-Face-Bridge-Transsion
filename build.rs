use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=aidl");
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_file = PathBuf::from(&out_dir).join("face_aidl.rs");

    rsbinder_aidl::Builder::new()
        .source(&PathBuf::from("aidl"))
        .output(&out_file)
        .generate()
        .expect("failed to generate face AIDL bindings");

    let generated = fs::read_to_string(&out_file).expect("failed to read generated bindings");
    let patched = generated
        .lines()
        .map(|line| {
            if line.contains("const r#getInterfaceVersion:") {
                "                        pub(crate) const r#getInterfaceVersion: rsbinder::TransactionCode = 16777215;".to_string()
            } else if line.contains("const r#getInterfaceHash:") {
                "                        pub(crate) const r#getInterfaceHash: rsbinder::TransactionCode = 16777214;".to_string()
            } else if line.contains("#![allow(non_upper_case_globals, non_snake_case, dead_code)]") {
                line.replace("dead_code", "dead_code, non_camel_case_types")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&out_file, patched).expect("failed to write patched bindings");

    println!("cargo:rerun-if-changed=src/surface_bridge.cpp");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .flag_if_supported("-O3")
        .flag_if_supported("-fvisibility=hidden")
        .file("src/surface_bridge.cpp")
        .compile("surface_bridge");
}
