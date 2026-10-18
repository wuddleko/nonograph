use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

struct Bundle {
    package: &'static str,
    pkg_dir: &'static str,
    js_name: &'static str,
    wasm_name: &'static str,
    artifact: &'static str,
    watched: &'static [&'static str],
}

const PAGE: Bundle = Bundle {
    package: "nostorgraph-page",
    pkg_dir: "page/pkg",
    js_name: "nostorgraph_page.js",
    wasm_name: "nostorgraph_page_bg.wasm",
    artifact: "nostorgraph_page.wasm",
    watched: &[
        "page/src/lib.rs",
        "page/Cargo.toml",
        "parser/src/lib.rs",
        "parser/Cargo.toml",
        "nip44/src/lib.rs",
        "nip44/src/seal.rs",
        "nip44/Cargo.toml",
    ],
};

const PDF: Bundle = Bundle {
    package: "nostorgraph-pdf",
    pkg_dir: "pdf/pkg",
    js_name: "nostorgraph_pdf.js",
    wasm_name: "nostorgraph_pdf_bg.wasm",
    artifact: "nostorgraph_pdf.wasm",
    watched: &[
        "pdf/wasm/src/lib.rs",
        "pdf/wasm/Cargo.toml",
        "pdf/Cargo.toml",
        "pdf/src/encode.rs",
        "pdf/src/font.rs",
        "pdf/src/lib.rs",
        "pdf/src/note.rs",
        "pdf/src/place.rs",
        "pdf/src/read.rs",
        "pdf/src/wrap.rs",
        "pdf/src/write.rs",
        "pdf/fonts/DejaVuSans.ttf",
        "pdf/fonts/DejaVuSans-Bold.ttf",
        "pdf/fonts/DejaVuSans-Oblique.ttf",
        "pdf/fonts/DejaVuSans-BoldOblique.ttf",
        "pdf/fonts/DejaVuSansMono.ttf",
        "pdf/fonts/DejaVuSansMono-Bold.ttf",
        "pdf/fonts/DejaVuSansMono-Oblique.ttf",
        "pdf/fonts/DejaVuSansMono-BoldOblique.ttf",
    ],
};

const BUNDLES: &[Bundle] = &[PAGE, PDF];

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NOSTORGRAPH_REBUILD_PAGE");
    let force = std::env::var_os("NOSTORGRAPH_REBUILD_PAGE").is_some();
    // Both browser bundles share this directory so the wasm32 dependencies
    // compile once. The page bundle is the one that already lives here.
    let wasm_target = manifest_dir.join("target/wasm-page");

    let mut pending = Vec::new();
    for bundle in BUNDLES {
        for source in bundle.watched {
            println!("cargo:rerun-if-changed={source}");
        }
        println!(
            "cargo:rerun-if-changed={}/{}",
            bundle.pkg_dir, bundle.js_name
        );
        println!(
            "cargo:rerun-if-changed={}/{}",
            bundle.pkg_dir, bundle.wasm_name
        );
        if !installed_from_cache(bundle, &manifest_dir, &out_dir, force) {
            pending.push(bundle);
        }
    }
    if pending.is_empty() {
        return;
    }

    let built = build_packages(&manifest_dir, &wasm_target, &pending);
    for bundle in pending {
        let pkg_js = manifest_dir.join(bundle.pkg_dir).join(bundle.js_name);
        let pkg_wasm = manifest_dir.join(bundle.pkg_dir).join(bundle.wasm_name);
        let have_pkg = pkg_js.is_file() && pkg_wasm.is_file();
        let result = match &built {
            Ok(()) => bindgen_bundle(bundle, &manifest_dir, &wasm_target, &out_dir),
            Err(error) => Err(error.clone()),
        };
        if let Err(error) = result {
            restore_or_panic(bundle, &error, have_pkg, &pkg_js, &pkg_wasm, &out_dir);
        }
    }
}

fn installed_from_cache(bundle: &Bundle, manifest_dir: &Path, out_dir: &Path, force: bool) -> bool {
    let pkg_js = manifest_dir.join(bundle.pkg_dir).join(bundle.js_name);
    let pkg_wasm = manifest_dir.join(bundle.pkg_dir).join(bundle.wasm_name);
    let have_pkg = pkg_js.is_file() && pkg_wasm.is_file();
    let stale = have_pkg && bundle_is_stale(manifest_dir, &pkg_wasm, bundle.watched);
    if have_pkg && !force && !stale {
        copy_pair(&pkg_js, &pkg_wasm, out_dir, bundle);
        true
    } else {
        false
    }
}

fn bundle_is_stale(manifest_dir: &Path, wasm: &Path, watched: &[&str]) -> bool {
    let Ok(wasm_time) = mtime(wasm) else {
        return true;
    };
    watched
        .iter()
        .any(|path| mtime(&manifest_dir.join(path)).is_ok_and(|time| time > wasm_time))
}

fn mtime(path: &Path) -> Result<SystemTime, std::io::Error> {
    Ok(std::fs::metadata(path)?.modified()?)
}

fn copy_pair(js: &Path, wasm: &Path, dest_dir: &Path, bundle: &Bundle) {
    std::fs::copy(js, dest_dir.join(bundle.js_name))
        .unwrap_or_else(|error| panic!("copy {js:?}: {error}"));
    std::fs::copy(wasm, dest_dir.join(bundle.wasm_name))
        .unwrap_or_else(|error| panic!("copy {wasm:?}: {error}"));
}

fn build_packages(
    manifest_dir: &Path,
    wasm_target: &Path,
    bundles: &[&Bundle],
) -> Result<(), String> {
    let mut command = cargo_command();
    command
        .current_dir(manifest_dir)
        .env("CARGO_TARGET_DIR", wasm_target)
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"]);
    for bundle in bundles {
        command.args(["-p", bundle.package]);
    }
    let status = command.status().map_err(|error| {
        format!(
            "failed to spawn cargo for the {} wasm build: {error}",
            bundle_names(bundles)
        )
    })?;
    if !status.success() {
        return Err(format!(
            "building {} for wasm32-unknown-unknown failed. \
             Install the target with: rustup target add wasm32-unknown-unknown",
            bundle_names(bundles)
        ));
    }
    Ok(())
}

fn bundle_names(bundles: &[&Bundle]) -> String {
    bundles
        .iter()
        .map(|bundle| bundle.package)
        .collect::<Vec<_>>()
        .join(", ")
}

fn bindgen_bundle(
    bundle: &Bundle,
    manifest_dir: &Path,
    wasm_target: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let wasm_in = wasm_target
        .join("wasm32-unknown-unknown")
        .join("release")
        .join(bundle.artifact);
    let staging = out_dir.join(format!("bindgen-{}", bundle.package));
    std::fs::create_dir_all(&staging).map_err(|error| format!("create {staging:?}: {error}"))?;
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(&wasm_in)
        .web(true)
        .map_err(|error| format!("wasm-bindgen web target for {}: {error}", bundle.package))?
        .generate(&staging)
        .map_err(|error| format!("wasm-bindgen generate for {}: {error}", bundle.package))?;

    let js = staging.join(bundle.js_name);
    let wasm = staging.join(bundle.wasm_name);
    if !js.is_file() || !wasm.is_file() {
        return Err(format!(
            "wasm-bindgen for {} did not write {} and {}",
            bundle.package, bundle.js_name, bundle.wasm_name
        ));
    }
    copy_pair(&js, &wasm, out_dir, bundle);
    let pkg_dir = manifest_dir.join(bundle.pkg_dir);
    std::fs::create_dir_all(&pkg_dir)
        .map_err(|error| format!("create {}: {error}", bundle.pkg_dir))?;
    std::fs::copy(&js, pkg_dir.join(bundle.js_name))
        .map_err(|error| format!("cache {} js: {error}", bundle.package))?;
    std::fs::copy(&wasm, pkg_dir.join(bundle.wasm_name))
        .map_err(|error| format!("cache {} wasm: {error}", bundle.package))?;
    Ok(())
}

fn restore_or_panic(
    bundle: &Bundle,
    error: &str,
    have_pkg: bool,
    pkg_js: &Path,
    pkg_wasm: &Path,
    out_dir: &Path,
) {
    if have_pkg {
        println!(
            "cargo:warning={} wasm build failed ({error}); using the existing {} bundle",
            bundle.package, bundle.pkg_dir
        );
        copy_pair(pkg_js, pkg_wasm, out_dir, bundle);
    } else {
        panic!("{error}");
    }
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
