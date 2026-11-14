use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use crate::{
    crypto::hash::{FileId, hash_file},
    file::{
        chunker::{PlainChunk, read_full_or_eof},
        descriptor::FileEntry,
        error::FileError,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSourceFile {
    pub entry: FileEntry,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageLayout {
    pub name: String,
    pub root_path: PathBuf,
    pub total_size: u64,
    pub files: Vec<PackageSourceFile>,
}

impl PackageLayout {
    #[must_use]
    pub fn descriptor_files(&self) -> Vec<FileEntry> {
        self.files.iter().map(|file| file.entry.clone()).collect()
    }
}

pub fn collect_package_layout(input: impl AsRef<Path>) -> Result<PackageLayout, FileError> {
    let input = input.as_ref();
    let metadata = fs::metadata(input)?;

    if metadata.is_file() {
        return collect_single_file_layout(input, metadata.len());
    }

    if metadata.is_dir() {
        return collect_directory_layout(input);
    }

    Err(FileError::InvalidPackageInput(input.to_path_buf()))
}

pub fn read_package_chunks(
    layout: &PackageLayout,
    chunk_size: usize,
) -> Result<Vec<PlainChunk>, FileError> {
    if chunk_size == 0 {
        return Err(FileError::InvalidChunkSize(chunk_size));
    }

    let mut chunks = Vec::new();
    let mut next_index = Some(0_u32);
    let mut chunk_buffer = Vec::with_capacity(chunk_size);
    let mut read_buffer = [0_u8; 64 * 1024];
    let is_directory_package = layout.root_path.is_dir();

    for source_file in &layout.files {
        if is_directory_package
            && fs::symlink_metadata(&source_file.source_path)?
                .file_type()
                .is_symlink()
        {
            return Err(FileError::InvalidPackageInput(
                source_file.source_path.clone(),
            ));
        }

        let mut file = File::open(&source_file.source_path)?;
        let mut file_hasher = blake3::Hasher::new();
        let mut bytes_read = 0_u64;

        loop {
            let read = read_full_or_eof(&mut file, &mut read_buffer)?;
            if read == 0 {
                break;
            }

            bytes_read = bytes_read
                .checked_add(read as u64)
                .ok_or(FileError::PackageTooLarge)?;
            if bytes_read > source_file.entry.size {
                return Err(FileError::Io(std::io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "source file `{}` grew while packaging chunks",
                        source_file.source_path.display()
                    ),
                )));
            }
            file_hasher.update(&read_buffer[..read]);

            let mut remaining = &read_buffer[..read];
            while !remaining.is_empty() {
                let free = chunk_size - chunk_buffer.len();
                let take = free.min(remaining.len());

                chunk_buffer.extend_from_slice(&remaining[..take]);
                remaining = &remaining[take..];

                if chunk_buffer.len() == chunk_size {
                    let index = next_index.ok_or(FileError::TooManyChunks)?;
                    next_index = index.checked_add(1);
                    let data = std::mem::replace(&mut chunk_buffer, Vec::with_capacity(chunk_size));
                    chunks.push(PlainChunk { index, data });
                }
            }
        }

        if bytes_read != source_file.entry.size {
            return Err(FileError::Io(std::io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "source file `{}` shrank while packaging chunks (expected {} bytes, read {bytes_read})",
                    source_file.source_path.display(),
                    source_file.entry.size
                ),
            )));
        }

        let actual_hash = FileId(*file_hasher.finalize().as_bytes());
        if actual_hash != source_file.entry.blake3_hash {
            return Err(FileError::Io(std::io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "source file `{}` changed while packaging chunks",
                    source_file.source_path.display()
                ),
            )));
        }
    }

    if !chunk_buffer.is_empty() {
        let index = next_index.ok_or(FileError::TooManyChunks)?;
        chunks.push(PlainChunk {
            index,
            data: chunk_buffer,
        });
    }

    Ok(chunks)
}

fn collect_single_file_layout(path: &Path, size: u64) -> Result<PackageLayout, FileError> {
    let name = file_name(path);
    let entry = FileEntry {
        path: name.clone(),
        size,
        offset: 0,
        blake3_hash: hash_file(path)?,
    };

    Ok(PackageLayout {
        name,
        root_path: path.to_path_buf(),
        total_size: size,
        files: vec![PackageSourceFile {
            entry,
            source_path: path.to_path_buf(),
        }],
    })
}

fn collect_directory_layout(root: &Path) -> Result<PackageLayout, FileError> {
    let mut paths = Vec::new();
    collect_regular_files(root, &mut paths)?;
    paths.sort();

    if paths.is_empty() {
        return Err(FileError::EmptyPackage);
    }

    let mut files = Vec::with_capacity(paths.len());
    let mut seen_paths = BTreeSet::new();
    let mut offset = 0_u64;

    for path in paths {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FileError::InvalidPackageInput(path));
        }
        let size = metadata.len();
        let package_path = relative_package_path(root, &path)?;
        if !seen_paths.insert(package_path.clone()) {
            return Err(FileError::InvalidPackageInput(path));
        }
        let entry = FileEntry {
            path: package_path,
            size,
            offset,
            blake3_hash: hash_file(&path)?,
        };

        files.push(PackageSourceFile {
            entry,
            source_path: path,
        });
        offset = offset.checked_add(size).ok_or(FileError::PackageTooLarge)?;
    }

    Ok(PackageLayout {
        name: file_name(root),
        root_path: root.to_path_buf(),
        total_size: offset,
        files,
    })
}

fn collect_regular_files(dir: &Path, output: &mut Vec<PathBuf>) -> Result<(), FileError> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();

        if file_type.is_dir() {
            collect_regular_files(&path, output)?;
        } else if file_type.is_file() {
            output.push(path);
        }
    }

    Ok(())
}

fn relative_package_path(root: &Path, path: &Path) -> Result<String, FileError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| FileError::PathOutsideRoot {
            path: path.to_path_buf(),
            root: root.to_path_buf(),
        })?;

    let mut segments = Vec::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(FileError::InvalidPackageInput(path.to_path_buf()));
        };
        let Some(segment) = part.to_str() else {
            return Err(FileError::InvalidPackageInput(path.to_path_buf()));
        };
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains(['/', '\\', '\0'])
        {
            return Err(FileError::InvalidPackageInput(path.to_path_buf()));
        }
        segments.push(segment);
    }

    if segments.is_empty() {
        return Err(FileError::InvalidPackageInput(path.to_path_buf()));
    }

    Ok(segments.join("/"))
}

fn file_name(path: &Path) -> String {
    if let Some(name) = path.file_name() {
        return name.to_string_lossy().into_owned();
    }

    if let Ok(canonical) = fs::canonicalize(path)
        && let Some(name) = canonical.file_name()
    {
        return name.to_string_lossy().into_owned();
    }

    "unnamed".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir_name(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
    }

    #[test]
    fn collects_directory_files_with_stable_offsets() {
        let root = temp_dir_name("package-layout");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("b.txt"), b"bb").unwrap();
        fs::write(root.join("a.txt"), b"aaa").unwrap();
        fs::write(root.join("nested/c.txt"), b"c").unwrap();

        let layout = collect_package_layout(&root).unwrap();
        let files = layout.descriptor_files();

        assert_eq!(layout.total_size, 6);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].offset, 0);
        assert_eq!(files[1].path, "b.txt");
        assert_eq!(files[1].offset, 3);
        assert_eq!(files[2].path, "nested/c.txt");
        assert_eq!(files[2].offset, 5);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_chunks_cross_file_boundaries() {
        let root = temp_dir_name("package-chunks");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"abc").unwrap();
        fs::write(root.join("b.txt"), b"defg").unwrap();

        let layout = collect_package_layout(&root).unwrap();
        let chunks = read_package_chunks(&layout, 4).unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].data, b"abcd");
        assert_eq!(chunks[1].index, 1);
        assert_eq!(chunks[1].data, b"efg");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_chunks_detects_modified_source_file_between_layout_and_read() {
        let root = temp_dir_name("package-toctou");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("a.txt");
        fs::write(&file, b"abcd").unwrap();

        let layout = collect_package_layout(&root).unwrap();
        fs::write(&file, b"wxyz").unwrap();

        assert!(read_package_chunks(&layout, 4).is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
