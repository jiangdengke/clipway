mod app;
mod clipboard;
mod daemon;
mod paths;
mod storage;
mod tray;

use anyhow::{Context, Result, bail};
use storage::{ClipboardEntryKind, Storage};

const DEFAULT_LIST_LIMIT: usize = 20;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match parse_command()? {
        Command::Gui { activation_token } => {
            let startup_notice = match daemon::ensure_running() {
                Ok(true) => Some(String::from("Clipway background capture started")),
                Ok(false) => None,
                Err(err) => Some(format!("Background capture could not start: {err:#}")),
            };

            if app::send_toggle_request(activation_token.clone())? {
                return Ok(());
            }

            app::run(startup_notice, activation_token);
            Ok(())
        }
        Command::Daemon => daemon::run_foreground(),
        Command::Tray => tray::run(),
        Command::List { limit } => {
            let storage = Storage::open()?;
            for entry in storage.recent_entries(limit)? {
                let preview = match entry.kind {
                    ClipboardEntryKind::Text => preview_text(&entry.content),
                    ClipboardEntryKind::Image => entry.content,
                };
                println!("{}\t{}\t{}", entry.id, entry.created_at, preview);
            }
            Ok(())
        }
        Command::Copy { id } => {
            let storage = Storage::open()?;
            let entry = storage
                .entry_by_id(id)?
                .with_context(|| format!("clipboard item {id} was not found"))?;

            match entry.kind {
                ClipboardEntryKind::Text => clipboard::copy_text(&entry.content)?,
                ClipboardEntryKind::Image => clipboard::copy_image(
                    &entry.content_type,
                    entry
                        .binary_content
                        .as_deref()
                        .context("image payload was missing")?,
                )?,
            }
            println!("copied item {id} back to the clipboard");
            Ok(())
        }
        Command::Clear => {
            let storage = Storage::open()?;
            storage.clear()?;
            println!("clipboard history cleared");
            Ok(())
        }
        Command::Help => {
            print_help();
            Ok(())
        }
    }
}

fn preview_text(content: &str) -> String {
    const MAX_CHARS: usize = 80;

    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = if normalized.is_empty() {
        String::from("[whitespace only]")
    } else {
        normalized
    };

    let mut chars = preview.chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn parse_command() -> Result<Command> {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        None => Ok(Command::Gui {
            activation_token: take_activation_token_from_env(),
        }),
        Some("gui") => {
            let mut activation_token = take_activation_token_from_env();

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--activation-token" => {
                        activation_token =
                            Some(args.next().context("--activation-token requires a value")?);
                    }
                    other => bail!("unknown gui option: {other}"),
                }
            }

            Ok(Command::Gui { activation_token })
        }
        Some("daemon") => Ok(Command::Daemon),
        Some("tray") => Ok(Command::Tray),
        Some("list") => {
            let limit = match args.next() {
                Some(value) => value
                    .parse::<usize>()
                    .with_context(|| format!("invalid list limit: {value}"))?,
                None => DEFAULT_LIST_LIMIT,
            };

            Ok(Command::List { limit })
        }
        Some("copy") => {
            let id = args
                .next()
                .context("copy requires an item id, for example: clipway copy 42")?
                .parse::<i64>()
                .context("copy id must be an integer")?;

            Ok(Command::Copy { id })
        }
        Some("clear") => Ok(Command::Clear),
        Some("help" | "--help" | "-h") => Ok(Command::Help),
        Some("--version" | "-V") => {
            println!("clipway {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        Some(other) => {
            bail!("unknown command: {other}\n\nRun `clipway help` to see supported commands.")
        }
    }
}

fn print_help() {
    println!(
        "\
Clipway {}

Usage:
  clipway                Launch the GUI and ensure the background daemon is running
  clipway gui            Launch the GUI
  clipway gui --activation-token TOKEN
                         Launch or activate the GUI with a Wayland activation token
  clipway daemon         Run the clipboard watcher in the foreground
  clipway tray           Start the tray resident entry
  clipway list [limit]   Print the most recent clipboard items
  clipway copy <id>      Copy a saved item back to the clipboard
  clipway clear          Delete all saved clipboard history
  clipway help           Show this help
",
        env!("CARGO_PKG_VERSION")
    );
}

fn take_activation_token_from_env() -> Option<String> {
    std::env::var("CLIPWAY_ACTIVATION_TOKEN")
        .ok()
        .or_else(|| std::env::var("XDG_ACTIVATION_TOKEN").ok())
}

enum Command {
    Gui { activation_token: Option<String> },
    Daemon,
    Tray,
    List { limit: usize },
    Copy { id: i64 },
    Clear,
    Help,
}
