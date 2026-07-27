use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CATEGORIES: &[&str] = &[
    "capture",
    "selection",
    "editing",
    "tools",
    "output",
    "preferences",
    "updates",
    "shortcuts",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "gnu" {
        copy_webview2_loader().expect("WebView2Loader.dll is required beside GNU Windows builds");
    } else if let Err(error) = copy_webview2_loader() {
        println!("cargo:warning=WebView2Loader.dll was not copied: {error}");
    }

    let source = PathBuf::from("assets/tips/tips.csv");
    println!("cargo:rerun-if-changed={}", source.display());

    let mut reader = csv::Reader::from_path(&source).expect("open capture tips CSV");
    let mut seen_ids = HashSet::new();
    let mut generated = String::from("pub static CAPTURE_TIPS: &[TipDefinition] = &[\n");
    let mut count = 0usize;

    for (index, record) in reader.records().enumerate() {
        let record =
            record.unwrap_or_else(|error| panic!("invalid tip row {}: {error}", index + 2));
        let id = record.get(0).unwrap_or_default().trim();
        let category = record.get(1).unwrap_or_default().trim();
        let text = record.get(2).unwrap_or_default().trim();

        assert!(
            !id.is_empty()
                && id.chars().all(|character| character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '-'),
            "tip row {} has an invalid id",
            index + 2
        );
        assert!(seen_ids.insert(id.to_string()), "duplicate tip id: {id}");
        assert!(
            CATEGORIES.contains(&category),
            "tip {id} has unrecognized category {category}"
        );
        assert_balanced_shortcuts(id, text);
        let visible_length = text
            .chars()
            .filter(|character| !matches!(character, '{' | '}'))
            .count();
        assert!(
            visible_length <= 84,
            "tip {id} exceeds 84 visible characters"
        );
        assert!(!text.is_empty(), "tip {id} is empty");

        generated.push_str(&format!(
            "    TipDefinition {{ id: {id:?}, category: {category:?}, text: {text:?} }},\n"
        ));
        count += 1;
    }
    assert!(count >= 30, "capture tips must contain at least 30 rows");
    generated.push_str("];\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("capture_tips.rs");
    fs::write(output, generated).expect("write generated capture tips");
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

fn assert_balanced_shortcuts(id: &str, text: &str) {
    let mut inside = false;
    let mut content = false;
    for character in text.chars() {
        match character {
            '{' if !inside => {
                inside = true;
                content = false;
            }
            '}' if inside && content => inside = false,
            '{' | '}' => panic!("tip {id} has invalid shortcut markers"),
            _ if inside => content = true,
            _ => {}
        }
    }
    assert!(!inside, "tip {id} has an unclosed shortcut marker");
}
