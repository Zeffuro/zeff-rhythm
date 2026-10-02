use std::error::Error;
use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

const MAX_INITIAL_ALLOCATION_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct AudioClip {
    pub samples: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
}

impl AudioClip {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frame_count() as f64 / self.sample_rate as f64
    }

    pub fn sample_interpolated(&self, frame_position: f64, channel: usize) -> f32 {
        if frame_position < 0.0 || self.samples.is_empty() {
            return 0.0;
        }

        let frame_count = self.frame_count();
        if frame_count == 0 {
            return 0.0;
        }

        let lower = frame_position.floor() as usize;
        if lower >= frame_count {
            return 0.0;
        }

        let upper = lower + 1;
        let lower_sample = self.sample_at(lower, channel);
        if upper >= frame_count {
            return lower_sample;
        }

        let upper_sample = self.sample_at(upper, channel);
        let mix = (frame_position - lower as f64) as f32;
        lower_sample + (upper_sample - lower_sample) * mix
    }

    fn sample_at(&self, frame: usize, channel: usize) -> f32 {
        self.samples[frame * self.channels + channel]
    }
}

pub fn load_audio_clip(path: &Path) -> Result<AudioClip, Box<dyn Error>> {
    let file = Box::new(File::open(path)?);
    let mss = MediaSourceStream::new(file, Default::default());
    let mut hint = Hint::new();

    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let mut format = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let (track_id, frame_count_hint, mut decoder) = {
        let track = format
            .default_track(TrackType::Audio)
            .ok_or("audio file has no audio track")?;
        let codec_params = track
            .codec_params
            .as_ref()
            .ok_or("audio track has no codec parameters")?;
        let audio_params = codec_params.audio().ok_or("selected track is not audio")?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;

        (track.id, track.num_frames, decoder)
    };

    let mut samples = Vec::new();
    let mut sample_rate = None;
    let mut channels = None;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err("audio stream changed tracks while decoding".into());
            }
            Err(error) => return Err(error.into()),
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => continue,
            Err(error) => return Err(error.into()),
        };

        let decoded_channels = decoded.spec().channels().count();
        if decoded_channels == 0 {
            continue;
        }

        match (sample_rate, channels) {
            (None, None) => {
                sample_rate = Some(decoded.spec().rate());
                channels = Some(decoded_channels);
                samples.try_reserve_exact(initial_sample_capacity(
                    frame_count_hint,
                    decoded_channels,
                ))?;
            }
            (Some(rate), Some(count))
                if rate == decoded.spec().rate() && count == decoded_channels => {}
            _ => return Err("audio format changed while decoding".into()),
        }

        let start = samples.len();
        samples.resize(start + decoded.samples_interleaved(), 0.0f32);
        decoded.copy_to_slice_interleaved(&mut samples[start..]);
    }

    let sample_rate = sample_rate.ok_or("audio decode produced no samples")?;
    let channels = channels.ok_or("audio decode produced no channels")?;

    // The cache counts retained allocation, including unused growth capacity.
    samples.shrink_to_fit();

    Ok(AudioClip {
        samples,
        channels,
        sample_rate,
    })
}

fn initial_sample_capacity(frame_count: Option<u64>, channels: usize) -> usize {
    // Metadata is only a bounded hint; larger clips still grow while decoding.
    frame_count
        .and_then(|frames| usize::try_from(frames).ok())
        .and_then(|frames| frames.checked_mul(channels))
        .filter(|samples| *samples <= MAX_INITIAL_ALLOCATION_BYTES / size_of::<f32>())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
