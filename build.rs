use std::env;
use std::path::PathBuf;

/// Detect the GCC internal include path for headers like `<stddef.h>`.
/// Falls back to `None` if `gcc` is not available or the path doesn't exist.
fn gcc_include_path() -> Option<String> {
    let output = std::process::Command::new("gcc")
        .args(["-print-file-name=include"])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if !path.is_empty() && std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=avm/avm_decoder.h");
    println!("cargo:rerun-if-changed=avm/avmdx.h");

    let dst = cmake::Config::new(".")
        .define("CONFIG_AV2_ENCODER", "0")
        .define("CONFIG_MULTITHREAD", "1")
        .define("ENABLE_EXAMPLES", "OFF")
        .define("ENABLE_TESTS", "OFF")
        .define("ENABLE_TOOLS", "OFF")
        .define("ENABLE_DOCS", "OFF")
        .build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    println!("cargo:rustc-link-lib=static=avm");

    let mut builder = bindgen::Builder::default()
        .header("avm/avm_decoder.h")
        .header("avm/avmdx.h")
        .clang_arg("-I.")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if let Some(gcc_inc) = gcc_include_path() {
        builder = builder.clang_arg(format!("-I{gcc_inc}"));
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
