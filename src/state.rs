use eframe::egui::Context;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Profile {
    PowerSaver,
    Balanced,
    Performance,
    Other,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PpdValue {
    Profile(Profile),
    BatteryAware(bool),
    Context(Context),
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
