use async_channel::TryRecvError;
use eframe::egui;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Tokio runtime needs to work"))
}

#[derive(Debug, PartialEq)]
enum Profile {
    PowerSaver,
    Balanced,
    Performance,
}

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let (ui_sender, ui_receiver) = async_channel::unbounded();
    let (task_sender, task_receiver) = async_channel::unbounded();

    let background = crate::runtime().spawn(async move {
        while let Ok(v) = ui_receiver.recv().await {
            task_sender.send(v).await.unwrap();
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 240.0])
            .with_resizable(false),
        ..Default::default()
    };

    // Our application state:
    let mut current = Profile::Balanced;

    eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("My egui Application");
            ui.label("Power Profiles");

            match task_receiver.try_recv() {
                Ok(v) => {
                    current = v;
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Closed) => panic!("Channel should not be closed"),
            }

            if ui
                .add(egui::RadioButton::new(
                    current == Profile::Balanced,
                    "Balanced",
                ))
                .clicked()
            {
                ui_sender
                    .try_send(Profile::Balanced)
                    .expect("Channel should work");
            }
            if ui
                .add(egui::RadioButton::new(
                    current == Profile::PowerSaver,
                    "Power Saver",
                ))
                .clicked()
            {
                ui_sender
                    .try_send(Profile::PowerSaver)
                    .expect("Channel should work");
            }
            if ui
                .add(egui::RadioButton::new(
                    current == Profile::Performance,
                    "Performance",
                ))
                .clicked()
            {
                ui_sender
                    .try_send(Profile::Performance)
                    .expect("Channel should work");
            }
        });
    })
}
