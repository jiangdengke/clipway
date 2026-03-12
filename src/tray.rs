use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use ksni::blocking::TrayMethods;

use crate::daemon;
use crate::storage::Storage;

pub fn run() -> Result<()> {
    let _ = daemon::ensure_running();

    let (sender, receiver) = mpsc::channel();
    let tray = ClipwayTray {
        sender,
        daemon_running: daemon::is_running().unwrap_or(false),
        item_count: Storage::open()
            .and_then(|storage| storage.history_signature())
            .map(|signature| signature.count)
            .unwrap_or(0),
    };

    let handle = tray.assume_sni_available(true).spawn()?;

    loop {
        match receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(TrayCommand::OpenWindow) => {
                let _ = daemon::spawn_detached_subcommand("gui");
            }
            Ok(TrayCommand::StartCapture) => {
                let _ = daemon::ensure_running();
            }
            Ok(TrayCommand::StopCapture) => {
                let _ = daemon::stop_running();
            }
            Ok(TrayCommand::ClearHistory) => {
                if let Ok(storage) = Storage::open() {
                    let _ = storage.clear();
                }
            }
            Ok(TrayCommand::QuitTray) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        handle.update(|tray: &mut ClipwayTray| {
            tray.daemon_running = daemon::is_running().unwrap_or(false);
            tray.item_count = Storage::open()
                .and_then(|storage| storage.history_signature())
                .map(|signature| signature.count)
                .unwrap_or(0);
        });
    }

    handle.shutdown().wait();

    Ok(())
}

#[derive(Debug)]
struct ClipwayTray {
    sender: mpsc::Sender<TrayCommand>,
    daemon_running: bool,
    item_count: i64,
}

impl ksni::Tray for ClipwayTray {
    fn id(&self) -> String {
        String::from("clipway")
    }

    fn title(&self) -> String {
        format!("Clipway ({})", self.item_count)
    }

    fn icon_name(&self) -> String {
        if self.daemon_running {
            String::from("edit-paste")
        } else {
            String::from("process-stop")
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        use ksni::MenuItem;

        let capture_label = if self.daemon_running {
            "Stop background capture"
        } else {
            "Start background capture"
        };
        let capture_command = if self.daemon_running {
            TrayCommand::StopCapture
        } else {
            TrayCommand::StartCapture
        };

        vec![
            StandardItem {
                label: String::from("Open Clipway"),
                icon_name: String::from("edit-paste"),
                activate: {
                    let sender = self.sender.clone();
                    Box::new(move |_| {
                        let _ = sender.send(TrayCommand::OpenWindow);
                    })
                },
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: capture_label.into(),
                activate: {
                    let sender = self.sender.clone();
                    Box::new(move |_| {
                        let _ = sender.send(capture_command);
                    })
                },
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: String::from("Clear history"),
                icon_name: String::from("user-trash"),
                activate: {
                    let sender = self.sender.clone();
                    Box::new(move |_| {
                        let _ = sender.send(TrayCommand::ClearHistory);
                    })
                },
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: String::from("Quit tray"),
                icon_name: String::from("application-exit"),
                activate: {
                    let sender = self.sender.clone();
                    Box::new(move |_| {
                        let _ = sender.send(TrayCommand::QuitTray);
                    })
                },
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
enum TrayCommand {
    OpenWindow,
    StartCapture,
    StopCapture,
    ClearHistory,
    QuitTray,
}
