use super::*;

fn clip(frames: usize, sample_rate: u32) -> AudioClip {
    AudioClip {
        samples: (0..frames)
            .map(|frame| frame as f32 / frames as f32)
            .collect(),
        channels: 1,
        sample_rate,
    }
}

fn make_renderer(clip: &AudioClip, rate: u32, volume: &PlaybackVolume) -> PreviewRenderer {
    PreviewRenderer {
        segment: PreviewSegment::new(clip, rate, Some(0.0), Some(1.0)).unwrap(),
        cursor: 0,
        gain: GainRamp::new(volume.gain(), rate),
    }
}

#[test]
fn declared_start_and_duration_choose_exact_source_frames() {
    let clip = clip(10_000, 100);
    let segment = PreviewSegment::new(&clip, 100, Some(12.345), Some(2.0)).unwrap();
    assert_eq!(segment.first_source_frame, 1234);
    assert_eq!(segment.end_source_frame, 1434);
    assert_eq!(segment.output_frames, 200);
    assert_eq!(segment.sample(&clip, 0, 0), clip.samples[1234]);
    assert_eq!(segment.sample(&clip, 199, 0), clip.samples[1433]);
}

#[test]
fn invalid_metadata_defaults_and_duration_bounds_are_predictable() {
    let clip = clip(10_000, 100);
    for start in [
        None,
        Some(-1.0),
        Some(100.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        let segment = PreviewSegment::new(&clip, 100, start, None).unwrap();
        assert_eq!(segment.first_source_frame, 4000);
        assert_eq!(segment.output_frames, 1200);
    }
    for (duration, frames) in [
        (Some(0.01), 100),
        (Some(90.0), 3000),
        (Some(-2.0), 1200),
        (Some(f64::NAN), 1200),
        (None, 1200),
    ] {
        let segment = PreviewSegment::new(&clip, 100, Some(0.0), duration).unwrap();
        assert_eq!(segment.output_frames, frames);
    }
}

#[test]
fn interpolation_resamples_and_maps_channels_without_crossing_segment_end() {
    let clip = AudioClip {
        samples: vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.8, 0.9],
        channels: 2,
        sample_rate: 2,
    };
    let segment = PreviewSegment::new(&clip, 4, Some(0.5), Some(1.0)).unwrap();
    assert_eq!(segment.output_frames, 4);
    assert!((segment.sample(&clip, 1, 0) - 0.3).abs() < 1e-6);
    assert!((segment.sample(&clip, 1, 1) - 0.4).abs() < 1e-6);
    assert_eq!(segment.sample(&clip, 1, 2), segment.sample(&clip, 1, 0));
    assert_eq!(segment.sample(&clip, 3, 0), 0.4);
    assert_eq!(segment.sample(&clip, 3, 1), 0.5);
}

#[test]
fn loop_repeats_across_callbacks_and_never_reads_outside_segment() {
    let clip = clip(2000, 1000);
    let volume = PlaybackVolume::default();
    let mut renderer = make_renderer(&clip, 1000, &volume);
    let mut first = vec![0.0; 1000];
    renderer.render(&mut first, &clip, 1, &volume);
    assert_eq!(renderer.cursor, 0);
    assert_eq!(first[100], clip.samples[100]);
    assert_eq!(first[500], clip.samples[500]);
    let mut repeated = vec![0.0; 2137];
    for block in repeated.chunks_mut(37) {
        renderer.render(block, &clip, 1, &volume);
    }
    for (index, sample) in repeated.iter().enumerate() {
        assert_eq!(*sample, first[index % 1000]);
    }
    assert_eq!(renderer.cursor, 137);
}

#[test]
fn short_tracks_and_track_end_produce_bounded_loops() {
    for frames in [1, 2, 3, 7, 39, 80] {
        let clip = clip(frames, 1000);
        let volume = PlaybackVolume::default();
        let mut renderer = make_renderer(&clip, 2000, &volume);
        assert_eq!(renderer.segment.output_frames, frames as u64 * 2);
        let mut data = vec![0.0_f32; frames * 6];
        renderer.render(&mut data, &clip, 1, &volume);
        assert!(
            data.iter()
                .all(|sample| sample.is_finite() && *sample <= 1.0)
        );
        assert_eq!(renderer.cursor, 0);
    }
    let clip = clip(10_000, 1000);
    let segment = PreviewSegment::new(&clip, 1500, Some(9.99), Some(30.0)).unwrap();
    assert_eq!(segment.first_source_frame, 9990);
    assert_eq!(segment.end_source_frame, 10_000);
    assert_eq!(segment.output_frames, 15);
    assert_eq!(segment.sample(&clip, 14, 0), clip.samples[9999]);
}

#[test]
fn envelope_is_smooth_symmetric_and_silent_at_each_edge() {
    let clip = clip(1000, 1000);
    let segment = PreviewSegment::new(&clip, 1000, Some(0.0), None).unwrap();
    assert_eq!(segment.fade_frames, 40);
    assert_eq!(segment.envelope(0), 0.0);
    assert_eq!(segment.envelope(999), 0.0);
    assert_eq!(segment.envelope(40), 1.0);
    assert!((segment.envelope(20) - 0.5).abs() < 1e-6);
    for cursor in 0..999 {
        assert_eq!(segment.envelope(cursor), segment.envelope(999 - cursor));
        assert!((segment.envelope(cursor + 1) - segment.envelope(cursor)).abs() < 0.04);
    }
}

#[test]
fn startup_mute_and_live_gain_apply_without_stopping_cursor() {
    let clip = AudioClip {
        samples: vec![1.0; 1000],
        channels: 1,
        sample_rate: 1000,
    };
    let volume = PlaybackVolume::new(0.0);
    let mut renderer = make_renderer(&clip, 1000, &volume);
    let mut initial = [9.0_f32; 100];
    renderer.render(&mut initial, &clip, 1, &volume);
    assert_eq!(initial, [0.0; 100]);
    assert_eq!(renderer.cursor, 100);
    volume.set_gain(0.5);
    let mut rise = [0.0_f32; 10];
    renderer.render(&mut rise, &clip, 1, &volume);
    assert!((rise[0] - 0.05).abs() < 1e-6);
    assert_eq!(rise[9], 0.5);
    volume.set_gain(0.0);
    let mut fall = [0.0_f32; 10];
    renderer.render(&mut fall, &clip, 1, &volume);
    assert!((fall[0] - 0.45).abs() < 1e-6);
    assert_eq!(fall[9], 0.0);
    assert_eq!(renderer.cursor, 120);
}

fn format_mute<T: SizedSample + FromSample<f32> + PartialEq + std::fmt::Debug>() {
    let clip = clip(1000, 1000);
    let volume = PlaybackVolume::new(0.0);
    let mut renderer = make_renderer(&clip, 1000, &volume);
    let mut output = [T::from_sample(1.0); 200];
    renderer.render(&mut output, &clip, 2, &volume);
    assert!(output.iter().all(|sample| *sample == T::from_sample(0.0)));
    assert_eq!(renderer.cursor, 100);
    let volume = PlaybackVolume::default();
    let mut renderer = make_renderer(&clip, 1000, &volume);
    renderer.render(&mut output, &clip, 2, &volume);
    assert_eq!(output[100], T::from_sample(clip.samples[50]));
    assert_eq!(output[101], output[100]);
}

#[test]
fn all_supported_pcm_formats_use_correct_silence() {
    format_mute::<i8>();
    format_mute::<i16>();
    format_mute::<I24>();
    format_mute::<i32>();
    format_mute::<i64>();
    format_mute::<u8>();
    format_mute::<u16>();
    format_mute::<u32>();
    format_mute::<u64>();
    format_mute::<f32>();
    format_mute::<f64>();
}

#[test]
fn malformed_clips_are_rejected_before_callback_setup() {
    for clip in [
        AudioClip {
            samples: vec![],
            channels: 1,
            sample_rate: 100,
        },
        AudioClip {
            samples: vec![0.0],
            channels: 0,
            sample_rate: 100,
        },
        AudioClip {
            samples: vec![0.0],
            channels: 1,
            sample_rate: 0,
        },
        AudioClip {
            samples: vec![0.0],
            channels: 2,
            sample_rate: 100,
        },
    ] {
        assert!(PreviewSegment::new(&clip, 100, None, None).is_err());
    }
    assert!(PreviewSegment::new(&clip(100, 100), 0, None, None).is_err());
}

#[test]
fn nonfinite_samples_are_silent_without_scanning_the_whole_clip_at_startup() {
    let mut clip = clip(1000, 1000);
    clip.samples[100] = f32::NAN;
    clip.samples[101] = f32::INFINITY;
    let segment = PreviewSegment::new(&clip, 1000, Some(0.0), None).unwrap();
    assert_eq!(segment.sample(&clip, 100, 0), 0.0);
    assert_eq!(segment.sample(&clip, 101, 0), 0.0);
    assert_eq!(segment.sample(&clip, 200, 0), clip.samples[200]);
}

#[test]
#[ignore = "requires native audio output; plays silence only"]
fn native_preview_build_play_drop_and_reopen() {
    use super::super::devices::{AudioStreamOptions, output_stream_target};
    use cpal::traits::StreamTrait;

    let target = output_stream_target(&AudioStreamOptions::default()).unwrap();
    let clip = Arc::new(AudioClip {
        samples: vec![0.0; 48_000],
        channels: 1,
        sample_rate: 48_000,
    });
    for _ in 0..3 {
        let stream = build_preview_stream(
            &target,
            clip.clone(),
            Some(0.0),
            Some(1.0),
            PlaybackVolume::new(0.0),
        )
        .unwrap();
        stream.play().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(80));
        drop(stream);
    }
}
