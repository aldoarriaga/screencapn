use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if let Err(error) = copy_webview2_loader() {
        println!("cargo:warning=WebView2Loader.dll was not copied: {error}");
    }
}

fn copy_webview2_loader() -> Result<(), Box<dyn std::error::Error>> {
    let arch = match env::var("CARGO_CFG_TARGET_ARCH")?.as_str() {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => return Err(format!("unsupported WebView2 architecture: {other}").into()),
    };

    let source = env::var_os("WEBVIEW2_LOADER_DLL")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| find_loader_in_cargo_registry(arch))
        .ok_or("could not find WebView2Loader.dll")?;

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let profile_dir =
        profile_dir_from_out_dir(&out_dir).ok_or("could not resolve target profile dir")?;
    let destination = profile_dir.join("WebView2Loader.dll");
    if !files_have_same_contents(&source, &destination) {
        fs::copy(&source, destination)?;
    }
    println!("cargo:rerun-if-changed={}", source.display());
    Ok(())
}

fn files_have_same_contents(left: &Path, right: &Path) -> bool {
    let Ok(left_metadata) = fs::metadata(left) else {
        return false;
    };
    let Ok(right_metadata) = fs::metadata(right) else {
        return false;
    };
    left_metadata.len() == right_metadata.len()
        && fs::read(left)
            .ok()
            .zip(fs::read(right).ok())
            .is_some_and(|(left, right)| left == right)
}

fn find_loader_in_cargo_registry(arch: &str) -> Option<PathBuf> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")))?;
    let registry_src = cargo_home.join("registry").join("src");
    for registry in fs::read_dir(registry_src).ok()?.flatten() {
        let Ok(entries) = fs::read_dir(registry.path()) else {
            continue;
        };
        let mut packages = entries
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("webview2-com-sys-")
            })
            .collect::<Vec<_>>();
        packages.sort_by_key(|entry| entry.file_name());
        for package in packages.into_iter().rev() {
            let candidate = package.path().join(arch).join("WebView2Loader.dll");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn profile_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    out_dir
        .ancestors()
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "debug" || name == "release")
        })
        .map(Path::to_path_buf)
}
