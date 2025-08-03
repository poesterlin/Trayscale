use std::thread;
use tray_icon::{
    TrayIconBuilder,
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
};

use crate::docker::Docker;

mod docker;

const TOGGLE_ID: &str = "toggle";
const QUIT_ID: &str = "quit";
const REFRESH_ID: &str = "refresh";

fn main() {
    let handle = thread::spawn(run_tray_app);
    handle.join().unwrap();
}

enum AppMessage {
    Toggle,
    Refresh,
    Quit,
}

/// Runs the entire tray application logic within the GTK event loop.
fn run_tray_app() {
    gtk::init().unwrap();

    const ICON_BYTES: &[u8] = include_bytes!("../imgs/docker.png");
    let icon = load_icon_from_bytes(ICON_BYTES);

    let (tx, rx) = std::sync::mpsc::channel::<AppMessage>();

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(rebuild_menu()))
        .with_tooltip("Docker Control")
        .with_icon(icon)
        .build()
        .unwrap();

    let tx_clone = tx.clone();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let (toggle_id, refresh_id, quit_id) = get_menu_item_ids();
        let event_id = event.id();

        // The handler's only job is to send a message.
        let msg = if event_id == &toggle_id {
            AppMessage::Toggle
        } else if event_id == &refresh_id {
            AppMessage::Refresh
        } else {
            AppMessage::Quit
        };

        tx.send(msg).unwrap();
    }));

    glib::source::timeout_add_local(std::time::Duration::from_secs(45), move || {
        tx_clone.send(AppMessage::Refresh).unwrap();
        glib::ControlFlow::Continue
    });

    glib::source::timeout_add_local(std::time::Duration::from_millis(200), move || {
        if let Ok(message) = rx.try_recv() {
            match message {
                AppMessage::Refresh => {
                    // We are on the main thread, so we can safely call `set_menu`.
                    tray_icon.set_menu(Some(Box::new(rebuild_menu())));
                }
                AppMessage::Toggle => {
                    let _ = Docker::toggle();
                    // We are on the main thread, so we can safely call `set_menu`.
                    tray_icon.set_menu(Some(Box::new(rebuild_menu())));
                }
                AppMessage::Quit => {
                    println!("Quitting...");
                    gtk::main_quit();
                    return glib::ControlFlow::Break;
                }
            }
        }

        glib::ControlFlow::Continue
    });

    gtk::main();
}

/// Rebuilds the menu based on current state.
fn rebuild_menu() -> Menu {
    let menu = Menu::new();

    // First, determine the current state of the Docker daemon.
    let is_active = Docker::is_active().unwrap_or(false);

    // Create the primary control items (Start/Stop, Refresh, Quit).
    let (toggle_item, refresh_item, quit_item) = build_control_items(is_active);

    if is_active {
        // --- Docker is ACTIVE ---
        // Add the main controls first.
        menu.append_items(&[
            &toggle_item,
            &refresh_item,
            &PredefinedMenuItem::separator(),
        ])
        .unwrap();

        // Try to get the detailed status. If it fails, we'll just show a generic message.
        match Docker::status() {
            Ok(status) => {
                // Display the detailed status info as un-clickable menu items.
                let status_header = MenuItem::new(
                    format!("Status: {}", status.active_state),
                    false, // Disabled
                    None,
                );
                menu.append(&status_header).unwrap();

                if let Some(pid) = status.main_pid {
                    let pid_item = MenuItem::new(
                        format!("PID: {}", pid),
                        false, // Disabled
                        None,
                    );
                    menu.append(&pid_item).unwrap();
                }
                if let Some(mem) = status.memory_peak {
                    let mem_item = MenuItem::new(
                        format!("Memory Peak: {}", mem),
                        false, // Disabled
                        None,
                    );
                    menu.append(&mem_item).unwrap();
                }
            }
            Err(_) => {
                // Fallback if status parsing fails for some reason
                let error_item = MenuItem::new("Could not retrieve status", false, None);
                menu.append(&error_item).unwrap();
            }
        }
    } else {
        // --- Docker is INACTIVE ---
        // The menu is much simpler. Just show a status and the button to start it.
        let status_header = MenuItem::new("Status: Inactive", false, None);
        menu.append_items(&[
            &toggle_item,
            &PredefinedMenuItem::separator(),
            &status_header,
            &refresh_item,
        ])
        .unwrap();
    }

    // Finally, add the separator and the quit button to all menu variants.
    menu.append_items(&[&PredefinedMenuItem::separator(), &quit_item])
        .unwrap();

    menu
}

/// Helper to create the main control menu items (Start/Stop, Refresh, Quit).
fn build_control_items(is_active: bool) -> (MenuItem, MenuItem, MenuItem) {
    let toggle_text = if is_active {
        "Stop Docker"
    } else {
        "Start Docker"
    };

    let toggle_item = MenuItem::with_id(
        MenuId::new(TOGGLE_ID),
        toggle_text,
        true, // Enabled
        None,
    );

    let refresh_item = MenuItem::with_id(
        MenuId::new(REFRESH_ID),
        "Refresh",
        true, // Enabled
        None,
    );

    let quit_item = MenuItem::with_id(
        MenuId::new(QUIT_ID),
        "Quit Tray",
        true, // Enabled
        None,
    );

    (toggle_item, refresh_item, quit_item)
}

/// Helper to get the IDs of control items without modifying state.
fn get_menu_item_ids() -> (MenuId, MenuId, MenuId) {
    let (toggle, refresh, quit) = build_control_items(true);
    (toggle.id().clone(), refresh.id().clone(), quit.id().clone())
}

/// Helper function to load a PNG icon for the tray.
fn load_icon_from_bytes(bytes: &[u8]) -> tray_icon::Icon {
    let image = image::load_from_memory(bytes)
        .expect("Failed to load icon from memory")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon")
}
