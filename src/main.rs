use async_channel::{Receiver, Sender};
use eframe::egui::{self, Context};
use futures::StreamExt;
use ppd::{PpdProxy, PpdProxyBlocking};
use std::sync::OnceLock;
use thiserror::Error as ThisError;
use tokio::runtime::Runtime;
use tokio::select;
use tokio_util::sync::CancellationToken;

use crate::toggle_switch::ToggleSwitch;

mod toggle_switch;

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

#[derive(Debug, PartialEq, Clone, Copy)]
enum Profile {
    PowerSaver,
    Balanced,
    Performance,
    Other,
}

#[derive(Debug, PartialEq, Clone)]
enum PpdValue {
    Profile(Profile),
    Context(Context),
    BatteryAware(bool),
}

impl From<String> for Profile {
    fn from(value: String) -> Self {
        match value.as_str() {
            "power-saver" => Self::PowerSaver,
            "balanced" => Self::Balanced,
            "performance" => Self::Performance,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Other => "other",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::PowerSaver => "power-saver",
        };
        write!(f, "{s}")
    }
}

async fn task_setup(
    sender: Sender<PpdValue>,
    receiver: Receiver<PpdValue>,
    cancellation_token: CancellationToken,
) {
    let rp = if let Ok(PpdValue::Context(c)) = receiver.recv().await {
        Some(c)
    } else {
        None
    };
    let rp_ba = rp.clone();

    if let Ok(conn) = zbus::Connection::system().await {
        if let Ok(setter_proxy) = PpdProxy::new(&conn).await {
            let profile_signal_proxy = setter_proxy.clone();
            let ba_signal_proxy = profile_signal_proxy.clone();

            let setter_task = crate::runtime().spawn(async move {
                while let Ok(v) = receiver.recv().await {
                    let result = match v {
                        PpdValue::Profile(p) => {
                            setter_proxy.set_active_profile(p.to_string()).await
                        }
                        PpdValue::BatteryAware(ba) => setter_proxy.set_battery_aware(ba).await,
                        PpdValue::Context(_) => Ok(()),
                    };

                    if result.is_err() {
                        break;
                    }
                }
            });

            let signal_sender = sender.clone();
            let signal_task = crate::runtime().spawn(async move {
                let mut signal = profile_signal_proxy.receive_active_profile_changed().await;
                while let Some(p) = signal.next().await {
                    if let Ok(profile) = p.get().await {
                        if let Ok(()) = signal_sender.send(PpdValue::Profile(profile.into())).await
                        {
                            if let Some(c) = &rp {
                                c.request_repaint();
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            });

            let battery_aware_sender = sender.clone();
            let battery_aware_task = crate::runtime().spawn(async move {
                let mut signal = ba_signal_proxy.receive_battery_aware_changed().await;
                while let Some(p) = signal.next().await {
                    if let Ok(ba) = p.get().await {
                        if let Ok(()) = battery_aware_sender.send(PpdValue::BatteryAware(ba)).await
                        {
                            if let Some(c) = &rp_ba {
                                c.request_repaint();
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            });

            select! {
                () = cancellation_token.cancelled() => {},
                _ = setter_task => {},
                _ = signal_task => {},
                _ = battery_aware_task => {},
            };
        }
    }
}

fn main() -> Result<(), Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let conn = zbus::blocking::Connection::system()?;
    let proxy = PpdProxyBlocking::new(&conn)?;

    let profiles: Vec<Profile> = proxy
        .profiles()?
        .into_iter()
        .map(|p| p.profile.into())
        .collect();

    // Our application state:
    let mut current = proxy.active_profile()?.into();
    let mut mybool = proxy.battery_aware()?;
    let mut context_sent = false;

    let _ = conn.close();

    let (ui_sender, ui_receiver) = async_channel::unbounded();
    let (task_sender, task_receiver) = async_channel::unbounded();

    let token = CancellationToken::new();
    let tasks = crate::runtime().spawn(task_setup(task_sender, ui_receiver, token.clone()));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 240.0])
            .with_resizable(false),
        ..Default::default()
    };

    let result = eframe::run_simple_native("ppd-egui", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("System Power");
            ui.label("Power Profiles");

            if !context_sent {
                context_sent = ui_sender.try_send(PpdValue::Context(ctx.clone())).is_ok();
            }

            for p in &profiles {
                if ui
                    .add(egui::RadioButton::new(current == *p, p.to_string()))
                    .clicked()
                {
                    ui_sender
                        .try_send(PpdValue::Profile(*p))
                        .expect("Channel should work");
                }
            }

            ui.label("Battery Aware");
            let mut rp = ui.add(ToggleSwitch::new(mybool));
            if rp.clicked() {
                ui_sender
                    .try_send(PpdValue::BatteryAware(!mybool))
                    .expect("Channel should work");
            }

            while let Ok(v) = task_receiver.try_recv() {
                match v {
                    PpdValue::Profile(p) => {
                        current = p;
                    }
                    PpdValue::BatteryAware(ba) => {
                        mybool = ba;
                        rp.mark_changed();
                    }
                    PpdValue::Context(_) => {}
                }
            }
        });
    });

    // The other tasks will trigger cancellation when the channels drop.
    // But we manually cancel in case in the future that is not the case.
    token.cancel();
    // Wait for tasks to end
    let _ = runtime().block_on(tasks);

    if let Err(e) = result {
        Err(e.into())
    } else {
        Ok(())
    }
}
