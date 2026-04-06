use rfd::FileDialog;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
pub(crate) struct SaveFile {
    name: String,
    content: String,
}

fn write_files_to_directory(files: Vec<SaveFile>, folder: &Path) -> Value {
    let mut success: usize = 0;
    let mut error_count: usize = 0;
    let mut errors: Vec<String> = Vec::new();

    for file in files {
        let path = folder.join(&file.name);
        match fs::write(&path, file.content.as_bytes()) {
            Ok(_) => success += 1,
            Err(error) => {
                error_count += 1;
                errors.push(format!("{}: {}", file.name, error));
            }
        }
    }

    json!({
        "success": success > 0,
        "successCount": success,
        "errorCount": error_count,
        "errors": if errors.is_empty() { Value::Null } else { json!(errors) }
    })
}

#[tauri::command]
pub(crate) fn save_files_to_directory(files: Vec<SaveFile>) -> Result<Value, String> {
    if files.is_empty() {
        return Ok(json!({"success": false, "error": "No files to save"}));
    }

    let folder = FileDialog::new()
        .set_title("Choose save directory")
        .pick_folder()
        .ok_or_else(|| "User cancelled directory selection".to_string())?;

    Ok(write_files_to_directory(files, &folder))
}

#[cfg(test)]
mod tests {
    use super::{write_files_to_directory, SaveFile};
    use std::fs;

    #[test]
    fn write_files_to_directory_reports_success_counts() {
        let temp_root =
            std::env::temp_dir().join(format!("nicecli-file-export-test-{}", std::process::id()));
        if temp_root.exists() {
            let _ = fs::remove_dir_all(&temp_root);
        }
        fs::create_dir_all(&temp_root).expect("create temp directory");

        let payload = write_files_to_directory(
            vec![
                SaveFile {
                    name: "alpha.txt".to_string(),
                    content: "alpha".to_string(),
                },
                SaveFile {
                    name: "beta.txt".to_string(),
                    content: "beta".to_string(),
                },
            ],
            &temp_root,
        );

        assert_eq!(payload["success"], true);
        assert_eq!(payload["successCount"], 2);
        assert_eq!(payload["errorCount"], 0);
        assert!(payload["errors"].is_null());
        assert_eq!(
            fs::read_to_string(temp_root.join("alpha.txt")).expect("read alpha file"),
            "alpha"
        );
        assert_eq!(
            fs::read_to_string(temp_root.join("beta.txt")).expect("read beta file"),
            "beta"
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp directory");
    }
}
