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
use gtk4_layer_shell::{
    Edge as LayerShellEdge, KeyboardMode as LayerShellKeyboardMode, Layer as LayerShellLayer,
    LayerShell,
};

use crate::clipboard;
use crate::daemon;
use crate::paths;
use crate::storage::{ClipboardEntry, ClipboardEntryKind, HistorySignature, Storage, human_size};

const HISTORY_PAGE_SIZE: usize = 500;
const GUI_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(120);
const APP_CSS: &str = r#"
window.clipway-window {
    background: transparent;
}

.clipway-shell {
    padding: 28px 0 0;
    background: transparent;
}

.clipway-panel {
    background-color: rgba(30, 30, 46, 0.87);
    border: 1px solid rgba(156, 207, 216, 0.16);
    border-radius: 16px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
    font-family: "MesloLGL Nerd Font";
    font-size: 11pt;
}

.clipway-status {
    margin: 0 2px 8px;
    color: #c4c3ce;
    font-size: 10pt;
}

.clipway-inputbar {
    padding: 2px;
    margin: 0 0 8px;
    border-radius: 10px;
    background-color: #2a2a37;
}

.clipway-prompt {
    padding: 8px 12px;
    margin: 0;
    border-radius: 8px;
    background-color: #9ccfd8;
    color: #1e1e2e;
    font-weight: 700;
}

searchentry.clipway-search {
    background-color: transparent;
    color: #e0def4;
    border: none;
    outline: none;
    box-shadow: none;
    padding: 4px 8px;
}

searchentry.clipway-search image {
    color: #c4c3ce;
}

.clipway-list {
    background: transparent;
}

.clipway-row {
    margin: 3px 0;
    border-radius: 12px;
    background: #2a2a37;
    border: 1px solid rgba(224, 222, 244, 0.06);
}

.clipway-row:hover,
.clipway-row:focus-within {
    background: #363646;
    border-color: rgba(156, 207, 216, 0.34);
}

.clipway-thumb {
    border-radius: 10px;
    background: #2a2a37;
    border: 1px solid rgba(224, 222, 244, 0.10);
}

.clipway-kind-badge {
    padding: 5px 9px;
    border-radius: 999px;
    font-size: 9pt;
    font-weight: 700;
}

.clipway-kind-badge-text {
    background: rgba(156, 207, 216, 0.18);
    color: #9ccfd8;
}

.clipway-kind-badge-image {
    background: rgba(246, 193, 119, 0.18);
    color: #f6c177;
}

.clipway-action-button,
.clipway-danger-button {
    min-width: 32px;
    min-height: 32px;
    border-radius: 8px;
    background: #363646;
    color: #e0def4;
}

.clipway-action-button:hover,
.clipway-danger-button:hover {
    background: #404053;
}

.clipway-danger-button {
    color: #eb6f92;
}

.clipway-empty-title {
    color: #e0def4;
}

.clipway-empty-subtitle {
    color: #c4c3ce;
}
"#;

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

    app.connect_startup(|_| install_app_css());
    app.connect_activate(move |app| {
        build_ui(
            app,
            startup_notice.clone(),
            initial_activation_token.borrow_mut().take(),
        )
    });
    app.run_with_args(&["clipway"]);
}

fn build_ui(
    app: &adw::Application,
    startup_notice: Option<String>,
    initial_activation_token: Option<String>,
) {
    let storage = match Storage::open() {
        Ok(storage) => Rc::new(RefCell::new(storage)),
        Err(err) => {
            present_fatal_window(app, &format!("无法打开 Clipway 数据库：\n\n{err:#}"));
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
                &format!("无法创建 Clipway GUI 控制 socket：\n\n{err:#}"),
            );
            return;
        }
    };
    let clear_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("清空历史")
        .build();
    let hide_button = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("收起面板")
        .build();
    let prompt_label = gtk::Label::new(Some("剪切板"));

    clear_button.add_css_class("flat");
    clear_button.add_css_class("clipway-danger-button");
    hide_button.add_css_class("flat");
    hide_button.add_css_class("clipway-action-button");
    prompt_label.add_css_class("clipway-prompt");

    let content = build_content(&widgets, &prompt_label, &clear_button, &hide_button);

    widgets.toast_overlay.set_child(Some(&content));

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("clipway-panel");
    panel.set_halign(Align::Center);
    panel.append(&widgets.toast_overlay);

    let window_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window_content.add_css_class("clipway-shell");
    window_content.append(&panel);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("剪切板历史")
        .default_width(560)
        .default_height(460)
        .build();

    configure_popup_window(&window);
    window.add_css_class("clipway-window");
    window.set_decorated(false);
    window.set_hide_on_close(true);
    window.set_content(Some(&window_content));
    add_window_shortcuts(&window);
    add_focus_behavior(&window);

    refresh_view(&widgets, &storage, &snapshot, &window, true);

    if let Some(message) = startup_notice {
        widgets.toast_overlay.add_toast(adw::Toast::new(&message));
    }

    {
        let window = window.clone();

        hide_button.connect_clicked(move |_| {
            window.hide();
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let window = window.clone();

        clear_button.connect_clicked(move |_| match storage.borrow().clear() {
            Ok(()) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new("已清空剪切板历史"));
                refresh_view(&widgets, &storage, &snapshot, &window, true);
            }
            Err(err) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new(&format!("清空历史失败：{err:#}")));
            }
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let window = window.clone();

        widgets
            .search_entry
            .clone()
            .connect_search_changed(move |_| {
                refresh_view(&widgets, &storage, &snapshot, &window, true);
            });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let window = window.clone();

        gtk::glib::timeout_add_local(Duration::from_millis(800), move || {
            refresh_view(&widgets, &storage, &snapshot, &window, false);
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
    let title = gtk::Label::new(Some("剪切板历史会显示在这里"));
    let subtitle = gtk::Label::new(Some(
        "在 Wayland 应用里复制一些内容后，这里会自动显示最近的剪切板历史。",
    ));

    title.add_css_class("clipway-empty-title");
    subtitle.add_css_class("clipway-empty-subtitle");
    title.set_halign(Align::Center);
    subtitle.set_halign(Align::Center);
    subtitle.set_wrap(true);

    placeholder.set_valign(Align::Center);
    placeholder.set_vexpand(true);
    placeholder.append(&title);
    placeholder.append(&subtitle);

    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("clipway-list");
    list_box.set_placeholder(Some(&placeholder));

    list_box
}

fn build_search_entry() -> gtk::SearchEntry {
    let search_entry = gtk::SearchEntry::new();
    search_entry.add_css_class("clipway-search");
    search_entry.set_placeholder_text(Some("搜索..."));
    search_entry.set_hexpand(true);
    search_entry
}

fn build_status_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("clipway-status");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn build_content(
    widgets: &AppWidgets,
    prompt_label: &gtk::Label,
    clear_button: &gtk::Button,
    hide_button: &gtk::Button,
) -> gtk::Box {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&widgets.list_box)
        .build();
    let inputbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    scroller.set_min_content_height(300);
    inputbar.add_css_class("clipway-inputbar");
    actions_box.append(clear_button);
    actions_box.append(hide_button);
    inputbar.append(prompt_label);
    inputbar.append(&widgets.search_entry);
    inputbar.append(&actions_box);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.append(&inputbar);
    content.append(&widgets.status_label);
    content.append(&scroller);

    content
}

fn refresh_view(
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    window: &adw::ApplicationWindow,
    force: bool,
) {
    let signature = match storage.borrow().history_signature() {
        Ok(signature) => signature,
        Err(err) => {
            widgets
                .toast_overlay
                .add_toast(adw::Toast::new(&format!("读取剪切板历史失败：{err:#}")));
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
                populate_entries(widgets, storage, snapshot, window, &query, entries);
                *snapshot.borrow_mut() = current.clone();
            }
            Err(err) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new(&format!("加载剪切板历史失败：{err:#}")));
            }
        }
    }

    set_status_label(widgets, storage.borrow().database_path(), current);
}

fn populate_entries(
    widgets: &AppWidgets,
    storage: &Rc<RefCell<Storage>>,
    snapshot: &Rc<RefCell<ViewSnapshot>>,
    window: &adw::ApplicationWindow,
    query: &str,
    entries: Vec<ClipboardEntry>,
) {
    clear_list_box(&widgets.list_box);

    let normalized_query = query.trim().to_lowercase();

    for entry in entries
        .into_iter()
        .filter(|entry| matches_query(entry, &normalized_query))
    {
        let row = build_entry_row(
            entry,
            widgets.clone(),
            storage.clone(),
            snapshot.clone(),
            window.clone(),
        );
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
    window: adw::ApplicationWindow,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("删除这条记录")
        .valign(Align::Center)
        .build();
    let kind_badge = build_kind_badge(&entry);

    delete_button.add_css_class("flat");
    delete_button.add_css_class("clipway-danger-button");
    row.add_css_class("clipway-row");

    let subtitle = build_row_subtitle(&entry);
    let title = match entry.kind {
        ClipboardEntryKind::Text => preview_text(&entry.content),
        ClipboardEntryKind::Image => entry.content.clone(),
    };

    row.set_title(&title);
    row.set_subtitle(&subtitle);
    row.set_title_lines(2);
    row.set_subtitle_lines(1);
    row.set_activatable(true);
    row.add_prefix(&kind_badge);
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
        let window = window.clone();

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
                    window.hide();
                }
                Err(err) => {
                    widgets
                        .toast_overlay
                        .add_toast(adw::Toast::new(&format!("写入剪切板失败：{err:#}")));
                }
            }
        });
    }

    {
        let widgets = widgets.clone();
        let storage = storage.clone();
        let snapshot = snapshot.clone();
        let entry_id = entry.id;
        let window = window.clone();

        delete_button.connect_clicked(move |_| match storage.borrow().delete_entry(entry_id) {
            Ok(()) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new("已删除这条记录"));
                refresh_view(&widgets, &storage, &snapshot, &window, true);
            }
            Err(err) => {
                widgets
                    .toast_overlay
                    .add_toast(adw::Toast::new(&format!("删除记录失败：{err:#}")));
            }
        });
    }

    row
}

fn build_row_subtitle(entry: &ClipboardEntry) -> String {
    match entry.kind {
        ClipboardEntryKind::Text => format!("#{} · {} · 文本", entry.id, entry.created_at),
        ClipboardEntryKind::Image => {
            let size = entry
                .binary_content
                .as_ref()
                .map(|bytes| human_size(bytes.len() as u64))
                .unwrap_or_else(|| String::from("大小未知"));

            if let Some((width, height)) = image_dimensions(entry) {
                format!(
                    "#{} · {} · {}x{} · {}",
                    entry.id, entry.created_at, width, height, size
                )
            } else {
                format!("#{} · {} · {}", entry.id, entry.created_at, size)
            }
        }
    }
}

fn build_image_prefix(entry: &ClipboardEntry) -> Option<gtk::Picture> {
    let bytes = entry.binary_content.as_ref()?;
    let texture =
        gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes.clone())).ok()?;

    let picture = gtk::Picture::new();
    picture.add_css_class("clipway-thumb");
    picture.set_paintable(Some(&texture));
    picture.set_size_request(72, 72);

    Some(picture)
}

fn build_kind_badge(entry: &ClipboardEntry) -> gtk::Label {
    let label = gtk::Label::new(Some(match entry.kind {
        ClipboardEntryKind::Text => "文本",
        ClipboardEntryKind::Image => "图片",
    }));

    label.add_css_class("clipway-kind-badge");
    match entry.kind {
        ClipboardEntryKind::Text => label.add_css_class("clipway-kind-badge-text"),
        ClipboardEntryKind::Image => label.add_css_class("clipway-kind-badge-image"),
    }

    label
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
        String::from("[仅空白字符]")
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

    refresh_view(widgets, storage, snapshot, window, true);
    window.present();
    let _ = widgets.search_entry.grab_focus();
}

fn set_status_label(widgets: &AppWidgets, database_path: &std::path::Path, snapshot: ViewSnapshot) {
    let daemon_state = if snapshot.daemon_running {
        "正在监听剪切板"
    } else {
        "后台监听未运行"
    };

    let search_state = if snapshot.query.trim().is_empty() {
        String::from("全部记录")
    } else {
        format!("筛选：{}", snapshot.query.trim())
    };

    widgets.status_label.set_text(&format!(
        "{} · 共 {} 条 · {} · Esc 收起",
        daemon_state, snapshot.signature.count, search_state
    ));
    widgets
        .status_label
        .set_tooltip_text(Some(&format!("数据库位置：{}", database_path.display())));
}

fn present_fatal_window(app: &adw::Application, message: &str) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("剪切板历史")
        .default_width(520)
        .default_height(220)
        .build();

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title = gtk::Label::new(Some("Clipway 启动失败"));
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

fn install_app_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_data(APP_CSS);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn configure_popup_window(window: &adw::ApplicationWindow) {
    if !gtk4_layer_shell::is_supported() {
        return;
    }

    window.init_layer_shell();
    window.set_namespace(Some("clipway"));
    window.set_layer(LayerShellLayer::Overlay);
    window.set_anchor(LayerShellEdge::Top, true);
    window.set_margin(LayerShellEdge::Top, 12);
    window.set_keyboard_mode(LayerShellKeyboardMode::Exclusive);
}

fn add_window_shortcuts(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let window = window.clone();
    let window_for_escape = window.clone();

    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            window_for_escape.hide();
            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });

    window.add_controller(controller);
}

fn add_focus_behavior(window: &adw::ApplicationWindow) {
    window.connect_is_active_notify(|window| {
        if window.is_visible() && !window.is_active() {
            window.hide();
        }
    });
}
