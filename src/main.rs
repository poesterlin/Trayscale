use cli_clipboard;
use std::thread;
use tray_icon::{
    TrayIconBuilder,
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
};

// Assuming your tailscale module is still present
use crate::tailscale::Tailscale;
mod tailscale;

#[derive(Clone, Debug)]
pub struct MachineData {
    pub ip: String,
    pub hostname: String,
    pub online: bool,
}

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
    CopyIp(String),
}

/// Runs the entire tray application logic within the GTK event loop.
fn run_tray_app() {
    gtk::init().unwrap();

    const ICON_BYTES: &[u8] = include_bytes!("../imgs/tailscale-32x32.png");
    let icon = load_icon_from_bytes(ICON_BYTES);

    let (tx, rx) = std::sync::mpsc::channel::<AppMessage>();

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(rebuild_menu()))
        .with_tooltip("Tailscale Control")
        .with_icon(icon)
        .build()
        .unwrap();

    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let (toggle_id, refresh_id, quit_id) = get_menu_item_ids();
        let event_id = event.id();

        // The handler's only job is to send a message.
        let msg = if event_id == &toggle_id {
            AppMessage::Toggle
        } else if event_id == &refresh_id {
            AppMessage::Refresh
        } else if event_id == &quit_id {
            AppMessage::Quit
        } else {
            AppMessage::CopyIp(event_id.0.clone())
        };

        tx.send(msg).unwrap();
    }));

    glib::source::idle_add_local(move || {
        if let Ok(message) = rx.try_recv() {
            match message {
                AppMessage::Refresh => {
                    // We are on the main thread, so we can safely call `set_menu`.
                    tray_icon.set_menu(Some(Box::new(rebuild_menu())));
                }
                AppMessage::Toggle => {
                    let _ = Tailscale::toggle();
                    // We are on the main thread, so we can safely call `set_menu`.
                    tray_icon.set_menu(Some(Box::new(rebuild_menu())));
                }
                AppMessage::CopyIp(ip) => {
                    if cli_clipboard::set_contents(ip.clone()).is_ok() {
                        println!("Copied IP {} to clipboard!", ip);
                    } else {
                        eprintln!("Failed to copy IP {} to clipboard.", ip);
                    }
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
    let is_enabled = Tailscale::is_enabled().unwrap_or(false);
    let (toggle_item, refresh_item, quit_item) = build_control_items(is_enabled);

    if is_enabled {
        menu.append_items(&[
            &toggle_item,
            &refresh_item,
            &PredefinedMenuItem::separator(),
        ])
        .unwrap();

        let machines = Tailscale::status().unwrap_or_else(|_| vec![]);
        for machine in machines {
            let icon = if machine.online { "🟢" } else { "⚫" };
            let text = format!("{} {} ({})", icon, machine.hostname, machine.ip);
            let id = MenuId::new(machine.ip.clone());
            let machine_item = MenuItem::with_id(id, text, true, None);

            menu.append(&machine_item).unwrap();
        }
    } else {
        menu.append_items(&[&toggle_item, &refresh_item]).unwrap();
    }

    menu.append_items(&[&PredefinedMenuItem::separator(), &quit_item])
        .unwrap();

    menu
}

/// Helper to create control menu items.
fn build_control_items(is_enabled: bool) -> (MenuItem, MenuItem, MenuItem) {
    let toggle_text = if is_enabled {
        "Turn Tailscale Off"
    } else {
        "Turn Tailscale On"
    };
    let toggle_item = MenuItem::with_id(MenuId::new(TOGGLE_ID), toggle_text, true, None);
    let refresh_item = MenuItem::with_id(MenuId::new(REFRESH_ID), "Refresh", true, None);
    let quit_item = MenuItem::with_id(MenuId::new(QUIT_ID), "Quit", true, None);
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
