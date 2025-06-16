use async_channel::TryRecvError;
use eframe::egui::{self, Context};
use futures::StreamExt;
use ppd::{PpdProxy, PpdProxyBlocking};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

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

    let (ui_sender, ui_receiver) = async_channel::unbounded::<Profile>();
    let (task_sender, task_receiver) = async_channel::unbounded();
    let (repaint_sender, repaint_receiver) = async_channel::unbounded();

    let _task_setup = crate::runtime().spawn(async move {
        let conn = zbus::Connection::system().await.unwrap();
        let setter_proxy = PpdProxy::new(&conn).await.unwrap();
        let signal_proxy = setter_proxy.clone();

        let _setter_task = crate::runtime().spawn(async move {
            while let Ok(v) = ui_receiver.recv().await {
                setter_proxy
                    .set_active_profile(v.to_string())
                    .await
                    .unwrap()
            }
        });

        let _signal_task = crate::runtime().spawn(async move {
            let rp: Context = repaint_receiver.recv().await.unwrap();
            repaint_receiver.close();
            let mut signal = signal_proxy.receive_active_profile_changed().await;
            while let Some(p) = signal.next().await {
                let profile: Profile = p.get().await.unwrap().into();
                task_sender.send(profile).await.unwrap();
                rp.request_repaint();
            }
        });
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 240.0])
            .with_resizable(false),
        ..Default::default()
    };

    // Our application state:
    let mut current = proxy.active_profile().unwrap().into();

    eframe::run_simple_native("ppd-egui", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("System Power");
            ui.label("Power Profiles");

            if !repaint_sender.is_closed() {
                let _ = repaint_sender.try_send(ctx.clone());
            }

            match task_receiver.try_recv() {
                Ok(v) => {
                    current = v;
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Closed) => panic!("Channel should not be closed"),
            }

            for p in &profiles {
                if ui
                    .add(egui::RadioButton::new(current == *p, p.to_string()))
                    .clicked()
                {
                    ui_sender.try_send(*p).expect("Channel should work");
                }
            }
        });
    })
}
