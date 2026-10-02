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
    pub manual_input_offset_ms: Option<f64>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationDeviceKey {
    pub version: u32,
    pub audio_host: String,
    pub cpal_device_id: Option<String>,
    pub native_device_id: Option<String>,
    pub stream_direction: String,
    pub sample_rate: u32,
    pub requested_buffer_frames: Option<u32>,
    pub first_callback_frames: Option<u32>,
    pub channel_count: u16,
    pub sample_format: String,
    pub shared_mode: bool,
    pub exclusive_mode: bool,
    pub raw_processing: bool,
    pub match_format: bool,
    pub device_label: String,
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
        self.capture_manual_offset();
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

    pub fn manual_input_offset_ms(&self) -> f64 {
        self.config.manual_input_offset_ms.unwrap_or_else(|| {
            if self.config.calibration_offsets.is_empty() {
                self.config.settings.input.input_offset_ms
            } else {
                0.0
            }
        })
    }

    pub fn set_manual_input_offset_ms(&mut self, offset_ms: f64) {
        self.config.manual_input_offset_ms = Some(offset_ms);
    }

    fn capture_manual_offset(&mut self) {
        if self.config.manual_input_offset_ms.is_none() {
            self.config.manual_input_offset_ms = Some(self.manual_input_offset_ms());
        }
    }

    pub fn calibration_for_audio(
        &self,
        audio: &LiveAudioSummary,
    ) -> Option<&SavedCalibrationOffset> {
        let resolved = CalibrationDeviceKey::from_audio(audio);
        self.config
            .calibration_offsets
            .iter()
            .filter(|saved| saved.key.same_stream_as(&resolved))
            .max_by_key(|saved| saved.updated_at_ms)
    }

    pub fn upsert_calibration_offset(
        &mut self,
        saved_offset: SavedCalibrationOffset,
    ) -> Result<(), Box<dyn Error>> {
        self.capture_manual_offset();
        self.config
            .calibration_offsets
            .retain(|saved| !saved.key.same_stream_as(&saved_offset.key));
        self.config.calibration_offsets.push(saved_offset);

        self.save()
    }
}

impl Default for AppConfigFile {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            settings: AppSettings::default(),
            calibration_offsets: Vec::new(),
            manual_input_offset_ms: None,
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
            version: 2,
            audio_host: non_empty_or_default(&audio.host_name, "default"),
            cpal_device_id: audio.device_id.clone(),
            native_device_id: None,
            stream_direction: "output".to_owned(),
            sample_rate: audio.sample_rate,
            requested_buffer_frames: audio.requested_buffer_frames,
            first_callback_frames: audio
                .first_callback_frames
                .and_then(|frames| u32::try_from(frames).ok()),
            channel_count: audio.channel_count,
            sample_format: non_empty_or_default(&audio.sample_format, "unknown"),
            shared_mode: true,
            exclusive_mode: false,
            raw_processing: false,
            match_format: false,
            device_label: non_empty_or_default(&audio.device_label, "default"),
        }
    }

    pub fn same_stream_as(&self, other: &Self) -> bool {
        // Callback size is an observation, while endpoint and stream format define a profile.
        let same_id = self.cpal_device_id.is_some() && self.cpal_device_id == other.cpal_device_id;
        let same_host = self.audio_host == other.audio_host
            || (same_id && (self.audio_host == "default" || other.audio_host == "default"));
        self.version == other.version
            && same_host
            && self.stream_direction == other.stream_direction
            && self.cpal_device_id == other.cpal_device_id
            && self.native_device_id == other.native_device_id
            && (self.cpal_device_id.is_some() || self.device_label == other.device_label)
            && self.sample_rate == other.sample_rate
            && self.requested_buffer_frames == other.requested_buffer_frames
            && self.channel_count == other.channel_count
            && self.sample_format == other.sample_format
            && self.shared_mode == other.shared_mode
            && self.exclusive_mode == other.exclusive_mode
            && self.raw_processing == other.raw_processing
            && self.match_format == other.match_format
    }
}

pub(crate) fn default_config_path() -> PathBuf {
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
mod tests;
