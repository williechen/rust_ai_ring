use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::BufReader;
use std::io::Error;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileOrganizerError {
    #[error("desktop pet must not run as root")]
    RootProcess,

    #[error("authorized root does not exist or cannot be resolved: {0}")]
    InvalidRoot(String),

    #[error("authorized root is not a directory")]
    RootNotDirectory,

    #[error("file item is no longer a regular file: {0}")]
    UnsafeItem(String),

    #[error("file item changed after scan: {0}")]
    ItemChanged(String),

    #[error("filesystem scan failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub id: String,
    pub relative_path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SizeCandidateGroup {
    pub size_bytes: u64,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanWarning {
    pub relative_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreview {
    pub files: Vec<FilePreview>,
    pub duplicate_size_candidates: Vec<SizeCandidateGroup>,
    pub warnings: Vec<ScanWarning>,
}

fn reject_root_euid(euid: u32) -> Result<(), FileOrganizerError> {
    if euid == 0 {
        Err(FileOrganizerError::RootProcess)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn ensure_non_root_process() -> Result<(), FileOrganizerError> {
    let euid = unsafe { libc::geteuid() };
    reject_root_euid(euid)
}

#[cfg(not(unix))]
fn ensure_non_root_process() -> Result<(), FileOrganizerError> {
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AuthorizedRoot {
    canonical: PathBuf,
}

impl AuthorizedRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, FileOrganizerError> {
        ensure_non_root_process()?;

        let canonical = fs::canonicalize(path.as_ref())
            .map_err(|error| FileOrganizerError::InvalidRoot(error.to_string()))?;

        if !canonical.is_dir() {
            return Err(FileOrganizerError::RootNotDirectory);
        }

        Ok(Self { canonical })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.canonical
    }
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn scan_dir(
    root: &Path,
    current: &Path,
    files: &mut Vec<FilePreview>,
    warnings: &mut Vec<ScanWarning>,
) -> Result<(), FileOrganizerError> {
    let entries = fs::read_dir(current)?;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(ScanWarning {
                    relative_path: ".".to_string(),
                    message: error.to_string(),
                });
                continue;
            }
        };

        let path = entry.path();

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(ScanWarning {
                    relative_path: display_relative(root, &path),
                    message: error.to_string(),
                });
                continue;
            }
        };

        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if let Err(error) = scan_dir(root, &path, files, warnings) {
                warnings.push(ScanWarning {
                    relative_path: display_relative(root, &path),
                    message: error.to_string(),
                });
            }
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        files.push(FilePreview {
            id: String::new(),
            relative_path: display_relative(root, &path),
            size_bytes: metadata.len(),
        });
    }

    Ok(())
}

fn build_size_candidates(files: &[FilePreview]) -> Vec<SizeCandidateGroup> {
    let mut by_size: BTreeMap<u64, Vec<String>> = BTreeMap::new();

    for file in files {
        by_size
            .entry(file.size_bytes)
            .or_default()
            .push(file.id.clone());
    }

    by_size
        .into_iter()
        .filter_map(|(size_bytes, item_ids)| {
            (item_ids.len() >= 2).then_some(SizeCandidateGroup {
                size_bytes,
                item_ids,
            })
        })
        .collect()
}

pub fn scan(root: &AuthorizedRoot) -> Result<FileScan, FileOrganizerError> {
    ensure_non_root_process()?;

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut by_digest: BTreeMap<String, ScannedFile> = BTreeMap::new();

    scan_dir(root.path(), root.path(), &mut files, &mut warnings)?;

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    for (index, file) in files.iter_mut().enumerate() {
        file.id = format!("item-{index:06}");
        by_digest.insert(
            file.id.clone(),
            ScannedFile {
                id: file.id.clone(),
                path: PathBuf::from(root.path()).join(&file.relative_path),
                size_bytes: file.size_bytes,
            },
        );
    }

    let duplicate_size_candidates = build_size_candidates(&files);

    Ok(FileScan {
        preview: ScanPreview {
            files,
            duplicate_size_candidates,
            warnings,
        },
        items: by_digest,
    })
}

pub fn scan_preview(root: &AuthorizedRoot) -> Result<ScanPreview, FileOrganizerError> {
    Ok(scan(root)?.preview)
}

#[derive(Debug, Clone)]
struct ScannedFile {
    id: String,
    path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug)]
pub struct FileScan {
    pub preview: ScanPreview,
    items: BTreeMap<String, ScannedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub digest_sha256: String,
    pub size_bytes: u64,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePreview {
    pub groups: Vec<DuplicateGroup>,
    pub warnings: Vec<ScanWarning>,
}

fn revalidate_scanned_file(item: &ScannedFile) -> Result<(), FileOrganizerError> {
    ensure_non_root_process()?;

    let metadata = fs::symlink_metadata(&item.path)?;

    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(FileOrganizerError::UnsafeItem(item.id.clone()));
    }

    if metadata.len() != item.size_bytes {
        return Err(FileOrganizerError::ItemChanged(item.id.clone()));
    }

    let canonical = fs::canonicalize(&item.path)?;

    if canonical != item.path {
        return Err(FileOrganizerError::UnsafeItem(item.id.clone()));
    }

    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Error> {
    sha256_reader(fs::File::open(path)?)
}

pub(crate) fn sha256_reader(file: impl Read) -> Result<String, Error> {
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();

    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Ok(hash)
}

pub fn duplicate_preview(scan: &FileScan) -> DuplicatePreview {
    let mut warnings = Vec::new();
    let mut groups = Vec::new();

    for size_group in &scan.preview.duplicate_size_candidates {
        let mut by_digest: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for item_id in &size_group.item_ids {
            let Some(item) = scan.items.get(item_id) else {
                warnings.push(ScanWarning {
                    relative_path: "<opaque-item>".to_string(),
                    message: format!("missing scan item: {item_id}"),
                });
                continue;
            };

            if let Err(error) = revalidate_scanned_file(item) {
                warnings.push(ScanWarning {
                    relative_path: "<opaque-item>".to_string(),
                    message: error.to_string(),
                });
                continue;
            }

            match sha256_file(&item.path) {
                Ok(digest) => {
                    by_digest.entry(digest).or_default().push(item.id.clone());
                }
                Err(error) => {
                    warnings.push(ScanWarning {
                        relative_path: "<opaque-item>".to_string(),
                        message: error.to_string(),
                    });
                }
            }
        }

        for (digest_sha256, item_ids) in by_digest {
            if item_ids.len() >= 2 {
                groups.push(DuplicateGroup {
                    digest_sha256,
                    size_bytes: size_group.size_bytes,
                    item_ids,
                });
            }
        }
    }
    DuplicatePreview { groups, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos();

            let path = std::env::temp_dir()
                .join(format!("desktop-pet-{name}-{}-{nonce}", std::process::id()));

            fs::create_dir_all(&path).expect("create test dir");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn scans_regular_files_and_builds_size_candidates() {
        let sandbox = TestDir::new("scan");
        fs::write(sandbox.path.join("a.txt"), b"abc").unwrap();
        fs::write(sandbox.path.join("b.txt"), b"xyz").unwrap();
        fs::write(sandbox.path.join("c.txt"), b"longer").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let preview = scan_preview(&root).unwrap();

        assert_eq!(preview.files.len(), 3);
        assert_eq!(preview.duplicate_size_candidates.len(), 1);
        assert_eq!(preview.duplicate_size_candidates[0].size_bytes, 3);
        assert_eq!(preview.duplicate_size_candidates[0].item_ids.len(), 2);
    }

    #[test]
    fn ids_are_opaque_not_absolute_paths() {
        let sandbox = TestDir::new("opaque-id");
        fs::write(sandbox.path.join("secret.txt"), b"x").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let preview = scan_preview(&root).unwrap();

        let file = &preview.files[0];
        assert!(file.id.starts_with("item-"));
        assert!(
            !file
                .id
                .contains(&sandbox.path.to_string_lossy().to_string())
        );
        assert_eq!(file.relative_path, "secret.txt");
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlink_instead_of_following_it() {
        use std::os::unix::fs::symlink;

        let sandbox = TestDir::new("symlink");
        let outside = TestDir::new("outside");

        fs::write(outside.path.join("outside.txt"), b"secret").unwrap();
        symlink(&outside.path, sandbox.path.join("escape")).unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let preview = scan_preview(&root).unwrap();

        assert!(preview.files.is_empty());
    }

    #[test]
    fn rejects_file_as_authorized_root() {
        let sandbox = TestDir::new("root-file");
        let file = sandbox.path.join("not-a-dir.txt");
        fs::write(&file, b"x").unwrap();

        let error = AuthorizedRoot::new(file).unwrap_err();
        assert!(matches!(error, FileOrganizerError::RootNotDirectory));
    }

    #[test]
    fn rejects_root_euid() {
        assert!(matches!(
            reject_root_euid(0),
            Err(FileOrganizerError::RootProcess)
        ));
        assert!(reject_root_euid(1000).is_ok());
    }

    #[test]
    fn same_size_different_content_is_not_duplicate() {
        let sandbox = TestDir::new("same-size-different");
        fs::write(sandbox.path.join("a.txt"), b"abc").unwrap();
        fs::write(sandbox.path.join("b.txt"), b"xyz").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let scan = scan(&root).unwrap();
        let duplicates = duplicate_preview(&scan);

        assert!(duplicates.groups.is_empty());
    }

    #[test]
    fn same_content_is_grouped_as_duplicate() {
        let sandbox = TestDir::new("same-content");
        fs::write(sandbox.path.join("a.txt"), b"same").unwrap();
        fs::write(sandbox.path.join("b.txt"), b"same").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let scan = scan(&root).unwrap();
        let duplicates = duplicate_preview(&scan);

        assert_eq!(duplicates.groups.len(), 1);
        assert_eq!(duplicates.groups[0].size_bytes, 4);
        assert_eq!(duplicates.groups[0].item_ids.len(), 2);
    }

    #[test]
    fn unique_size_file_is_not_hashed_into_duplicate_group() {
        let sandbox = TestDir::new("unique-size");
        fs::write(sandbox.path.join("a.txt"), b"a").unwrap();
        fs::write(sandbox.path.join("b.txt"), b"longer").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let scan = scan(&root).unwrap();
        let duplicates = duplicate_preview(&scan);

        assert!(duplicates.groups.is_empty());
    }

    #[test]
    fn duplicate_preview_warns_when_file_size_changes_after_scan() {
        let sandbox = TestDir::new("changed-size");

        let a = sandbox.path.join("a.txt");
        let b = sandbox.path.join("b.txt");

        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let scan = scan(&root).unwrap();

        fs::write(&b, b"changed-size").unwrap();

        let duplicates = duplicate_preview(&scan);

        assert!(duplicates.groups.is_empty());
        assert_eq!(duplicates.warnings.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_preview_warns_when_file_is_replaced_by_symlink() {
        use std::os::unix::fs::symlink;

        let sandbox = TestDir::new("replaced-symlink");
        let outside = TestDir::new("replaced-symlink-outside");

        let a = sandbox.path.join("a.txt");
        let b = sandbox.path.join("b.txt");
        let outside_file = outside.path.join("outside.txt");

        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();
        fs::write(&outside_file, b"same").unwrap();

        let root = AuthorizedRoot::new(&sandbox.path).unwrap();
        let scan = scan(&root).unwrap();

        fs::remove_file(&b).unwrap();
        symlink(&outside_file, &b).unwrap();

        let duplicates = duplicate_preview(&scan);

        assert!(duplicates.groups.is_empty());
        assert_eq!(duplicates.warnings.len(), 1);
    }
}
