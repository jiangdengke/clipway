use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum ClipboardEvent {
    TextCopied(String),
    ImageCopied {
        content_type: String,
        bytes: Vec<u8>,
    },
    Error(String),
}

pub fn spawn_text_watcher(sender: Sender<ClipboardEvent>) {
    std::thread::spawn(move || {
        if let Err(err) = run_text_watcher(sender.clone()) {
            let _ = sender.send(ClipboardEvent::Error(format!(
                "text clipboard watcher failed: {err:#}"
            )));
        }
    });
}

pub fn spawn_image_watcher(sender: Sender<ClipboardEvent>) {
    std::thread::spawn(move || {
        if let Err(err) = run_image_watcher(sender.clone()) {
            let _ = sender.send(ClipboardEvent::Error(format!(
                "image clipboard watcher failed: {err:#}"
            )));
        }
    });
}

pub fn copy_text(content: &str) -> anyhow::Result<()> {
    copy_via_wl_copy("text/plain", content.as_bytes())
}

pub fn copy_image(content_type: &str, bytes: &[u8]) -> anyhow::Result<()> {
    copy_via_wl_copy(content_type, bytes)
}

fn run_text_watcher(sender: Sender<ClipboardEvent>) -> anyhow::Result<()> {
    ensure_wayland()?;

    let mut child = Command::new("wl-paste")
        .args([
            "--no-newline",
            "--type",
            "text",
            "--watch",
            "sh",
            "-c",
            "cat; printf '\\0'",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("wl-paste text watcher did not expose stdout"))?;
    let mut reader = BufReader::new(stdout);

    loop {
        let mut frame = Vec::new();

        match reader.read_until(0, &mut frame) {
            Ok(0) => return Err(anyhow::anyhow!("wl-paste text watcher stopped unexpectedly")),
            Ok(_) => {
                if frame.last() == Some(&0) {
                    frame.pop();
                }

                if frame.is_empty() {
                    continue;
                }

                let text = String::from_utf8_lossy(&frame).into_owned();
                let _ = sender.send(ClipboardEvent::TextCopied(text));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn run_image_watcher(sender: Sender<ClipboardEvent>) -> anyhow::Result<()> {
    ensure_wayland()?;

    let mut child = Command::new("wl-paste")
        .args([
            "--type",
            "image/png",
            "--watch",
            "sh",
            "-c",
            "tmp=$(mktemp); cat > \"$tmp\"; size=$(wc -c < \"$tmp\"); printf '%s\\n' \"$size\"; cat \"$tmp\"; rm -f \"$tmp\"",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("wl-paste image watcher did not expose stdout"))?;
    let mut reader = BufReader::new(stdout);

    loop {
        let mut size_line = String::new();

        match reader.read_line(&mut size_line) {
            Ok(0) => return Err(anyhow::anyhow!("wl-paste image watcher stopped unexpectedly")),
            Ok(_) => {
                let size = size_line
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| anyhow::anyhow!("invalid image frame size: {}", size_line.trim()))?;

                if size == 0 {
                    continue;
                }

                let mut bytes = vec![0; size];
                reader.read_exact(&mut bytes)?;

                let _ = sender.send(ClipboardEvent::ImageCopied {
                    content_type: String::from("image/png"),
                    bytes,
                });
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn copy_via_wl_copy(content_type: &str, bytes: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;

    let mut command = Command::new("wl-copy");
    command
        .args(["--type", content_type])
        .stdin(Stdio::piped())
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

    let mut child = command.spawn().context("failed to launch wl-copy")?;
    let mut stdin = child
        .stdin
        .take()
        .context("wl-copy stdin was unavailable")?;
    stdin
        .write_all(bytes)
        .context("failed to write clipboard bytes to wl-copy")?;
    drop(stdin);

    let status = child.wait().context("failed to wait for wl-copy")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("wl-copy exited with status {status}"))
    }
}

fn ensure_wayland() -> anyhow::Result<()> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "WAYLAND_DISPLAY is not set. Clipway currently only supports Wayland."
        ))
    }
}
