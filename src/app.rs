use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use adw::prelude::*;
use anyhow::{Context, Result};
use gtk::Align;

use crate::clipboard;
use crate::daemon;
use crate::paths;
use crate::storage::{ClipboardEntry, ClipboardEntryKind, HistorySignature, Storage, human_size};

const HISTORY_PAGE_SIZE: usize = 500;
const GUI_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Clone)]
struct AppWidgets {
    list_box: gtk::ListBox,
    search_entry: gtk::SearchEntry,
    status_label: gtk::Label,
    toast_overlay: adw::ToastOverlay,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ViewSnapshot {
    signature: HistorySignature,
    query: String,
    daemon_running: bool,
}

enum GuiCommand {
    Toggle(Option<String>),
}

pub fn send_toggle_request(activation_token: Option<String>) -> Result<bool> {
    let path = paths::gui_socket_path()?;

    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            if let Some(token) = activation_token {
                stream
                    .write_all(token.as_bytes())
                    .context("failed to send activation token to the running GUI")?;
            }

            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            let _ = std::fs::remove_file(&path);
            Ok(false)
        }
        Err(err) => Err(err).with_context(|| format!("failed to connect to {}", path.display())),
    }
}

pub fn run(startup_notice: Option<String>, activation_token: Option<String>) {
    let app = adw::Application::builder()
        .application_id("io.github.jdk.clipway")
        .build();
    let initial_activation_token = Rc::new(RefCell::new(activation_token));

    app.connect_activate(move |app| {
        build_ui(
            app,
            startup_notice.clone(),
            initial_activation_token.borrow_mut().take(),
        )
    });
    app.run();
}

fn build_ui(
    app: &adw::Application,
    startup_notice: Option<String>,
    initial_activation_token: Option<String>,
) {
    let storage = match Storage::open() {
        Ok(storage) => Rc::new(RefCell::new(storage)),
        Err(err) => {
            present_fatal_window(app, &format!("Failed to open Clipway storage:\n\n{err:#}"));
            return;
        }
    };

    let toast_overlay = adw::ToastOverlay::new();
    let list_box = build_history_list();
    let search_entry = build_search_entry();
    let status_label = build_status_label();
    let widgets = AppWidgets {
        list_box,
        search_entry,
        status_label,
        toast_overlay,
    };
    let snapshot = Rc::new(RefCell::new(ViewSnapshot::default()));
    let (gui_command_sender, gui_command_receiver) = mpsc::channel();
    let socket_listener = match start_gui_socket_listener(gui_command_sender) {
        Ok(listener) => Rc::new(listener),
        Err(err) => {
            present_fatal_window(
                app,
                &format!("Failed to create Clipway GUI control socket:\n\n{err:#}"),
            );
            return;
        }
    };
    let content = build_content(&widgets);
    let header_bar = adw::HeaderBar::new();
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh history")
        .build();
    let clear_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Clear clipboard history")
        .build();

    refresh_button.add_css_class("flat");
    clear_button.add_css_class("flat");

    header_bar.set_title_widget(Some(&gtk::Label::new(Some("Clipway"))));
    header_bar.pack_end(&clear_button);
    header_bar.pack_end(&refresh_button);

    widgets.toast_overlay.set_child(Some(&content));

    let window_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window_content.append(&header_bar);
    window_content.append(&widgets.toast_overlay);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Clipway")
        .default_width(560)
        .default_height(760)
        .build();

    window.set_content(Some(&window_content));

    refresh_view(&widgets, &storage, &snapshot, true);

    if let Some(message) = startup_notice {
        widgets.toast_overlay.add_toast(adw::Toast::new(&message));
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();

        refresh_button.connect_clicked(move |_| {
            refresh_view(&widgets, &storage, &snapshot, true);
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();

        clear_button.connect_clicked(move |_| match storage.borrow().clear() {
            Ok(()) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new("Clipboard history cleared"));
                refresh_view(&widgets, &storage, &snapshot, true);
            }
            Err(err) => {
                widgets.toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Failed to clear history: {err:#}"
                )));
            }
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();

        widgets
            .search_entry
            .clone()
            .connect_search_changed(move |_| {
                refresh_view(&widgets, &storage, &snapshot, true);
            });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();

        gtk::glib::timeout_add_local(Duration::from_millis(800), move || {
            refresh_view(&widgets, &storage, &snapshot, false);
            gtk::glib::ControlFlow::Continue
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let window = window.clone();
        let socket_listener = socket_listener.clone();

        gtk::glib::timeout_add_local(GUI_COMMAND_POLL_INTERVAL, move || {
            let _keep_listener_alive = &socket_listener;

            loop {
                match gui_command_receiver.try_recv() {
                    Ok(GuiCommand::Toggle(activation_token)) => {
                        toggle_window(&window, &widgets, &storage, &snapshot, activation_token);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }

            gtk::glib::ControlFlow::Continue
        });
    }

    present_window(
        &window,
        &widgets,
        &storage,
        &snapshot,
        initial_activation_token,
    );
}

fn build_history_list() -> gtk::ListBox {
    let list_box = gtk::ListBox::new();
    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::new(Some("Clipboard history will appear here"));
    let subtitle = gtk::Label::new(Some(
        "Copy some text in a Wayland application and it will be captured by the background daemon.",
    ));

    title.add_css_class("title-4");
    subtitle.add_css_class("dim-label");
    title.set_halign(Align::Center);
    subtitle.set_halign(Align::Center);
    subtitle.set_wrap(true);

    placeholder.set_valign(Align::Center);
    placeholder.set_vexpand(true);
    placeholder.append(&title);
    placeholder.append(&subtitle);

    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("boxed-list");
    list_box.set_placeholder(Some(&placeholder));

    list_box
}

fn build_search_entry() -> gtk::SearchEntry {
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search clipboard history"));
    search_entry.set_hexpand(true);
    search_entry
}

fn build_status_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn build_content(widgets: &AppWidgets) -> gtk::Box {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&widgets.list_box)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&widgets.status_label);
    content.append(&widgets.search_entry);
    content.append(&scroller);

    content
}

fn refresh_view(
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    force: bool,
) {
    let signature = match storage.borrow().history_signature() {
        Ok(signature) => signature,
        Err(err) => {
            widgets.toast_overlay.add_toast(adw::Toast::new(&format!(
                "Failed to read clipboard history: {err:#}"
            )));
            return;
        }
    };

    let query = widgets.search_entry.text().to_string();
    let daemon_running = daemon::is_running().unwrap_or(false);
    let current = ViewSnapshot {
        signature,
        query: query.clone(),
        daemon_running,
    };

    if force || *snapshot.borrow() != current {
        match storage.borrow().recent_entries(HISTORY_PAGE_SIZE) {
            Ok(entries) => {
                populate_entries(widgets, storage, snapshot, &query, entries);
                *snapshot.borrow_mut() = current.clone();
            }
            Err(err) => {
                widgets.toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Failed to load clipboard history: {err:#}"
                )));
            }
        }
    }

    set_status_label(widgets, storage.borrow().database_path(), current);
}

fn populate_entries(
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    query: &str,
    entries: Vec<ClipboardEntry>,
) {
    clear_list_box(&widgets.list_box);

    let normalized_query = query.trim().to_lowercase();

    for entry in entries
        .into_iter()
        .filter(|entry| matches_query(entry, &normalized_query))
    {
        let row = build_entry_row(entry, widgets.clone(), storage.clone(), snapshot.clone());
        widgets.list_box.append(&row);
    }
}

fn matches_query(entry: &ClipboardEntry, query: &str) -> bool {
    query.is_empty() || entry.content.to_lowercase().contains(query)
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
}

fn build_entry_row(
    entry: ClipboardEntry,
    widgets: AppWidgets,
    storage: Rc<RefCell<Storage>>,
    snapshot: Rc<RefCell<ViewSnapshot>>,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Delete this item")
        .valign(Align::Center)
        .build();

    delete_button.add_css_class("flat");

    let subtitle = build_row_subtitle(&entry);
    let title = match entry.kind {
        ClipboardEntryKind::Text => preview_text(&entry.content),
        ClipboardEntryKind::Image => entry.content.clone(),
    };

    row.set_title(&title);
    row.set_subtitle(&subtitle);
    row.set_activatable(true);
    row.add_suffix(&delete_button);
    row.set_tooltip_text(Some(&entry.content));

    if let Some(prefix) = build_image_prefix(&entry) {
        row.add_prefix(&prefix);
    }

    {
        let content = entry.content.clone();
        let content_type = entry.content_type.clone();
        let binary_content = entry.binary_content.clone();
        let kind = entry.kind.clone();
        let widgets = widgets.clone();

        row.connect_activated(move |_| {
            let copy_result = match kind {
                ClipboardEntryKind::Text => clipboard::copy_text(&content),
                ClipboardEntryKind::Image => clipboard::copy_image(
                    &content_type,
                    binary_content.as_deref().unwrap_or_default(),
                ),
            };

            match copy_result {
                Ok(()) => {
                    widgets
                        .toast_overlay
                        .add_toast(adw::Toast::new("Copied back to clipboard"));
                }
                Err(err) => {
                    widgets.toast_overlay.add_toast(adw::Toast::new(&format!(
                        "Failed to write clipboard: {err:#}"
                    )));
                }
            }
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let entry_id = entry.id;

        delete_button.connect_clicked(move |_| match storage.borrow().delete_entry(entry_id) {
            Ok(()) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new("Clipboard item deleted"));
                refresh_view(&widgets, &storage, &snapshot, true);
            }
            Err(err) => {
                widgets.toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Failed to delete clipboard item: {err:#}"
                )));
            }
        });
    }

    row
}

fn build_row_subtitle(entry: &ClipboardEntry) -> String {
    match entry.kind {
        ClipboardEntryKind::Text => format!(
            "#{}  {} | text | activate to copy",
            entry.id, entry.created_at
        ),
        ClipboardEntryKind::Image => {
            let size = entry
                .binary_content
                .as_ref()
                .map(|bytes| human_size(bytes.len() as u64))
                .unwrap_or_else(|| String::from("unknown size"));

            if let Some((width, height)) = image_dimensions(entry) {
                format!(
                    "#{}  {} | {}x{} | {} | activate to copy",
                    entry.id, entry.created_at, width, height, size
                )
            } else {
                format!(
                    "#{}  {} | {} | activate to copy",
                    entry.id, entry.created_at, size
                )
            }
        }
    }
}

fn build_image_prefix(entry: &ClipboardEntry) -> Option<gtk::Picture> {
    let bytes = entry.binary_content.as_ref()?;
    let texture =
        gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes.clone())).ok()?;

    let picture = gtk::Picture::new();
    picture.set_paintable(Some(&texture));
    picture.set_size_request(72, 72);

    Some(picture)
}

fn image_dimensions(entry: &ClipboardEntry) -> Option<(i32, i32)> {
    let bytes = entry.binary_content.as_ref()?;
    let texture =
        gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes.clone())).ok()?;
    Some((texture.width(), texture.height()))
}

fn preview_text(content: &str) -> String {
    const MAX_CHARS: usize = 100;

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

fn toggle_window(
    window: &adw::ApplicationWindow,
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    activation_token: Option<String>,
) {
    if window.is_visible() && window.is_active() {
        window.hide();
        return;
    }

    present_window(window, widgets, storage, snapshot, activation_token);
}

fn present_window(
    window: &adw::ApplicationWindow,
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    activation_token: Option<String>,
) {
    if let Some(token) = activation_token.as_deref() {
        window.set_startup_id(token);
    }

    refresh_view(widgets, storage, snapshot, true);
    window.present();
    let _ = widgets.search_entry.grab_focus();
}

fn set_status_label(widgets: &AppWidgets, database_path: &std::path::Path, snapshot: ViewSnapshot) {
    let daemon_state = if snapshot.daemon_running {
        "Background capture is running"
    } else {
        "Background capture is not running"
    };

    let search_state = if snapshot.query.trim().is_empty() {
        String::from("showing all items")
    } else {
        format!("search: \"{}\"", snapshot.query.trim())
    };

    widgets.status_label.set_text(&format!(
        "{} | {} items stored | {} | {}",
        daemon_state,
        snapshot.signature.count,
        search_state,
        database_path.display()
    ));
}

fn present_fatal_window(app: &adw::Application, message: &str) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Clipway")
        .default_width(520)
        .default_height(220)
        .build();

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title = gtk::Label::new(Some("Clipway could not start"));
    let detail = gtk::Label::new(Some(message));

    title.add_css_class("title-3");
    detail.set_wrap(true);
    detail.set_selectable(true);
    title.set_xalign(0.0);
    detail.set_xalign(0.0);

    container.set_margin_top(24);
    container.set_margin_bottom(24);
    container.set_margin_start(24);
    container.set_margin_end(24);
    container.append(&title);
    container.append(&detail);

    window.set_content(Some(&container));
    window.present();
}

fn start_gui_socket_listener(sender: mpsc::Sender<GuiCommand>) -> Result<GuiSocketListener> {
    let path = paths::gui_socket_path()?;

    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("failed to bind GUI control socket {}", path.display()))?;
    listener
        .set_nonblocking(true)
        .context("failed to configure GUI control socket as nonblocking")?;

    let running = Arc::new(AtomicBool::new(true));
    let join_handle = spawn_gui_socket_thread(listener, running.clone(), sender);

    Ok(GuiSocketListener {
        path,
        running,
        join_handle: Some(join_handle),
    })
}

fn spawn_gui_socket_thread(
    listener: UnixListener,
    running: Arc<AtomicBool>,
    sender: mpsc::Sender<GuiCommand>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut token = String::new();

                    if stream.read_to_string(&mut token).is_ok() {
                        let token = token.trim().to_string();
                        let activation_token = if token.is_empty() { None } else { Some(token) };
                        let _ = sender.send(GuiCommand::Toggle(activation_token));
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(GUI_COMMAND_POLL_INTERVAL);
                }
                Err(_) => break,
            }
        }
    })
}

struct GuiSocketListener {
    path: PathBuf,
    running: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl Drop for GuiSocketListener {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.path);

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }

        let _ = std::fs::remove_file(&self.path);
    }
}
