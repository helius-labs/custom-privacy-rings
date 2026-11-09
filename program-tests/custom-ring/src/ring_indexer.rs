//! The ring indexer each localnet bring-up starts beside Photon, and the Photon restart that
//! follows the bring-up's tree creation.

use std::{
    fs::{self, File},
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use spr_client::{Rpc, SolanaRpc};
use spr_test_utils::localnet::isolated_temp_path;

const BINARY_ENV: &str = "RING_INDEXER_BIN";
const PHOTON_BINARY_ENV: &str = "ZOLANA_PHOTON_BIN";
/// The block fetch width the zolana CLI gives a Photon in a process scope.
const PHOTON_BLOCK_FETCHES: &str = "8";
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(250);

pub struct RingIndexerLaunch<'a> {
    /// Clients read every indexer method here.
    pub indexer_url: &'a str,
    pub photon_url: &'a str,
    pub rpc_url: &'a str,
    pub log_dir: &'a Path,
}

impl RingIndexerLaunch<'_> {
    /// Replaces the indexer of an earlier bring-up, its index belongs to the stopped chain.
    pub fn spawn(self) -> Result<()> {
        let Self {
            indexer_url,
            photon_url,
            rpc_url,
            log_dir,
        } = self;
        if indexer_url.trim_end_matches('/') == photon_url.trim_end_matches('/') {
            bail!("set ZOLANA_INDEXER_URL to the ring indexer, {photon_url} serves Photon");
        }
        let binary = std::env::var_os(BINARY_ENV)
            .map(PathBuf::from)
            .with_context(|| format!("set {BINARY_ENV} to the ring indexer binary"))?;
        let port = port(indexer_url)?;
        stop_listener(port)?;
        let data = PathBuf::from(isolated_temp_path(&format!("zolana-ring-indexer-{port}")));
        match fs::remove_dir_all(&data) {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("clear {}", data.display()))
            }
            _ => {}
        }
        fs::create_dir_all(&data)?;
        fs::create_dir_all(log_dir)?;
        let log_path = log_dir.join("ring-indexer.log");
        let log = File::create(&log_path)?;
        Command::new(&binary)
            .arg("--db-url")
            .arg(format!(
                "sqlite://{}?mode=rwc",
                data.join("index.db").display()
            ))
            .args(["--rpc-url", rpc_url])
            .args(["--photon-url", photon_url])
            .args(["--port", &port.to_string()])
            .args(["--start-slot", "latest"])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()
            .with_context(|| format!("start {}", binary.display()))?;
        let deadline = Instant::now() + READY_TIMEOUT;
        while !ready(port) {
            if Instant::now() > deadline {
                bail!(
                    "the ring indexer on port {port} is not ready, see {}",
                    log_path.display()
                );
            }
            thread::sleep(POLL);
        }
        Ok(())
    }
}

/// Photon reads tree accounts once at startup and otherwise learns a tree only from an indexed
/// transaction touching it. A deposit proven against a tree no transaction touched yet asks for
/// its dummies' exclusion on a tree Photon refuses, so the bring-up restarts Photon after it
/// creates its trees.
pub struct PhotonRestart<'a> {
    pub photon_url: &'a str,
    pub rpc_url: &'a str,
    pub log_dir: &'a Path,
}

impl PhotonRestart<'_> {
    /// Restarts an exited Photon until it indexes the slot read after the tree creation.
    pub fn spawn(self) -> Result<()> {
        let Self {
            photon_url,
            rpc_url,
            log_dir,
        } = self;
        let binary = std::env::var_os(PHOTON_BINARY_ENV)
            .map(PathBuf::from)
            .with_context(|| format!("set {PHOTON_BINARY_ENV} to the Photon binary"))?;
        let port = port(photon_url)?;
        let created = SolanaRpc::new(rpc_url.to_owned())
            .get_slot()
            .context("read the slot of the tree creation")?;
        // The CLI's database directory for the port, Photon recreates its database there.
        let temp_dir = std::env::temp_dir().join(format!("zolana-photon-{port}"));
        fs::create_dir_all(&temp_dir)?;
        fs::create_dir_all(log_dir)?;
        let log_path = log_dir.join("photon-restart.log");
        let log = File::create(&log_path)?;
        let start = || {
            Command::new(&binary)
                .args(["--rpc-url", rpc_url])
                .args(["--port", &port.to_string()])
                .args(["--start-slot", "latest"])
                .args(["--max-concurrent-block-fetches", PHOTON_BLOCK_FETCHES])
                .env("TMPDIR", &temp_dir)
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log.try_clone()?)
                .spawn()
                .with_context(|| format!("start {}", binary.display()))
        };
        // Only the new Photon may answer the slot poll.
        stop_listener(port)?;
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut photon: Option<Child> = None;
        loop {
            if !matches!(
                photon.as_mut().map(Child::try_wait).transpose()?,
                Some(None)
            ) {
                photon = Some(start()?);
            }
            if indexed_slot(port).is_some_and(|slot| slot >= created) {
                return Ok(());
            }
            if Instant::now() > deadline {
                if let Some(mut photon) = photon {
                    let _ = photon.kill();
                }
                bail!(
                    "Photon on port {port} did not index slot {created}\n{}",
                    log_tail(&log_path)
                );
            }
            thread::sleep(POLL);
        }
    }
}

pub(crate) fn port(url: &str) -> Result<u16> {
    url.rsplit_once(':')
        .and_then(|(_, port)| port.trim_end_matches('/').parse().ok())
        .with_context(|| format!("{url} names no port"))
}

pub(crate) fn stop_listener(port: u16) -> Result<()> {
    let listeners = Command::new("lsof")
        .args(["-t", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .context("lsof is required to stop an earlier ring indexer")?;
    for pid in String::from_utf8_lossy(&listeners.stdout).split_whitespace() {
        Command::new("kill").arg(pid).status()?;
    }
    let deadline = Instant::now() + READY_TIMEOUT;
    while TcpStream::connect(("127.0.0.1", port)).is_ok() {
        if Instant::now() > deadline {
            bail!("port {port} stays busy");
        }
        thread::sleep(POLL);
    }
    Ok(())
}

/// Photon errors on `getIndexerSlot` until it stores its first block.
fn indexed_slot(port: u16) -> Option<u64> {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"getIndexerSlot"}"#;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let request = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut response = String::new();
    stream.write_all(request.as_bytes()).ok()?;
    stream.read_to_string(&mut response).ok()?;
    response_slot(&response)
}

fn response_slot(response: &str) -> Option<u64> {
    if !response.starts_with("HTTP/1.1 200") {
        return None;
    }
    let body = response.get(response.find('{')?..=response.rfind('}')?)?;
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("result")?
        .as_u64()
}

/// CI prints no restart log, the error carries its tail.
fn log_tail(path: &Path) -> String {
    const LINES: usize = 40;
    fs::read_to_string(path).map_or_else(
        |error| format!("{} is unreadable, {error}", path.display()),
        |log| {
            let lines: Vec<&str> = log.lines().collect();
            lines[lines.len().saturating_sub(LINES)..].join("\n")
        },
    )
}

fn ready(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let mut status = [0; 12];
    stream
        .write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .and_then(|()| stream.read_exact(&mut status))
        .is_ok_and(|()| status.ends_with(b" 200"))
}

#[cfg(test)]
mod tests {
    use super::response_slot;

    #[test]
    fn photon_is_ready_only_on_an_indexed_slot() {
        let reply = |status: &str, body: &str| {
            format!("HTTP/1.1 {status}\r\ncontent-length: 1\r\n\r\n{body}")
        };
        assert_eq!(
            response_slot(&reply("200 OK", r#"{"jsonrpc":"2.0","result":41,"id":1}"#)),
            Some(41)
        );
        // Photon's reply over an empty blocks table.
        let empty = r#"{"jsonrpc":"2.0","error":{"code":-32000,"message":"Internal server database error: Type Error: A null value was encountered while decoding \"slot\""},"id":1}"#;
        assert_eq!(response_slot(&reply("200 OK", empty)), None);
        assert_eq!(response_slot(&reply("503 Service Unavailable", "")), None);
    }
}
