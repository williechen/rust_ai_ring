use crate::file_organizer::AuthorizedRoot;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectoryEntry {
    name: String,
    is_directory: bool,
    size_bytes: Option<u64>,
    modified_at: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectoryListing {
    entries: Vec<DirectoryEntry>,
    skipped: usize,
}

#[cfg(target_os = "linux")]
fn open_relative(
    root: &AuthorizedRoot,
    path: &[String],
    directory: bool,
) -> Result<std::fs::File, String> {
    use std::{
        ffi::{CString, OsStr},
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
        path::{Component, Path},
    };

    if path.iter().any(|name| {
        let mut parts = Path::new(name).components();
        !matches!(parts.next(), Some(Component::Normal(part)) if part == OsStr::new(name))
            || parts.next().is_some()
            || name.contains('\0')
    }) {
        return Err("只能開啟授權目錄內的子目錄".into());
    }

    // 逐層以已開啟的目錄定位，避免檢查後路徑被替換成符號連結。
    let mut folder = File::open("/").map_err(|_| "無法開啟授權目錄")?;
    let authorized = root
        .path()
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        });
    let mut segments = authorized.chain(path.iter().map(OsStr::new)).peekable();
    while let Some(segment) = segments.next() {
        let mode = if directory || segments.peek().is_some() {
            libc::O_DIRECTORY
        } else {
            libc::O_NONBLOCK
        };
        let name = CString::new(segment.as_bytes()).map_err(|_| "目錄名稱無效")?;
        // name 以 NUL 結尾，folder 的描述符在此呼叫期間仍有效。
        let fd = unsafe {
            libc::openat(
                folder.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | mode | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("無法開啟這個目錄，請確認授權或重新整理".into());
        }
        // openat 成功傳回由這個 File 唯一持有的描述符。
        folder = unsafe { File::from_raw_fd(fd) };
    }

    Ok(folder)
}

#[cfg(target_os = "linux")]
pub(crate) fn create_directory(
    root: &AuthorizedRoot,
    path: &[String],
    name: &str,
) -> Result<(), String> {
    use std::{ffi::CString, os::fd::AsRawFd};
    if name.is_empty()
        || name.trim() != name
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.contains(['/', '\\', '\0'])
    {
        return Err("資料夾名稱無效，請輸入單一名稱".into());
    }
    let parent = open_relative(root, path, true)?;
    let name = CString::new(name).map_err(|_| "資料夾名稱無效")?;
    // 由已開啟的授權目錄建立新項目，同名檔案或連結一律拒絕。
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
    if result != 0 {
        return Err(
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists {
                "已有同名項目，請使用其他名稱".into()
            } else {
                "無法建立資料夾，請檢查授權與磁碟空間".into()
            },
        );
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn create_directory(_: &AuthorizedRoot, _: &[String], _: &str) -> Result<(), String> {
    Err("這個平台目前無法安全建立授權資料夾".into())
}

#[cfg(target_os = "linux")]
pub(crate) fn browse(root: &AuthorizedRoot, path: &[String]) -> Result<DirectoryListing, String> {
    use std::{fs, os::fd::AsRawFd, time::UNIX_EPOCH};
    let folder = open_relative(root, path, true)?;
    let stable = format!("/proc/self/fd/{}", folder.as_raw_fd());
    let mut entries = Vec::new();
    let mut skipped = 0;
    for entry in fs::read_dir(stable).map_err(|_| "無法列出目錄內容")? {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let Ok(name) = entry.file_name().into_string() else {
            skipped += 1;
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            skipped += 1;
            continue;
        };
        if !metadata.is_file() && !metadata.is_dir() {
            skipped += 1;
            continue;
        }
        entries.push(DirectoryEntry {
            name,
            is_directory: metadata.is_dir(),
            size_bytes: metadata.is_file().then(|| metadata.len()),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_secs()),
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(DirectoryListing { entries, skipped })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn browse(_: &AuthorizedRoot, _: &[String]) -> Result<DirectoryListing, String> {
    Err("這個平台目前無法安全瀏覽授權目錄".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComparisonRow {
    original: Vec<String>,
    proposed: Vec<String>,
    size_bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct DirectoryComparison {
    original: Vec<ComparisonRow>,
    organized: Vec<ComparisonRow>,
    duplicates: Vec<ComparisonRow>,
    skipped: usize,
}

#[cfg(target_os = "linux")]
pub(crate) fn compare(
    root: &AuthorizedRoot,
    path: &[String],
) -> Result<DirectoryComparison, String> {
    use std::{collections::BTreeMap, path::Path};
    let mut result = DirectoryComparison {
        original: Vec::new(),
        organized: Vec::new(),
        duplicates: Vec::new(),
        skipped: 0,
    };
    let mut pending = vec![Vec::<String>::new()];
    let mut sizes = BTreeMap::<u64, Vec<usize>>::new();
    while let Some(relative) = pending.pop() {
        let target: Vec<_> = path.iter().chain(&relative).cloned().collect();
        let listing = match browse(root, &target) {
            Ok(listing) => listing,
            Err(message) if relative.is_empty() => return Err(message),
            Err(_) => {
                result.skipped += 1;
                continue;
            }
        };
        result.skipped += listing.skipped;
        for item in listing.entries {
            let mut original = relative.clone();
            original.push(item.name.clone());
            if item.is_directory {
                pending.push(original);
                continue;
            }
            let Some(size_bytes) = item.size_bytes else {
                continue;
            };
            let extension = Path::new(&item.name)
                .extension()
                .and_then(|part| part.to_str())
                .filter(|part| !part.is_empty())
                .map(|part| part.to_lowercase())
                .unwrap_or_else(|| "無副檔名".into());
            let mut proposed = vec![extension];
            proposed.extend(original.clone());
            result.organized.push(ComparisonRow {
                original: original.clone(),
                proposed,
                size_bytes,
            });
            sizes
                .entry(size_bytes)
                .or_default()
                .push(result.original.len());
            result.original.push(ComparisonRow {
                original,
                proposed: Vec::new(),
                size_bytes,
            });
        }
    }

    for (size_bytes, rows) in sizes.into_iter().filter(|(_, group)| group.len() > 1) {
        let mut digests = BTreeMap::<String, Vec<usize>>::new();
        for index in rows {
            let original = &result.original[index].original;
            let target: Vec<_> = path.iter().chain(original).cloned().collect();
            match current_digest(root, &target, size_bytes) {
                Ok(digest) => digests.entry(digest).or_default().push(index),
                Err(_) => result.skipped += 1,
            }
        }
        for (digest, group) in digests.into_iter().filter(|(_, group)| group.len() > 1) {
            for index in group {
                let original = result.original[index].original.clone();
                let mut proposed = vec![digest.clone()];
                proposed.extend(original.clone());
                result.duplicates.push(ComparisonRow {
                    original,
                    proposed,
                    size_bytes,
                });
            }
        }
    }

    result
        .original
        .sort_by(|left, right| left.original.cmp(&right.original));
    result
        .organized
        .sort_by(|left, right| left.proposed.cmp(&right.proposed));
    result
        .duplicates
        .sort_by(|left, right| left.proposed.cmp(&right.proposed));
    Ok(result)
}

#[cfg(target_os = "linux")]
fn current_digest(root: &AuthorizedRoot, path: &[String], size: u64) -> Result<String, String> {
    let file = open_relative(root, path, false)?;
    let before = file.metadata().map_err(|_| "無法核對檔案")?;
    if !before.is_file() || before.len() != size {
        return Err("檔案已變更".into());
    }
    let digest = crate::file_organizer::sha256_reader(&file).map_err(|_| "無法讀取檔案")?;
    let after = file.metadata().map_err(|_| "無法核對檔案")?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("檔案已變更".into());
    }
    Ok(digest)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn compare(_: &AuthorizedRoot, _: &[String]) -> Result<DirectoryComparison, String> {
    Err("這個平台目前無法安全比對授權目錄".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn directory_creation_is_atomic_and_keeps_the_authorized_boundary() {
        let path = std::env::temp_dir().join(format!("pet-mkdir-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("子目錄")).unwrap();
        symlink("/tmp", path.join("越權連結")).unwrap();
        let root = AuthorizedRoot::new(&path).unwrap();
        for invalid in ["", ".", "..", "../外部", "/tmp", "a/b", "a\\b", " a", "a\0"] {
            assert!(create_directory(&root, &[], invalid).is_err());
        }
        assert!(create_directory(&root, &["越權連結".into()], "不得建立").is_err());
        let create = || {
            let root = AuthorizedRoot::new(&path).unwrap();
            create_directory(&root, &["子目錄".into()], "並行資料夾")
        };
        let results = std::thread::scope(|scope| {
            let a = scope.spawn(&create);
            let b = scope.spawn(&create);
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(path.join("子目錄/並行資料夾").is_dir());
        fs::write(path.join("檔案"), b"keep").unwrap();
        assert!(create_directory(&root, &[], "檔案").is_err());
        assert_eq!(fs::read(path.join("檔案")).unwrap(), b"keep");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn browsing_keeps_paths_relative_and_refuses_links_and_traversal() {
        let root = std::env::temp_dir().join(format!("pet-browse-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("子目錄")).unwrap();
        fs::write(root.join("子目錄").join("檔案.txt"), b"abc").unwrap();
        symlink("/etc", root.join("外部")).unwrap();
        let granted = AuthorizedRoot::new(&root).unwrap();
        let home = browse(&granted, &[]).unwrap();
        assert_eq!(home.skipped, 1);
        assert_eq!(home.entries[0].name, "子目錄");
        assert!(home.entries[0].is_directory);
        let next = browse(&granted, &["子目錄".into()]).unwrap();
        assert_eq!(next.entries[0].size_bytes, Some(3));
        for input in ["..", ".", "/etc", "子目錄/../外部", "外部", "子目錄/"] {
            assert!(browse(&granted, &[input.into()]).is_err(), "{input}");
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn comparison_preserves_origin_and_groups_only_identical_content() {
        let root = std::env::temp_dir().join(format!("pet-compare-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("子")).unwrap();
        for (path, contents) in [
            ("甲.TXT", b"abc"),
            ("子/甲.txt", b"abc"),
            ("乙.txt", b"xyz"),
            ("沒有副檔名", b"z00"),
        ] {
            fs::write(root.join(path), contents).unwrap();
        }
        symlink("/etc", root.join("外部")).unwrap();
        let granted = AuthorizedRoot::new(&root).unwrap();
        let result = compare(&granted, &[]).unwrap();
        assert_eq!(result.original.len(), 4);
        assert_eq!(result.organized.len(), 4);
        assert_eq!(result.duplicates.len(), 2);
        assert_eq!(
            result.duplicates[0].proposed[0],
            result.duplicates[1].proposed[0]
        );
        assert!(
            result
                .organized
                .iter()
                .any(|r| r.proposed == ["txt", "子", "甲.txt"])
        );
        assert!(
            result
                .organized
                .iter()
                .any(|r| r.proposed == ["無副檔名", "沒有副檔名"])
        );
        assert_eq!(result.skipped, 1);
        assert_eq!(compare(&granted, &["子".into()]).unwrap().original.len(), 1);
        assert!(compare(&granted, &["外部".into()]).is_err());
        assert_eq!(fs::read(root.join("乙.txt")).unwrap(), b"xyz");
        fs::remove_dir_all(root).unwrap();
    }
}
