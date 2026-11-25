pub mod process;
pub mod release;

use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AtomicFileMode {
    OwnerOnly,
    DefaultUmask,
}

pub(crate) struct AtomicWrite<'a> {
    pub path: &'a Path,
    pub bytes: &'a [u8],
    pub mode: AtomicFileMode,
}

struct AllocatedTemp {
    path: PathBuf,
    file: fs::File,
}

/// Every zolana tool shares this directory and its release cache.
pub fn config_dir() -> PathBuf {
    match (env::var_os("ZOLANA_CONFIG_DIR"), env::var_os("HOME")) {
        (Some(path), _) => PathBuf::from(path),
        (None, Some(home)) => PathBuf::from(home).join(".config").join("zolana"),
        (None, None) => PathBuf::from(".zolana"),
    }
}

impl AtomicWrite<'_> {
    pub(crate) fn write(self) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = self.path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "target path names no file")
        })?;
        let pid = std::process::id();
        let AllocatedTemp {
            path: tmp_path,
            mut file,
        } = (0..64)
            .find_map(|_| {
                let seq = NEXT.fetch_add(1, Ordering::Relaxed);
                let tmp_path = self
                    .path
                    .with_file_name(format!(".{}.{pid}.{seq}.tmp", name.to_string_lossy()));
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NOFOLLOW);
                    if self.mode == AtomicFileMode::OwnerOnly {
                        options.mode(0o600);
                    }
                }
                match options.open(&tmp_path) {
                    Ok(file) => Some(Ok(AllocatedTemp {
                        path: tmp_path,
                        file,
                    })),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .unwrap_or_else(|| {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "cannot allocate temp file",
                ))
            })?;
        let written = file
            .write_all(self.bytes)
            .and_then(|()| file.sync_all())
            .and_then(|()| {
                drop(file);
                fs::rename(&tmp_path, self.path)
            });
        if let Err(error) = written {
            let _ = fs::remove_file(&tmp_path);
            return Err(error);
        }
        Ok(())
    }
}
