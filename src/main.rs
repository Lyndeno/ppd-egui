use async_channel::{Receiver, Sender};
use eframe::egui::{self, Context};
use futures::StreamExt;
use ppd::{PpdProxy, PpdProxyBlocking};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

use crate::toggle_switch::ToggleSwitch;

mod toggle_switch;

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

fn task_setup(sender: Sender<PpdValue>, receiver: Receiver<PpdValue>) {
    let _task_setup = crate::runtime().spawn(async move {
        let rp = if let Ok(PpdValue::Context(c)) = receiver.recv().await {
            Some(c)
        } else {
            None
        };
        let rp_ba = rp.clone();
        let conn = zbus::Connection::system().await.unwrap();
        let setter_proxy = PpdProxy::new(&conn).await.unwrap();
        let profile_signal_proxy = setter_proxy.clone();
        let ba_signal_proxy = profile_signal_proxy.clone();

        let _setter_task = crate::runtime().spawn(async move {
            while let Ok(v) = receiver.recv().await {
                match v {
                    PpdValue::Profile(p) => setter_proxy
                        .set_active_profile(p.to_string())
                        .await
                        .unwrap(),
                    PpdValue::Context(_) => todo!(),
                    PpdValue::BatteryAware(ba) => setter_proxy.set_battery_aware(ba).await.unwrap(),
                }
            }
        });

        let signal_sender = sender.clone();
        let _signal_task = crate::runtime().spawn(async move {
            let mut signal = profile_signal_proxy.receive_active_profile_changed().await;
            while let Some(p) = signal.next().await {
                let profile: Profile = p.get().await.unwrap().into();
                signal_sender
                    .send(PpdValue::Profile(profile))
                    .await
                    .unwrap();
                if let Some(c) = &rp {
                    c.request_repaint();
                };
            }
        });

        let battery_aware_sender = sender.clone();
        let _battery_aware_task = crate::runtime().spawn(async move {
            let mut signal = ba_signal_proxy.receive_battery_aware_changed().await;
            while let Some(p) = signal.next().await {
                let ba = p.get().await.unwrap();
                battery_aware_sender
                    .send(PpdValue::BatteryAware(ba))
                    .await
                    .unwrap();
                if let Some(c) = &rp_ba {
                    c.request_repaint();
                };
            }
        });
    });
}

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let conn = zbus::blocking::Connection::system().unwrap();
    let proxy = PpdProxyBlocking::new(&conn).unwrap();

    let profiles: Vec<Profile> = proxy
        .profiles()
        .unwrap()
        .into_iter()
        .map(|p| p.profile.into())
        .collect();

    let _ = conn.close();

    let (ui_sender, ui_receiver) = async_channel::unbounded();
    let (task_sender, task_receiver) = async_channel::unbounded();
    task_setup(task_sender, ui_receiver);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 240.0])
            .with_resizable(false),
        ..Default::default()
    };

    // Our application state:
    let mut current = proxy.active_profile().unwrap().into();
    let mut mybool = proxy.battery_aware().unwrap();
    let mut context_sent = false;

    eframe::run_simple_native("ppd-egui", options, move |ctx, _frame| {
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
    })
}
