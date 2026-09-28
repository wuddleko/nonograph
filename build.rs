use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const PAGE_JS_NAME: &str = "nonograph_page.js";
const PAGE_WASM_NAME: &str = "nonograph_page_bg.wasm";
const WATCHED_SOURCES: &[&str] = &[
    "page/src/lib.rs",
    "page/Cargo.toml",
    "parser/src/lib.rs",
    "parser/Cargo.toml",
];

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=build.rs");
    for source in WATCHED_SOURCES {
        println!("cargo:rerun-if-changed={source}");
    }
    println!("cargo:rerun-if-changed=page/pkg/{PAGE_JS_NAME}");
    println!("cargo:rerun-if-changed=page/pkg/{PAGE_WASM_NAME}");
    println!("cargo:rerun-if-env-changed=NONOGRAPH_REBUILD_PAGE");

    let pkg_dir = manifest_dir.join("page/pkg");
    let pkg_js = pkg_dir.join(PAGE_JS_NAME);
    let pkg_wasm = pkg_dir.join(PAGE_WASM_NAME);
    let force = std::env::var_os("NONOGRAPH_REBUILD_PAGE").is_some();
    let have_pkg = pkg_js.is_file() && pkg_wasm.is_file();
    let stale = have_pkg && bundle_is_stale(&manifest_dir, &pkg_wasm);

    if have_pkg && !force && !stale {
        install_bundle(&pkg_js, &pkg_wasm, &out_dir);
        return;
    }

    match build_page_wasm(&manifest_dir, &out_dir) {
        Ok(()) => {}
        Err(error) if have_pkg => {
            println!(
                "cargo:warning=page wasm build failed ({error}); using the existing page/pkg bundle"
            );
            install_bundle(&pkg_js, &pkg_wasm, &out_dir);
        }
        Err(error) => panic!("{error}"),
    }
}

fn bundle_is_stale(manifest_dir: &Path, wasm: &Path) -> bool {
    let Ok(wasm_time) = mtime(wasm) else {
        return true;
    };
    WATCHED_SOURCES
        .iter()
        .any(|path| mtime(&manifest_dir.join(path)).is_ok_and(|time| time > wasm_time))
}

fn mtime(path: &Path) -> Result<SystemTime, std::io::Error> {
    Ok(std::fs::metadata(path)?.modified()?)
}

fn install_bundle(js: &Path, wasm: &Path, out_dir: &Path) {
    std::fs::copy(js, out_dir.join(PAGE_JS_NAME))
        .unwrap_or_else(|error| panic!("copy {js:?}: {error}"));
    std::fs::copy(wasm, out_dir.join(PAGE_WASM_NAME))
        .unwrap_or_else(|error| panic!("copy {wasm:?}: {error}"));
    write_asset_version(out_dir);
}

fn write_asset_version(out_dir: &Path) {
    let version = asset_version(&out_dir.join(PAGE_JS_NAME), &out_dir.join(PAGE_WASM_NAME));
    std::fs::write(out_dir.join("parser_asset_version.txt"), version)
        .expect("write parser asset version");
}

fn build_page_wasm(manifest_dir: &Path, out_dir: &Path) -> Result<(), String> {
    let wasm_target = manifest_dir.join("target/wasm-page");
    let status = cargo_command()
        .current_dir(manifest_dir)
        .env("CARGO_TARGET_DIR", &wasm_target)
        .args([
            "build",
            "-p",
            "nonograph-page",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .status()
        .map_err(|error| format!("failed to spawn cargo for the page wasm build: {error}"))?;
    if !status.success() {
        return Err(
            "building nonograph-page for wasm32-unknown-unknown failed. \
             Install the target with: rustup target add wasm32-unknown-unknown"
                .to_string(),
        );
    }

    let wasm_in = wasm_target
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("nonograph_page.wasm");
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(&wasm_in)
        .web(true)
        .map_err(|error| format!("wasm-bindgen web target: {error}"))?
        .generate(out_dir)
        .map_err(|error| format!("wasm-bindgen generate: {error}"))?;

    let pkg_dir = manifest_dir.join("page/pkg");
    std::fs::create_dir_all(&pkg_dir).map_err(|error| format!("create page/pkg: {error}"))?;
    std::fs::copy(out_dir.join(PAGE_JS_NAME), pkg_dir.join(PAGE_JS_NAME))
        .map_err(|error| format!("cache page js: {error}"))?;
    std::fs::copy(out_dir.join(PAGE_WASM_NAME), pkg_dir.join(PAGE_WASM_NAME))
        .map_err(|error| format!("cache page wasm: {error}"))?;
    write_asset_version(out_dir);
    Ok(())
}

fn asset_version(js_path: &Path, wasm_path: &Path) -> String {
    let js = std::fs::read(js_path).expect("read generated page js");
    let wasm = std::fs::read(wasm_path).expect("read generated page wasm");
    let mut hasher = Sha256::new();
    hasher.update(js);
    hasher.update(wasm);
    let digest = hasher.finalize();
    let mut version = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        version.push_str(&format!("{byte:02x}"));
    }
    version
}

fn cargo_command() -> Command {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    let remove: Vec<String> = std::env::vars()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with("CARGO_") && key != "CARGO_HOME" && key != "CARGO_TARGET_DIR")
        .collect();
    for key in remove {
        command.env_remove(key);
    }
    command.env_remove("RUSTFLAGS");
    command.env_remove("CARGO_ENCODED_RUSTFLAGS");
    command.env_remove("RUSTC_WRAPPER");
    command.env_remove("RUSTC_WORKSPACE_WRAPPER");
    command
}
