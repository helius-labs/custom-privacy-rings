//! One place the cli touches a path and names it in the failure.

use std::{fs, io, path::Path};

use anyhow::{anyhow, Context as _, Result};
use custom_ring_sdk::secret_json_body;
use solana_keypair::Keypair;
use zeroize::Zeroizing;
use zolana_secret_file::{create_new, read_private, Access, Visibility};

const MAX_KEYPAIR_FILE_BYTES: u64 = 4096;

pub fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}

pub fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("cannot write {}", path.display()))
}

/// A missing directory is already removed.
pub fn remove_dir_all(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(anyhow::Error::new(error).context(format!("cannot write {}", path.display())))
        }
        _ => Ok(()),
    }
}

pub fn read_keypair(path: &Path) -> Result<Keypair> {
    let text = read_private(path, Access::OwnerOnly, MAX_KEYPAIR_FILE_BYTES)
        .with_context(|| format!("cannot read keypair {}", path.display()))?;
    solana_keypair::read_keypair(&mut text.as_bytes())
        .map_err(|error| anyhow!("cannot read keypair {}, {error}", path.display()))
}

pub fn read_or_create_keypair(path: &Path) -> Result<Keypair> {
    match fs::symlink_metadata(path) {
        Ok(_) => read_keypair(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let keypair = Keypair::new();
            write_keypair(&keypair, path)?;
            Ok(keypair)
        }
        Err(error) => {
            Err(anyhow::Error::new(error)
                .context(format!("cannot read keypair {}", path.display())))
        }
    }
}

/// Owner-only and never over an existing file.
pub fn write_keypair(keypair: &Keypair, path: &Path) -> Result<()> {
    let secret = Zeroizing::new(keypair.to_bytes());
    let json = secret_json_body(&secret[..])?;
    create_new(path, json.as_bytes(), Visibility::Secret)
        .with_context(|| format!("cannot write keypair {}", path.display()))
}

#[cfg(test)]
mod tests {
    use solana_signer::Signer;

    use super::*;

    #[test]
    fn a_written_keypair_reads_back() {
        let dir = std::env::temp_dir().join(format!("ring-keypair-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sender.json");
        let keypair = Keypair::new();
        write_keypair(&keypair, &path).unwrap();
        assert_eq!(read_keypair(&path).unwrap().pubkey(), keypair.pubkey());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let link = dir.join("link.json");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read_keypair(&link).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_keypair(&path).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
