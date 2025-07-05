use crate::state::PpdValue;
use crate::state::Profile;
use crate::toggle_switch::ToggleSwitch;
use eframe::egui;
use tokio::sync::mpsc::{Receiver, Sender};

const APP_NAME: &str = "ppd-egui";
const WINDOW_WIDTH: f32 = 320.0;
const WINDOW_HEIGHT: f32 = 240.0;

pub struct PpdApp {
    pub profiles: Vec<Profile>,
    pub current_profile: Profile,
    pub battery_aware: bool,
    pub sender: Sender<PpdValue>,
    pub receiver: Receiver<PpdValue>,
    context_sent: bool,
}

impl PpdApp {
    pub fn new(
        profiles: Vec<Profile>,
        current_profile: Profile,
        battery_aware: bool,
        sender: Sender<PpdValue>,
        receiver: Receiver<PpdValue>,
    ) -> Self {
        Self {
            profiles,
            current_profile,
            battery_aware,
            sender,
            receiver,
            context_sent: false,
        }
    }

    pub fn update_profile(&mut self, profile: Profile) {
        self.current_profile = profile;
    }

    pub fn update_battery_aware(&mut self, value: bool) {
        self.battery_aware = value;
    }

    pub fn run_ui(mut self) -> Result<(), eframe::Error> {
        self.context_sent = false;
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
                .with_resizable(false),
            ..Default::default()
        };

        eframe::run_native(APP_NAME, options, Box::new(|_| Ok(Box::new(self))))
    }
}

impl eframe::App for PpdApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("System Power");
            ui.label("Power Profiles");

            if !self.context_sent {
                self.sender
                    .try_send(PpdValue::Context(ctx.clone()))
                    .expect("Channel must work");
            }

            // Render profiles
            let profiles = self.profiles.clone();
            for p in &profiles {
                if ui
                    .add(egui::RadioButton::new(
                        self.current_profile == *p,
                        p.to_string(),
                    ))
                    .clicked()
                {
                    self.sender
                        .try_send(PpdValue::Profile(*p))
                        .expect("Channel should work");
                }
            }

            ui.label("Battery Aware");
            let mut rp = ui.add(ToggleSwitch::new(self.battery_aware));
            if rp.clicked() {
                self.sender
                    .try_send(PpdValue::BatteryAware(!self.battery_aware))
                    .expect("Channel should work");
            }

            while let Ok(v) = self.receiver.try_recv() {
                match v {
                    PpdValue::Profile(p) => {
                        self.update_profile(p);
                    }
                    PpdValue::BatteryAware(ba) => {
                        self.update_battery_aware(ba);
                        rp.mark_changed();
                    }
                    PpdValue::Context(_) => {}
                }
            }
        });
    }
}
