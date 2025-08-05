use cli_clipboard;
use muda::Submenu;
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
const DESELECT_EXIT_NODE_ID: &str = "deselect_exit_node";

fn main() {
    let handle = thread::spawn(run_tray_app);
    handle.join().unwrap();
}

enum AppMessage {
    Toggle,
    Refresh,
    Quit,
    DeselectExitNode,
    SetExitNode(String),
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

    let tx_clone = tx.clone();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let (toggle_id, refresh_id, quit_id) = get_menu_item_ids();
        let event_id = event.id();

        // The handler's only job is to send a message.
        let msg = if event_id == &toggle_id {
            AppMessage::Toggle
        } else if event_id == &refresh_id {
            AppMessage::Refresh
        } else if event_id.0 == DESELECT_EXIT_NODE_ID {
            AppMessage::DeselectExitNode
        } else if event_id == &quit_id {
            AppMessage::Quit
        } else if event_id.0.starts_with("copy-") {
            let ip = event_id.0.replace("copy-", "");
            AppMessage::CopyIp(ip)
        } else if event_id.0.starts_with("set-exit-node-") {
            let hostname = event_id.0.replace("set-exit-node-", "");
            AppMessage::SetExitNode(hostname)
        } else {
            return;
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
                AppMessage::DeselectExitNode => {
                    let _ = Tailscale::deselect_exit_node();
                    tray_icon.set_menu(Some(Box::new(rebuild_menu())));
                }
                AppMessage::SetExitNode(hostname) => {
                    let _ = Tailscale::set_exit_node(&hostname);
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
        if machines.iter().any(|m| m.is_exit_node) {
            let deselect_item = MenuItem::with_id(
                MenuId::new(DESELECT_EXIT_NODE_ID),
                "Deselect Exit Node",
                true,
                None,
            );
            menu.append(&deselect_item).unwrap();
            menu.append(&PredefinedMenuItem::separator()).unwrap();
        }

        let submenu = Submenu::new("Set Exit Node", true);

        for machine in machines {
            let icon = if machine.is_exit_node {
                "🔽"
            // } else if machine.online && machine.advertises_exit_node {
            //     "🚀"
            } else if machine.online {
                "🟢"
            } else {
                "⚫"
            };
            let text = format!("{} {} ({})", icon, machine.hostname, machine.ip);
            let id = MenuId::new(format!("copy-{}", machine.ip.clone()));
            let item = MenuItem::with_id(id, text, true, None);
            menu.append(&item).unwrap();

            if machine.advertises_exit_node {
                let text = format!("{}", machine.hostname);
                let id = MenuId::new(format!("set-exit-node-{}", machine.hostname));
                let exit_node_item = MenuItem::with_id(id, text, true, None);
                submenu.append(&exit_node_item).unwrap();
            }
        }

        menu.append(&submenu).unwrap();
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
