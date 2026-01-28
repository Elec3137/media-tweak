use std::{ffi::OsStr, path::PathBuf};

pub async fn pick_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}
pub async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|file| file.path().to_path_buf())
}

/// returns a path with a different filename
pub async fn modify_path(mut path: PathBuf) -> PathBuf {
    path.set_file_name(format!(
        "{}_edited.{}",
        path.file_stem()
            .unwrap_or_else(|| OsStr::new("media"))
            .to_str()
            .unwrap_or_else(|| {
                eprintln!("Failed to decode file_stem");
                ""
            }),
        path.extension()
            .unwrap_or_else(|| OsStr::new("mkv"))
            .to_str()
            .unwrap_or_else(|| {
                eprintln!("Failed to decode extension");
                ""
            })
    ));

    path
}
