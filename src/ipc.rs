use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    #[default]
    Toggle,
    Show,
    Hide,
    Quit,
    Doctor,
    List,
    Run,
    Daemon,
    Install,
    Uninstall,
    Ping,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub command: Command,
    #[serde(default)]
    pub argument: String,
}
impl Request {
    pub fn new(command: Command) -> Self {
        Self {
            command,
            argument: String::new(),
        }
    }
}

pub fn send(runtime: &Path, message: &Request) -> Result<()> {
    let mut stream = UnixStream::connect(runtime.join("arch-cast.sock"))?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    serde_json::to_writer(&mut stream, message)?;
    stream.write_all(b"\n")?;
    let mut reply = String::new();
    BufReader::new(stream).take(1024).read_line(&mut reply)?;
    if reply.trim() != "ok" {
        bail!("resident did not acknowledge request");
    }
    Ok(())
}

/// Keep the lock open for the resident's whole lifetime, including windowless periods.
pub struct Server {
    _lock: File,
    pub listener: UnixListener,
    socket: PathBuf,
}
impl Server {
    pub fn bind(runtime: &Path) -> Result<Option<Self>> {
        if !runtime.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(runtime)?;
        }
        let metadata = fs::metadata(runtime)?;
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        if metadata.uid() != unsafe { libc::getuid() } || metadata.permissions().mode() & 0o077 != 0
        {
            bail!(
                "runtime directory must be owned by this user and mode 0700: {}",
                runtime.display()
            );
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(runtime.join("arch-cast.lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error.into());
        }
        let socket = runtime.join("arch-cast.sock");
        match fs::remove_file(&socket) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener =
            UnixListener::bind(&socket).with_context(|| format!("bind {}", socket.display()))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        Ok(Some(Self {
            _lock: lock,
            listener,
            socket,
        }))
    }
    pub fn start(&self) -> Result<async_channel::Receiver<Request>> {
        let listener = self.listener.try_clone()?;
        let (tx, rx) = async_channel::bounded(32);
        std::thread::Builder::new()
            .name("launcher-ipc".into())
            .spawn(move || {
                for incoming in listener.incoming() {
                    let Ok(mut stream) = incoming else {
                        break;
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                    let mut source = Vec::new();
                    let read = BufReader::new(&stream)
                        .take(16385)
                        .read_until(b'\n', &mut source);
                    if read.is_err() || source.len() > 16384 || !source.ends_with(b"\n") {
                        continue;
                    }
                    let Ok(request) = serde_json::from_slice::<Request>(&source) else {
                        continue;
                    };
                    if tx.try_send(request).is_ok() {
                        let _ = stream.write_all(b"ok\n");
                    }
                }
            })?;
        Ok(rx)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_instance_permissions_and_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        let server = Server::bind(&runtime).unwrap().unwrap();
        assert!(Server::bind(&runtime).unwrap().is_none());
        assert_eq!(
            fs::metadata(runtime.join("arch-cast.sock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let rx = server.start().unwrap();
        send(&runtime, &Request::new(Command::Show)).unwrap();
        assert_eq!(rx.recv_blocking().unwrap().command, Command::Show);
        let mut stream = UnixStream::connect(runtime.join("arch-cast.sock")).unwrap();
        stream.write_all(b"{bad}\n").unwrap();
        drop(stream);
        send(&runtime, &Request::new(Command::Hide)).unwrap();
        assert_eq!(rx.recv_blocking().unwrap().command, Command::Hide);
        drop(server);
        assert!(!runtime.join("arch-cast.sock").exists());
        assert!(Server::bind(&runtime).unwrap().is_some());
    }
}
