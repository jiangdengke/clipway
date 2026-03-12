use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use fs2::FileExt;

use crate::clipboard::{self, ClipboardEvent};
use crate::paths;
use crate::storage::Storage;

const WATCHER_RESTART_DELAY: Duration = Duration::from_secs(1);

pub fn ensure_running() -> Result<bool> {
    if is_running()? {
        return Ok(false);
    }

    spawn_detached_subcommand("daemon")?;
    Ok(true)
}

pub fn spawn_detached_subcommand(subcommand: &str) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to determine clipway binary path")?;
    let mut command = Command::new(current_exe);
    command
        .arg(subcommand)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        });
    }

    command
        .spawn()
        .with_context(|| format!("failed to spawn detached clipway subcommand `{subcommand}`"))?;

    Ok(())
}

pub fn is_running() -> Result<bool> {
    let file = open_lock_file()?;

    match file.try_lock_exclusive() {
        Ok(()) => {
            file.unlock()
                .context("failed to unlock daemon lock probe file")?;
            Ok(false)
        }
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
        Err(err) => Err(err).context("failed to inspect daemon lock state"),
    }
}

pub fn run_foreground() -> Result<()> {
    let _lock = DaemonLock::acquire()?;
    let mut storage = Storage::open()?;

    loop {
        let (sender, receiver) = mpsc::channel();
        clipboard::spawn_text_watcher(sender.clone());
        clipboard::spawn_image_watcher(sender);

        loop {
            match receiver.recv_timeout(Duration::from_secs(30)) {
                Ok(ClipboardEvent::TextCopied(text)) => {
                    if let Err(err) = storage.upsert_text(&text) {
                        eprintln!("failed to store clipboard item: {err:#}");
                    }
                }
                Ok(ClipboardEvent::ImageCopied {
                    content_type,
                    bytes,
                }) => {
                    if let Err(err) = storage.upsert_image(&content_type, &bytes) {
                        eprintln!("failed to store image clipboard item: {err:#}");
                    }
                }
                Ok(ClipboardEvent::Error(message)) => {
                    eprintln!("{message}");
                    std::thread::sleep(WATCHER_RESTART_DELAY);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    eprintln!("clipboard watcher disconnected");
                    std::thread::sleep(WATCHER_RESTART_DELAY);
                    break;
                }
            }
        }
    }
}

pub fn stop_running() -> Result<bool> {
    let Some(pid) = read_lock_pid()? else {
        return Ok(false);
    };

    #[cfg(unix)]
    unsafe {
        if libc::kill(pid as i32, libc::SIGTERM) != 0 {
            return Err(std::io::Error::last_os_error()).context("failed to signal clipway daemon");
        }
    }

    for _ in 0..20 {
        if !is_running()? {
            return Ok(true);
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(true)
}

struct DaemonLock {
    file: File,
}

impl DaemonLock {
    fn acquire() -> Result<Self> {
        let mut file = open_lock_file()?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                write_pid(&mut file)?;
                Ok(Self { file })
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                bail!("Clipway daemon is already running")
            }
            Err(err) => Err(err).context("failed to acquire daemon lock"),
        }
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = self.file.set_len(0);
        let _ = self.file.unlock();
    }
}

fn open_lock_file() -> Result<File> {
    let path = paths::daemon_lock_path()?;

    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("failed to open daemon lock file {}", path.display()))
}

fn write_pid(file: &mut File) -> Result<()> {
    file.set_len(0)
        .context("failed to clear daemon lock file")?;
    file.seek(SeekFrom::Start(0))
        .context("failed to rewind daemon lock file")?;
    file.write_all(std::process::id().to_string().as_bytes())
        .context("failed to write pid into daemon lock file")?;
    file.sync_data()
        .context("failed to flush daemon lock file")?;
    Ok(())
}

fn read_lock_pid() -> Result<Option<u32>> {
    let mut file = open_lock_file()?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)
        .context("failed to read daemon lock file")?;

    Ok(buffer.trim().parse::<u32>().ok())
}
