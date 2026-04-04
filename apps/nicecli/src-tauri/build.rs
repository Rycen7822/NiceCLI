use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    build_embedded_cliproxyapi().expect("failed to build embedded CLIProxyAPI payload");
    tauri_build::build();
}

fn build_embedded_cliproxyapi() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let nicecli_root = manifest_dir
        .parent()
        .ok_or_else(|| "failed to resolve NiceCLI root".to_string())?;
    let apps_root = nicecli_root
        .parent()
        .ok_or_else(|| "failed to resolve apps root".to_string())?;
    let cliproxy_root = apps_root.join("cliproxyapi");
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);

    register_rerun(&cliproxy_root)?;
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("build.rs").display()
    );

    let target = env::var("TARGET").map_err(|e| e.to_string())?;
    let (goos, goarch, executable_name) = target_to_go(&target)?;
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "embedded".to_string());
    let build_date = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        .to_string();

    let embedded_dir = out_dir.join("embedded");
    fs::create_dir_all(&embedded_dir).map_err(|e| e.to_string())?;

    let backend_output = embedded_dir.join(executable_name);
    let config_example = cliproxy_root.join("config.example.yaml");
    let config_output = embedded_dir.join("config.example.yaml");

    let status = Command::new("go")
        .current_dir(&cliproxy_root)
        .env("GOOS", goos)
        .env("GOARCH", goarch)
        .args([
            "build",
            "-tags",
            "desktoplite",
            "-trimpath",
            "-ldflags",
            &format!(
                "-s -w -X main.Version={version} -X main.Commit=nicecli-embedded -X main.BuildDate={build_date} -X github.com/router-for-me/CLIProxyAPI/v6/internal/buildinfo.Flavor=desktop-lite"
            ),
            "-o",
            backend_output
                .to_str()
                .ok_or_else(|| "invalid backend output path".to_string())?,
            "./cmd/desktoplite",
        ])
        .status()
        .map_err(|e| format!("failed to run go build: {e}"))?;

    if !status.success() {
        return Err(format!("go build exited with status {status}"));
    }

    fs::copy(&config_example, &config_output).map_err(|e| {
        format!(
            "failed to copy config.example.yaml from {}: {e}",
            config_example.display()
        )
    })?;

    let generated = format!(
        "pub const EMBEDDED_BACKEND_VERSION: &str = {version:?};\n\
pub const EMBEDDED_BACKEND_FILENAME: &str = {filename:?};\n\
pub static EMBEDDED_BACKEND_BYTES: &[u8] = include_bytes!(r#\"{backend}\"#);\n\
pub static EMBEDDED_CONFIG_EXAMPLE: &str = include_str!(r#\"{config}\"#);\n",
        filename = executable_name,
        backend = backend_output.display(),
        config = config_output.display(),
    );
    fs::write(out_dir.join("embedded_backend.rs"), generated).map_err(|e| e.to_string())?;

    Ok(())
}

fn target_to_go(target: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    match target {
        "x86_64-pc-windows-msvc" => Ok(("windows", "amd64", "cli-proxy-api.exe")),
        "aarch64-pc-windows-msvc" => Ok(("windows", "arm64", "cli-proxy-api.exe")),
        "x86_64-apple-darwin" => Ok(("darwin", "amd64", "cli-proxy-api")),
        "aarch64-apple-darwin" => Ok(("darwin", "arm64", "cli-proxy-api")),
        "x86_64-unknown-linux-gnu" => Ok(("linux", "amd64", "cli-proxy-api")),
        "aarch64-unknown-linux-gnu" => Ok(("linux", "arm64", "cli-proxy-api")),
        other => Err(format!(
            "unsupported cargo target for embedded backend: {other}"
        )),
    }
}

fn register_rerun(root: &Path) -> Result<(), String> {
    for path in walk_relevant_files(root)? {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    Ok(())
}

fn walk_relevant_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    visit(root, &mut files)?;
    Ok(files)
}

fn visit(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("failed to stat {}: {e}", path.display()))?;
    if metadata.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|e| format!("failed to read dir {}: {e}", path.display()))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let child = entry.path();
            if should_skip_dir(&child) {
                continue;
            }
            visit(&child, files)?;
        }
        return Ok(());
    }

    if let Some(ext) = path.extension().and_then(|value| value.to_str()) {
        if matches!(ext, "go" | "mod" | "sum" | "yaml") {
            files.push(path.to_path_buf());
        }
    }

    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some(".git") | Some("dist") | Some("target") | Some("tmp")
    )
}
