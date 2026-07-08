use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderPresentModePreference {
    Fifo,
    Mailbox,
    Immediate,
}

impl RenderPresentModePreference {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "fifo" => Ok(Self::Fifo),
            "mailbox" => Ok(Self::Mailbox),
            "immediate" => Ok(Self::Immediate),
            _ => Err(format!(
                "invalid present mode `{value}`; expected fifo, mailbox, or immediate"
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fifo => "fifo",
            Self::Mailbox => "mailbox",
            Self::Immediate => "immediate",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Fifo => "FIFO",
            Self::Mailbox => "MAILBOX",
            Self::Immediate => "IMMEDIATE",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Fifo => Self::Mailbox,
            Self::Mailbox => Self::Immediate,
            Self::Immediate => Self::Fifo,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Fifo => Self::Immediate,
            Self::Mailbox => Self::Fifo,
            Self::Immediate => Self::Mailbox,
        }
    }

    pub const fn to_wgpu_present_mode(self) -> wgpu::PresentMode {
        match self {
            Self::Fifo => wgpu::PresentMode::Fifo,
            Self::Mailbox => wgpu::PresentMode::Mailbox,
            Self::Immediate => wgpu::PresentMode::Immediate,
        }
    }

    pub fn select_supported_wgpu_present_mode(
        self,
        supported_modes: &[wgpu::PresentMode],
    ) -> wgpu::PresentMode {
        let requested = self.to_wgpu_present_mode();

        if supported_modes.contains(&requested) {
            requested
        } else {
            wgpu::PresentMode::Fifo
        }
    }
}

impl Default for RenderPresentModePreference {
    fn default() -> Self {
        Self::Fifo
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderLatencySettings {
    pub present_mode: RenderPresentModePreference,
    pub desired_maximum_frame_latency: u32,
}

impl Default for RenderLatencySettings {
    fn default() -> Self {
        Self {
            present_mode: RenderPresentModePreference::Fifo,
            desired_maximum_frame_latency: 1,
        }
    }
}

impl RenderLatencySettings {
    pub fn select_supported_wgpu_present_mode(
        self,
        supported_modes: &[wgpu::PresentMode],
    ) -> wgpu::PresentMode {
        self.present_mode
            .select_supported_wgpu_present_mode(supported_modes)
    }
}

pub fn clamp_desired_frame_latency(value: u32) -> u32 {
    value.clamp(1, 3)
}

#[cfg(test)]
mod tests {
    use super::{RenderLatencySettings, RenderPresentModePreference, clamp_desired_frame_latency};

    #[test]
    fn defaults_to_competitive_fifo_low_latency() {
        let settings = RenderLatencySettings::default();

        assert_eq!(settings.present_mode, RenderPresentModePreference::Fifo);
        assert_eq!(settings.desired_maximum_frame_latency, 1);
    }

    #[test]
    fn clamps_frame_latency_to_supported_app_range() {
        assert_eq!(clamp_desired_frame_latency(0), 1);
        assert_eq!(clamp_desired_frame_latency(2), 2);
        assert_eq!(clamp_desired_frame_latency(99), 3);
    }

    #[test]
    fn unsupported_low_latency_present_modes_fall_back_to_fifo() {
        let supported = [wgpu::PresentMode::Fifo];

        assert_eq!(
            RenderPresentModePreference::Mailbox.select_supported_wgpu_present_mode(&supported),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            RenderPresentModePreference::Immediate.select_supported_wgpu_present_mode(&supported),
            wgpu::PresentMode::Fifo
        );
    }

    #[test]
    fn supported_low_latency_present_mode_is_selected() {
        let supported = [wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox];

        assert_eq!(
            RenderPresentModePreference::Mailbox.select_supported_wgpu_present_mode(&supported),
            wgpu::PresentMode::Mailbox
        );
    }
}
