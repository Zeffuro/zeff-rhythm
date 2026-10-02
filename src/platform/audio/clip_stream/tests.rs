use super::*;
use std::time::Duration;

fn clip() -> AudioClip {
    AudioClip {
        samples: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
        channels: 1,
        sample_rate: 100,
    }
}

fn render<T: SizedSample + FromSample<f32>>(
    data: &mut [T],
    clip: &AudioClip,
    clock: &PlaybackClock,
    cursor: &mut u64,
    at: Instant,
) {
    render_callback(
        data,
        clip,
        clock,
        &PlaybackVolume::default(),
        &mut GainRamp::new(1.0, 100),
        cursor,
        1,
        100,
        6,
        at,
        Duration::from_millis(20),
    );
}

#[test]
fn pause_and_resume_preserve_exact_source_samples_and_cursor() {
    let clip = clip();
    let clock = PlaybackClock::new(clip.duration_seconds());
    let start = Instant::now();
    let mut cursor = 0;
    let mut data = [9.0_f32; 2];
    render(&mut data, &clip, &clock, &mut cursor, start);
    assert_eq!(data, [0.1, 0.2]);
    assert_eq!(cursor, 2);
    clock.set_paused(true);
    for block in 1..5 {
        data.fill(9.0);
        render(
            &mut data,
            &clip,
            &clock,
            &mut cursor,
            start + Duration::from_secs(block),
        );
        assert_eq!(data, [0.0, 0.0]);
        assert_eq!(cursor, 2);
        assert_eq!(clock.scheduled_time_seconds(), 0.02);
    }
    clock.set_paused(false);
    render(
        &mut data,
        &clip,
        &clock,
        &mut cursor,
        start + Duration::from_secs(5),
    );
    assert_eq!(data, [0.3, 0.4]);
    assert_eq!(cursor, 4);
    render(
        &mut data,
        &clip,
        &clock,
        &mut cursor,
        start + Duration::from_secs(6),
    );
    assert_eq!(data, [0.5, 0.6]);
    assert_eq!(cursor, 6);
}

#[test]
fn unsigned_pcm_pause_uses_equilibrium_not_integer_zero() {
    let clip = clip();
    let clock = PlaybackClock::new(clip.duration_seconds());
    clock.set_paused(true);
    let mut cursor = 0;
    let mut data = [0_u16; 4];
    render(&mut data, &clip, &clock, &mut cursor, Instant::now());
    assert_eq!(data, [32_768; 4]);
    assert_eq!(cursor, 0);
}

#[test]
fn resumed_resampled_stereo_frames_keep_channel_mapping() {
    let clip = AudioClip {
        samples: vec![0.2, 0.6, 0.4, 0.8, 0.6, 1.0],
        channels: 2,
        sample_rate: 100,
    };
    let clock = PlaybackClock::new(clip.duration_seconds());
    let start = Instant::now();
    let mut cursor = 0;
    let mut data = [0.0_f32; 4];
    render_callback(
        &mut data,
        &clip,
        &clock,
        &PlaybackVolume::default(),
        &mut GainRamp::new(1.0, 200),
        &mut cursor,
        2,
        200,
        6,
        start,
        Duration::ZERO,
    );
    assert_eq!(data, [0.2, 0.6, 0.3, 0.70000005]);
    clock.set_paused(true);
    render_callback(
        &mut data,
        &clip,
        &clock,
        &PlaybackVolume::default(),
        &mut GainRamp::new(1.0, 200),
        &mut cursor,
        2,
        200,
        6,
        start + Duration::from_secs(1),
        Duration::ZERO,
    );
    assert_eq!(data, [0.0; 4]);
    assert_eq!(cursor, 2);
    clock.set_paused(false);
    render_callback(
        &mut data,
        &clip,
        &clock,
        &PlaybackVolume::default(),
        &mut GainRamp::new(1.0, 200),
        &mut cursor,
        2,
        200,
        6,
        start + Duration::from_secs(2),
        Duration::ZERO,
    );
    assert_eq!(data, [0.4, 0.8, 0.5, 0.9]);
    assert_eq!(cursor, 4);
}

#[test]
fn exhausted_callback_silence_still_obeys_pause_gate() {
    let clip = clip();
    let clock = PlaybackClock::new(clip.duration_seconds());
    let start = Instant::now();
    let mut cursor = 6;
    let mut data = [9.0_f32; 2];
    render(&mut data, &clip, &clock, &mut cursor, start);
    assert_eq!(data, [0.0; 2]);
    assert_eq!(cursor, 8);
    clock.set_paused(true);
    render(
        &mut data,
        &clip,
        &clock,
        &mut cursor,
        start + Duration::from_secs(1),
    );
    assert_eq!(data, [0.0; 2]);
    assert_eq!(cursor, 8);
    clock.set_paused(false);
    render(
        &mut data,
        &clip,
        &clock,
        &mut cursor,
        start + Duration::from_secs(2),
    );
    assert_eq!(data, [0.0; 2]);
    assert_eq!(cursor, 10);
}

fn assert_format_volume<T>()
where
    T: SizedSample + FromSample<f32> + PartialEq + std::fmt::Debug,
{
    let clip = clip();
    for gain in [0.0, 0.25, 1.0] {
        let volume = PlaybackVolume::new(gain);
        let mut ramp = GainRamp::new(gain, 100);
        let clock = PlaybackClock::new(clip.duration_seconds());
        let mut cursor = 0;
        let mut data = [T::from_sample(1.0); 8];
        render_callback(
            &mut data,
            &clip,
            &clock,
            &volume,
            &mut ramp,
            &mut cursor,
            1,
            100,
            6,
            Instant::now(),
            Duration::ZERO,
        );
        let expected: Vec<_> = clip
            .samples
            .iter()
            .map(|sample| T::from_sample(sample * gain))
            .chain([T::from_sample(0.0); 2])
            .collect();
        assert_eq!(data.as_slice(), expected);
        assert_eq!(cursor, 8);
        assert_eq!(clock.scheduled_time_seconds(), 0.08);
    }
}

#[test]
fn pcm_volume_attenuation_and_silence_cover_every_output_format() {
    assert_format_volume::<i8>();
    assert_format_volume::<i16>();
    assert_format_volume::<I24>();
    assert_format_volume::<i32>();
    assert_format_volume::<i64>();
    assert_format_volume::<u8>();
    assert_format_volume::<u16>();
    assert_format_volume::<u32>();
    assert_format_volume::<u64>();
    assert_format_volume::<f32>();
    assert_format_volume::<f64>();
}

#[test]
fn pcm_ramp_spans_callbacks_and_reaches_silence_after_ten_milliseconds() {
    let clip = AudioClip {
        samples: vec![1.0; 2_000],
        channels: 2,
        sample_rate: 48_000,
    };
    let volume = PlaybackVolume::default();
    let mut ramp = GainRamp::new(1.0, 48_000);
    let clock = PlaybackClock::new(clip.duration_seconds());
    let start = Instant::now();
    let mut cursor = 0;
    let mut previous = 1.0;
    volume.set_gain(0.0);
    for block in 0..4 {
        let mut data = [9.0_f32; 256];
        render_callback(
            &mut data,
            &clip,
            &clock,
            &volume,
            &mut ramp,
            &mut cursor,
            2,
            48_000,
            1_000,
            start + Duration::from_secs_f64(block as f64 * 128.0 / 48_000.0),
            Duration::ZERO,
        );
        for frame in data.chunks_exact(2) {
            assert_eq!(frame[0], frame[1]);
            assert!((0.0..=previous).contains(&frame[0]));
            assert!(previous - frame[0] <= 1.0 / 480.0 + 0.00002);
            previous = frame[0];
        }
        if block == 3 {
            assert_eq!(&data[190..], &[0.0; 66]);
        }
    }
    assert_eq!(cursor, 512);
    assert_eq!(clock.scheduled_time_seconds(), 512.0 / 48_000.0);
}

#[test]
fn live_volume_changes_are_per_frame_and_leave_resampling_clock_and_pause_intact() {
    let clip = AudioClip {
        samples: vec![0.2, 0.6, 0.4, 0.8, 0.6, 1.0],
        channels: 2,
        sample_rate: 100,
    };
    let volume = PlaybackVolume::default();
    let mut ramp = GainRamp::new(volume.gain(), 200);
    let clock = PlaybackClock::new(clip.duration_seconds());
    let start = Instant::now();
    let mut cursor = 0;
    let mut data = [9.0_f32; 4];
    volume.set_gain(0.0);
    render_callback(
        &mut data,
        &clip,
        &clock,
        &volume,
        &mut ramp,
        &mut cursor,
        2,
        200,
        6,
        start,
        Duration::from_millis(20),
    );
    assert_eq!(data, [0.1, 0.3, 0.0, 0.0]);
    assert_eq!(cursor, 2);
    assert_eq!(clock.scheduled_time_seconds(), 0.01);
    assert_eq!(clock.song_time_at(start + Duration::from_millis(5)), 0.005);
    assert_eq!(clock.output_latency_seconds(), Some(0.02));
    clock.set_paused(true);
    volume.set_gain(0.5);
    render_callback(
        &mut data,
        &clip,
        &clock,
        &volume,
        &mut ramp,
        &mut cursor,
        2,
        200,
        6,
        start + Duration::from_millis(10),
        Duration::from_millis(20),
    );
    assert_eq!(data, [0.0; 4]);
    assert_eq!(cursor, 2);
    assert_eq!(clock.scheduled_time_seconds(), 0.01);
    assert_eq!(clock.song_time_at(start + Duration::from_secs(1)), 0.01);
    clock.set_paused(false);
    render_callback(
        &mut data,
        &clip,
        &clock,
        &volume,
        &mut ramp,
        &mut cursor,
        2,
        200,
        6,
        start + Duration::from_secs(2),
        Duration::from_millis(20),
    );
    assert_eq!(data, [0.2, 0.4, 0.25, 0.45]);
    assert_eq!(cursor, 4);
    assert_eq!(clock.scheduled_time_seconds(), 0.02);
    assert_eq!(clock.song_time_at(start + Duration::from_secs(2)), 0.01);
}
