use std::{fs::File, io::Read, path::Path};

use crate::file::error::FileError;

pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlainChunk {
    pub index: u32,
    pub data: Vec<u8>,
}

pub fn read_file_chunks(
    path: impl AsRef<Path>,
    chunk_size: usize,
) -> Result<Vec<PlainChunk>, FileError> {
    if chunk_size == 0 {
        return Err(FileError::InvalidChunkSize(chunk_size));
    }

    let mut file = File::open(path)?;
    let mut chunks = Vec::new();
    let mut index = 0_u32;

    loop {
        let mut buffer = vec![0_u8; chunk_size];
        let read = file.read(&mut buffer)?;

        if read == 0 {
            break;
        }

        buffer.truncate(read);
        chunks.push(PlainChunk {
            index,
            data: buffer,
        });
        index = index.checked_add(1).ok_or(FileError::TooManyChunks)?;
    }

    Ok(chunks)
}

pub fn join_chunks(chunks: &[PlainChunk]) -> Vec<u8> {
    let total_len = chunks.iter().map(|chunk| chunk.data.len()).sum();
    let mut output = Vec::with_capacity(total_len);

    let mut sorted = chunks.to_vec();
    sorted.sort_by_key(|chunk| chunk.index);

    for chunk in sorted {
        output.extend_from_slice(&chunk.data);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn temp_file_name(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
    }

    #[test]
    fn chunks_file_with_smaller_last_chunk() {
        let path = temp_file_name("chunker.bin");
        fs::write(&path, b"abcdefghi").unwrap();

        let chunks = read_file_chunks(&path, 4).unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].data, b"abcd");
        assert_eq!(chunks[1].data, b"efgh");
        assert_eq!(chunks[2].data, b"i");

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_zero_chunk_size() {
        let path = temp_file_name("zero.bin");
        fs::write(&path, b"abc").unwrap();

        assert!(read_file_chunks(&path, 0).is_err());

        fs::remove_file(path).unwrap();
    }
}
