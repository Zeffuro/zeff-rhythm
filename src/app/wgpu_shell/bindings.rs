use super::*;
use crate::app::settings::{InputSettings, LaneBinding};

pub(super) fn key_label(code: KeyCode) -> Option<String> {
    let name = format!("{code:?}");
    let label = name
        .strip_prefix("Key")
        .or_else(|| name.strip_prefix("Digit"))?;
    (label.len() == 1).then(|| label.to_owned())
}

pub(super) fn normalize_bindings(input: &mut InputSettings) {
    let valid = input
        .lane_bindings
        .iter()
        .enumerate()
        .all(|(index, binding)| {
            binding.lane as usize == index
                && binding.code.len() == 1
                && binding.code.chars().all(|c| c.is_ascii_alphanumeric())
                && !input.lane_bindings[..index]
                    .iter()
                    .any(|other| other.code.eq_ignore_ascii_case(&binding.code))
        });
    if !valid {
        input.lane_bindings = InputSettings::default().lane_bindings;
    }
}

impl WgpuAppShell {
    pub(super) fn bound_lane(&self, code: KeyCode) -> Option<usize> {
        let label = key_label(code)?;
        self.state
            .settings
            .input
            .lane_bindings
            .iter()
            .position(|binding| binding.code.eq_ignore_ascii_case(&label))
    }

    pub(super) fn bindings_text(&self) -> String {
        self.state
            .settings
            .input
            .lane_bindings
            .iter()
            .map(|binding| binding.code.to_uppercase())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub(super) fn capture_binding(&mut self, code: KeyCode) {
        let Some(lane) = self.binding_capture else {
            return;
        };
        if code == KeyCode::Escape {
            self.binding_capture = None;
            return;
        }
        let Some(label) = key_label(code) else {
            return;
        };
        let bindings = &mut self.state.settings.input.lane_bindings;
        if let Some(other) = bindings
            .iter()
            .position(|binding| binding.code.eq_ignore_ascii_case(&label))
        {
            bindings[other].code = bindings[lane].code.clone();
        }
        bindings[lane] = LaneBinding::keyboard_scancode(lane as u8, &label);
        self.binding_capture = None;
        self.persist_current_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebinding_swaps_duplicates_and_drives_actual_lane_input() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.binding_capture = Some(0);
        shell.capture_binding(KeyCode::KeyJ);
        assert_eq!(shell.bound_lane(KeyCode::KeyJ), Some(0));
        assert_eq!(shell.bound_lane(KeyCode::KeyD), Some(2));
        assert_eq!(shell.bindings_text(), "J / F / D / K");
        shell.binding_capture = Some(1);
        shell.capture_binding(KeyCode::Escape);
        assert_eq!(shell.bound_lane(KeyCode::KeyF), Some(1));
    }
    #[test]
    fn invalid_persisted_bindings_restore_defaults_without_changing_offset() {
        let mut input = InputSettings::default();
        input.input_offset_ms = 24.0;
        input.lane_bindings[0].code = "unsupported".into();
        normalize_bindings(&mut input);
        assert_eq!(input.lane_bindings, InputSettings::default().lane_bindings);
        assert_eq!(input.input_offset_ms, 24.0);
    }
}
