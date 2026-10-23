use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ConvertError;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Canonical input path and resolved destination path.
pub(crate) struct ResolvedPaths {
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
}

/// Resolve and validate the input/output pair before any output is created.
pub(crate) fn resolve_paths(input: &Path, output: &Path) -> Result<ResolvedPaths, ConvertError> {
    let input_metadata =
        fs::metadata(input).map_err(|err| ConvertError::InputOpen(input.to_path_buf(), err))?;
    if !input_metadata.is_file() {
        return Err(ConvertError::InputNotFile(input.to_path_buf()));
    }
    let input = input
        .canonicalize()
        .map_err(|err| ConvertError::InputOpen(input.to_path_buf(), err))?;

    match fs::symlink_metadata(output) {
        Ok(_) => {
            let resolved = output
                .canonicalize()
                .unwrap_or_else(|_| output.to_path_buf());
            if resolved == input {
                return Err(ConvertError::SamePath(output.to_path_buf()));
            }
            return Err(ConvertError::OutputExists(output.to_path_buf()));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(ConvertError::OutputInspect(output.to_path_buf(), err)),
    }

    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| ConvertError::OutputDirectoryMissing(parent.to_path_buf()))?;
    if !parent.is_dir() {
        return Err(ConvertError::OutputDirectoryMissing(parent));
    }
    let name = output
        .file_name()
        .ok_or_else(|| ConvertError::OutputDirectoryMissing(output.to_path_buf()))?;
    let output = parent.join(name);
    if output == input {
        return Err(ConvertError::SamePath(output));
    }

    Ok(ResolvedPaths { input, output })
}

/// Temporary output file in the destination directory.
///
/// The file is removed on drop unless [`publish`](Self::publish) has completed
/// successfully.
pub(crate) struct TempOutput {
    path: PathBuf,
    file: Option<File>,
    published: bool,
}

impl TempOutput {
    pub(crate) fn create(output: &Path) -> Result<Self, ConvertError> {
        let parent = output.parent().unwrap_or_else(|| Path::new("."));
        for _ in 0..64 {
            let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".aac-convert-{}-{sequence}.tmp",
                std::process::id()
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        published: false,
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(ConvertError::TempCreate(path, err)),
            }
        }
        Err(ConvertError::TempCreate(
            parent.join(".aac-convert-<exhausted>.tmp"),
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary output name space exhausted",
            ),
        ))
    }

    pub(crate) fn write_all(&mut self, bytes: &[u8]) -> Result<(), ConvertError> {
        let file = self
            .file
            .as_mut()
            .expect("temporary output is open until publish");
        file.write_all(bytes)
            .map_err(|err| ConvertError::TempWrite(self.path.clone(), err))
    }

    /// Seek within the temporary file (used by container finalization).
    pub(crate) fn seek(&mut self, position: SeekFrom) -> Result<u64, ConvertError> {
        let file = self
            .file
            .as_mut()
            .expect("temporary output is open until publish");
        file.seek(position)
            .map_err(|err| ConvertError::TempWrite(self.path.clone(), err))
    }

    /// Truncate or extend the temporary file (used when experimental gapless
    /// signalling rewrites the final `moov` box with a different size).
    pub(crate) fn set_len(&mut self, len: u64) -> Result<(), ConvertError> {
        let file = self
            .file
            .as_mut()
            .expect("temporary output is open until publish");
        file.set_len(len)
            .map_err(|err| ConvertError::TempWrite(self.path.clone(), err))
    }

    /// Read exactly `buf.len()` bytes at the current position (used to verify
    /// the `ftyp` layout before an experimental brand splice).
    pub(crate) fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ConvertError> {
        use std::io::Read as _;
        let file = self
            .file
            .as_mut()
            .expect("temporary output is open until publish");
        file.read_exact(buf)
            .map_err(|err| ConvertError::TempWrite(self.path.clone(), err))
    }

    /// Flush, sync, close, then publish by hard-linking the temporary file.
    ///
    /// The hard link atomically fails with `AlreadyExists` if the destination
    /// was created concurrently, so a competitor's output is never replaced.
    /// Hard-link-capable filesystems are required; there is deliberately no
    /// check-then-rename fallback.
    pub(crate) fn publish(mut self, output: &Path) -> Result<(), ConvertError> {
        let mut file = self
            .file
            .take()
            .expect("temporary output is open until publish");
        file.flush()
            .map_err(|err| ConvertError::Finalize(output.to_path_buf(), err))?;
        file.sync_all()
            .map_err(|err| ConvertError::Finalize(output.to_path_buf(), err))?;
        drop(file);

        match fs::hard_link(&self.path, output) {
            Ok(()) => {
                // The destination is already complete. Keep the temporary name
                // if unlinking fails so `Drop` can retry the cleanup.
                if fs::remove_file(&self.path).is_ok() {
                    self.published = true;
                }
                Ok(())
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                Err(ConvertError::OutputExists(output.to_path_buf()))
            }
            Err(err) => Err(ConvertError::Publish(output.to_path_buf(), err)),
        }
    }
}

impl Drop for TempOutput {
    fn drop(&mut self) {
        self.file.take();
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

    fn test_directory() -> PathBuf {
        for _ in 0..64 {
            let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "aac-convert-output-test-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&directory) {
                Ok(()) => return directory,
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("could not create test directory: {err}"),
            }
        }
        panic!("could not allocate a unique test directory");
    }

    fn temporary_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".aac-convert-"))
            })
            .collect()
    }

    #[test]
    fn concurrent_publication_never_replaces_the_winner() {
        let directory = test_directory();
        let output = directory.join("out.aac");
        let mut first = TempOutput::create(&output).unwrap();
        let mut second = TempOutput::create(&output).unwrap();
        first.write_all(b"first contender").unwrap();
        second.write_all(b"second contender").unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let first_output = output.clone();
        let second_output = output.clone();
        let first_barrier = Arc::clone(&barrier);
        let second_barrier = Arc::clone(&barrier);
        let first_thread = thread::spawn(move || {
            first_barrier.wait();
            first.publish(&first_output)
        });
        let second_thread = thread::spawn(move || {
            second_barrier.wait();
            second.publish(&second_output)
        });
        barrier.wait();

        let first_result = first_thread.join().unwrap();
        let second_result = second_thread.join().unwrap();
        let first_won = first_result.is_ok();
        assert_ne!(
            first_won,
            second_result.is_ok(),
            "exactly one publication must win: {first_result:?} / {second_result:?}"
        );
        let loser = if first_won {
            second_result
        } else {
            first_result
        };
        assert!(
            matches!(loser, Err(ConvertError::OutputExists(_))),
            "loser was not rejected as an existing output: {loser:?}"
        );

        let bytes = fs::read(&output).unwrap();
        assert_eq!(
            bytes,
            if first_won {
                b"first contender".as_slice()
            } else {
                b"second contender".as_slice()
            }
        );
        assert!(
            temporary_files(&directory).is_empty(),
            "a temporary file was left behind"
        );
        let _ = fs::remove_dir_all(&directory);
    }
}
