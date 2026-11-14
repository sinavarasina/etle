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
    let mut next_index = Some(0_u32);

    loop {
        let mut buffer = vec![0_u8; chunk_size];
        let read = read_full_or_eof(&mut file, &mut buffer)?;

        if read == 0 {
            break;
        }

        let index = next_index.ok_or(FileError::TooManyChunks)?;
        next_index = index.checked_add(1);

        buffer.truncate(read);
        chunks.push(PlainChunk {
            index,
            data: buffer,
        });
    }

    Ok(chunks)
}

pub(crate) fn read_full_or_eof(
    reader: &mut impl Read,
    buffer: &mut [u8],
) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match reader.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(read) => total += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}

pub fn join_chunks(chunks: &[PlainChunk]) -> Vec<u8> {
    let total_len = chunks
        .iter()
        .try_fold(0_usize, |acc, chunk| acc.checked_add(chunk.data.len()))
        .expect("joined chunk size overflows usize");
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

    #[test]
    fn read_full_or_eof_reassembles_short_reads() {
        struct ShortReader {
            data: Vec<u8>,
            pos: usize,
        }

        impl Read for ShortReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos == self.data.len() {
                    return Ok(0);
                }
                if self.pos == 1 {
                    self.pos += 1;
                    buf[0] = self.data[1];
                    return Ok(1);
                }
                let take = (self.data.len() - self.pos).min(2).min(buf.len());
                buf[..take].copy_from_slice(&self.data[self.pos..self.pos + take]);
                self.pos += take;
                Ok(take)
            }
        }

        let mut reader = ShortReader {
            data: b"abcdef".to_vec(),
            pos: 0,
        };
        let mut buf = [0_u8; 4];
        assert_eq!(read_full_or_eof(&mut reader, &mut buf).unwrap(), 4);
        assert_eq!(&buf, b"abcd");
        assert_eq!(read_full_or_eof(&mut reader, &mut buf).unwrap(), 2);
        assert_eq!(&buf[..2], b"ef");
    }
}
