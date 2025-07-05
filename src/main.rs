use std::sync::OnceLock;
use thiserror::Error as ThisError;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

mod dbus;
mod state;
mod toggle_switch;
mod ui;

use crate::state::Profile;

#[derive(ThisError, Debug)]
enum Error {
    #[error("Error communicating with dbus")]
    ZBusError(#[from] zbus::Error),
    #[error("Error producing user interface")]
    EFrameError(#[from] eframe::Error),
}

/// Create or return existing tokio runtime
///
/// # Panics
/// Panics when a runtime is not able to be created. Failure to create a runtime will result in the
/// program not working.
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Tokio runtime needs to work"))
}

fn main() -> Result<(), Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // Initialize state
    let conn = zbus::blocking::Connection::system()?;
    let proxy = ppd::PpdProxyBlocking::new(&conn)?;

    let profiles: Vec<Profile> = proxy
        .profiles()?
        .into_iter()
        .map(|p| p.profile.into())
        .collect();

    let current = proxy.active_profile()?.into();
    let battery_aware = proxy.battery_aware()?;

    let _ = conn.close();

    // Set up channels
    let (ui_sender, ui_receiver) = tokio::sync::mpsc::channel(10);
    let (task_sender, task_receiver) = tokio::sync::mpsc::channel(10);

    let token = CancellationToken::new();

    // Spawn DBus setup as a background task
    let _dbus_task = runtime().spawn(dbus::setup_dbus(task_sender, ui_receiver, token.clone()));

    let app = ui::PpdApp::new(profiles, current, battery_aware, ui_sender, task_receiver);

    // Run UI (this blocks until the UI is closed)
    let result = app.run_ui();

    // Clean up
    token.cancel();

    result.map_err(Into::into)
}
