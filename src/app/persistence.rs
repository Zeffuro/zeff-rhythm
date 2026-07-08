use super::settings::AppSettings;
use crate::play::LiveAudioSummary;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct AppPersistence {
    path: Option<PathBuf>,
    config: AppConfigFile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfigFile {
    pub version: u32,
    pub settings: AppSettings,
    pub calibration_offsets: Vec<SavedCalibrationOffset>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCalibrationOffset {
    pub key: CalibrationDeviceKey,
    pub input_offset_ms: f64,
    pub hit_count: usize,
    pub trial_count: usize,
    pub confidence: String,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationDeviceKey {
    pub host_name: String,
    pub device_key: String,
    pub sample_rate: u32,
    pub buffer_frames: Option<u32>,
}

impl AppPersistence {
    pub fn disabled() -> Self {
        Self {
            path: None,
            config: AppConfigFile::default(),
        }
    }

    pub fn load_default() -> Result<Self, Box<dyn Error>> {
        Self::load_from_path(default_config_path())
    }

    pub fn load_from_path(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let config = if path.exists() {
            let source = fs::read_to_string(&path)?;
            toml::from_str(&source)?
        } else {
            AppConfigFile::default()
        };

        Ok(Self {
            path: Some(path),
            config,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn settings(&self) -> &AppSettings {
        &self.config.settings
    }

    pub fn set_settings(&mut self, settings: &AppSettings) {
        self.config.settings = settings.clone();
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let source = toml::to_string_pretty(&self.config)?;
        fs::write(path, source)?;
        Ok(())
    }

    pub fn save_settings(&mut self, settings: &AppSettings) -> Result<(), Box<dyn Error>> {
        self.set_settings(settings);
        self.save()
    }

    pub fn calibration_for_key(
        &self,
        key: &CalibrationDeviceKey,
    ) -> Option<&SavedCalibrationOffset> {
        self.config
            .calibration_offsets
            .iter()
            .find(|saved| &saved.key == key)
    }

    pub fn upsert_calibration_offset(
        &mut self,
        saved_offset: SavedCalibrationOffset,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(existing) = self
            .config
            .calibration_offsets
            .iter_mut()
            .find(|saved| saved.key == saved_offset.key)
        {
            *existing = saved_offset;
        } else {
            self.config.calibration_offsets.push(saved_offset);
        }

        self.save()
    }
}

impl Default for AppConfigFile {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            settings: AppSettings::default(),
            calibration_offsets: Vec::new(),
        }
    }
}

impl SavedCalibrationOffset {
    pub fn new(
        key: CalibrationDeviceKey,
        input_offset_ms: f64,
        hit_count: usize,
        trial_count: usize,
        confidence: impl Into<String>,
    ) -> Self {
        Self {
            key,
            input_offset_ms,
            hit_count,
            trial_count,
            confidence: confidence.into(),
            updated_at_ms: now_ms(),
        }
    }
}

impl CalibrationDeviceKey {
    pub fn from_audio(audio: &LiveAudioSummary) -> Self {
        Self {
            host_name: non_empty_or_default(&audio.host_name, "default"),
            device_key: audio
                .device_id
                .clone()
                .unwrap_or_else(|| non_empty_or_default(&audio.device_label, "default")),
            sample_rate: audio.sample_rate,
            buffer_frames: audio.buffer_frames,
        }
    }

    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            host_name: settings
                .audio
                .host
                .clone()
                .unwrap_or_else(|| "default".to_owned()),
            device_key: settings
                .audio
                .device_id
                .clone()
                .or_else(|| settings.audio.device_label.clone())
                .unwrap_or_else(|| "default".to_owned()),
            sample_rate: settings.audio.sample_rate.unwrap_or_default(),
            buffer_frames: settings.audio.buffer_frames,
        }
    }
}

fn default_config_path() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("zeff-rhythm")
            .join("app-state.toml");
    }

    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home)
            .join("zeff-rhythm")
            .join("app-state.toml");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("zeff-rhythm")
            .join("app-state.toml");
    }

    PathBuf::from(".local_config").join("app-state.toml")
}

fn non_empty_or_default(value: &str, default: &str) -> String {
    if value.trim().is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{AppPersistence, CalibrationDeviceKey, SavedCalibrationOffset};
    use crate::play::LiveAudioSummary;
    use std::fs;

    #[test]
    fn round_trips_settings_and_calibration_offsets() {
        let path = temp_config_path_for_test("round-trip");
        let mut persistence = AppPersistence::load_from_path(path.clone()).unwrap();
        let mut settings = persistence.settings().clone();
        settings.input.input_offset_ms = -20.5;

        persistence.save_settings(&settings).unwrap();
        persistence
            .upsert_calibration_offset(SavedCalibrationOffset::new(
                CalibrationDeviceKey {
                    host_name: "WASAPI".to_owned(),
                    device_key: "device-id".to_owned(),
                    sample_rate: 96_000,
                    buffer_frames: Some(960),
                },
                -20.5,
                72,
                3,
                "STABLE",
            ))
            .unwrap();

        let loaded = AppPersistence::load_from_path(path.clone()).unwrap();

        assert_eq!(loaded.settings().input.input_offset_ms, -20.5);
        assert_eq!(loaded.config.calibration_offsets.len(), 1);
        assert_eq!(loaded.config.calibration_offsets[0].hit_count, 72);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn device_key_prefers_resolved_audio_device_id() {
        let key = CalibrationDeviceKey::from_audio(&LiveAudioSummary {
            host_name: "WASAPI".to_owned(),
            device_label: "Speakers".to_owned(),
            device_id: Some("wasapi:{device}".to_owned()),
            sample_rate: 96_000,
            buffer_frames: Some(960),
        });

        assert_eq!(key.host_name, "WASAPI");
        assert_eq!(key.device_key, "wasapi:{device}");
        assert_eq!(key.sample_rate, 96_000);
        assert_eq!(key.buffer_frames, Some(960));
    }

    fn temp_config_path_for_test(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zeff-rhythm-{label}-{unique}.toml"))
    }
}
