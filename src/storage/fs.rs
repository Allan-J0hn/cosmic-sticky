// SPDX-License-Identifier: GPL-3.0-only

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `data` to `.<name>.tmp` beside `path`, syncs it, then renames it over `path`.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    write_atomic_with(path, data, |_| Ok(()))
}

fn write_atomic_with(
    path: &Path,
    data: &[u8],
    before_rename: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let (dir, name) = match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => (dir, name),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a file path",
            ));
        }
    };
    let tmp = dir.join(format!(".{}.tmp", name.to_string_lossy()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        before_rename(&tmp)?;
        fs::rename(&tmp, path)
    })();
    match result {
        Ok(()) => {
            sync_dir(dir);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

pub fn sync_dir(dir: &Path) {
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        tracing::debug!("fsync {}: {e}", dir.display());
    }
}

pub fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => {
            if let Some(dir) = path.parent() {
                sync_dir(dir);
            }
            Ok(())
        }
    }
}

/// Moves an unreadable file into `corrupt_dir` without overwriting anything already there.
pub fn quarantine(path: &Path, corrupt_dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(corrupt_dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let mut dest = corrupt_dir.join(name);
    let stem = path.file_stem().unwrap_or(name).to_string_lossy();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 1;
    while dest.symlink_metadata().is_ok() {
        dest = corrupt_dir.join(format!("{stem}.{n}{ext}"));
        n += 1;
    }
    fs::rename(path, &dest)?;
    sync_dir(corrupt_dir);
    if let Some(dir) = path.parent() {
        sync_dir(dir);
    }
    tracing::warn!("moved unreadable {} to {}", path.display(), dest.display());
    Ok(dest)
}

/// Visible `*.md` files in `dir`; temp files and other dotfiles are skipped.
pub fn markdown_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        let is_md = path.extension().is_some_and(|e| e == "md");
        if !hidden && is_md && entry.file_type()?.is_file() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn writes_new_and_replaces_existing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        write_atomic(&p, b"one").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"one");
        write_atomic(&p, b"two, longer").unwrap();
        write_atomic(&p, b"3").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"3");
        assert_eq!(entries(dir.path()), ["a.md"]);
    }

    #[test]
    fn failure_before_rename_keeps_old_content_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        write_atomic(&p, b"original").unwrap();
        let err = write_atomic_with(&p, b"partial new content", |tmp| {
            assert_eq!(fs::read(tmp).unwrap(), b"partial new content");
            assert_eq!(fs::read(&p).unwrap(), b"original");
            Err(io::Error::other("simulated crash"))
        });
        assert!(err.is_err());
        assert_eq!(fs::read(&p).unwrap(), b"original");
        assert_eq!(entries(dir.path()), ["a.md"]);
    }

    #[test]
    fn failure_on_new_file_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("new.md");
        assert!(write_atomic_with(&p, b"x", |_| Err(io::Error::other("boom"))).is_err());
        assert!(entries(dir.path()).is_empty());
    }

    #[test]
    fn stale_temp_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        fs::write(
            dir.path().join(".a.md.tmp"),
            b"left over from a crash, much longer",
        )
        .unwrap();
        write_atomic(&p, b"fresh").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"fresh");
        assert_eq!(entries(dir.path()), ["a.md"]);
    }

    #[test]
    fn missing_directory_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_atomic(&dir.path().join("nope/a.md"), b"x").is_err());
    }

    #[test]
    fn quarantine_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let corrupt = dir.path().join("corrupt");
        for (i, content) in ["first", "second", "third"].iter().enumerate() {
            let p = dir.path().join("bad.md");
            fs::write(&p, content).unwrap();
            let dest = quarantine(&p, &corrupt).unwrap();
            assert!(!p.exists());
            assert_eq!(fs::read_to_string(&dest).unwrap(), *content, "move {i}");
        }
        assert_eq!(entries(&corrupt), ["bad.1.md", "bad.2.md", "bad.md"]);
        assert_eq!(fs::read_to_string(corrupt.join("bad.md")).unwrap(), "first");
    }

    #[test]
    fn lists_only_visible_markdown_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "b.md",
            "a.md",
            ".a.md.tmp",
            ".hidden.md",
            "notes.txt",
            "x.md~",
        ] {
            fs::write(dir.path().join(name), "").unwrap();
        }
        fs::create_dir(dir.path().join("sub.md")).unwrap();
        let names: Vec<_> = markdown_files(dir.path())
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.md", "b.md"]);
    }
}
