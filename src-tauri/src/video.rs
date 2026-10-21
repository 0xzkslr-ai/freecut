//! Portable recording geometry, color conversion, and pause-aware timestamps.
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingFormat {
    #[default]
    Mp4,
    Gif,
}

impl RecordingFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Gif => "GIF",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordingOptions {
    pub format: RecordingFormat,
    pub gif_fps: u32,
    /// Zero keeps the original size; smaller recordings are never enlarged.
    pub gif_max_edge: u32,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            format: RecordingFormat::Mp4,
            gif_fps: 10,
            gif_max_edge: 960,
        }
    }
}

impl RecordingOptions {
    pub fn validate(self) -> Result<Self, String> {
        if !matches!(self.gif_fps, 5 | 10 | 15) {
            return Err("GIF 帧率请选择 5、10 或 15 FPS。".into());
        }
        if !matches!(self.gif_max_edge, 0 | 480 | 720 | 960) {
            return Err("GIF 尺寸请选择原始尺寸，或最长边 480、720、960 像素。".into());
        }
        Ok(self)
    }

    pub fn frame_rate(self) -> u32 {
        if self.format == RecordingFormat::Gif {
            self.gif_fps
        } else {
            FRAME_RATE
        }
    }

    pub fn output_size(self, width: u32, height: u32) -> Result<(u32, u32), String> {
        self.validate()?;
        let padded = encoded_size(width, height)?;
        if self.format == RecordingFormat::Mp4 {
            return Ok(padded);
        }
        let edge = width.max(height);
        if self.gif_max_edge == 0 || edge <= self.gif_max_edge {
            return Ok((width, height));
        }
        let resize = |value: u32| {
            ((value as u64 * self.gif_max_edge as u64 + edge as u64 / 2) / edge as u64).max(1)
                as u32
        };
        Ok((resize(width), resize(height)))
    }
}

/// Resize opaque desktop BGRA pixels into RGB for palette quantization.
pub fn bgra_to_rgb(
    bgra: &[u8],
    width: u32,
    height: u32,
    output_width: u32,
    output_height: u32,
) -> Result<Vec<u8>, String> {
    encoded_size(width, height)?;
    if output_width == 0
        || output_height == 0
        || output_width > width
        || output_height > height
        || bgra.len() != width as usize * height as usize * 4
    {
        return Err("GIF 画面缓冲区或输出尺寸无效。".into());
    }
    let mut rgb = vec![0; output_width as usize * output_height as usize * 3];
    let coordinate = |value: u32, source: u32, output: u32| {
        let position = ((value as f64 + 0.5) * source as f64 / output as f64 - 0.5)
            .clamp(0., (source - 1) as f64);
        let low = position.floor() as usize;
        (
            low,
            (low + 1).min(source as usize - 1),
            position - low as f64,
        )
    };
    let columns: Vec<_> = (0..output_width)
        .map(|x| coordinate(x, width, output_width))
        .collect();
    for y in 0..output_height {
        let (top, bottom, vertical) = coordinate(y, height, output_height);
        for (x, &(left, right, horizontal)) in columns.iter().enumerate() {
            for (channel, source_channel) in [2, 1, 0].into_iter().enumerate() {
                let sample = |row: usize, column: usize| {
                    bgra[(row * width as usize + column) * 4 + source_channel] as f64
                };
                let above = sample(top, left) * (1. - horizontal) + sample(top, right) * horizontal;
                let below =
                    sample(bottom, left) * (1. - horizontal) + sample(bottom, right) * horizontal;
                rgb[(y as usize * output_width as usize + x) * 3 + channel] =
                    (above * (1. - vertical) + below * vertical).round() as u8;
            }
        }
    }
    Ok(rgb)
}

pub const FRAME_RATE: u32 = 30;

pub fn encoded_size(width: u32, height: u32) -> Result<(u32, u32), String> {
    if !(48..=3840).contains(&width) || !(48..=2160).contains(&height) {
        return Err("录屏区域宽高至少为 48 像素，最大支持 3840 × 2160 像素。".into());
    }
    // H.264 4:2:0 needs even dimensions. Repeat edge pixels instead of cropping.
    Ok(((width + 1) & !1, (height + 1) & !1))
}

pub fn bgra_to_nv12(bgra: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let (out_width, out_height) = encoded_size(width, height)?;
    if bgra.len() != width as usize * height as usize * 4 {
        return Err("录屏画面缓冲区尺寸不匹配。".into());
    }
    let (w, h) = (out_width as usize, out_height as usize);
    let mut output = vec![0; w * h * 3 / 2];
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (mut r_sum, mut g_sum, mut b_sum) = (0, 0, 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let source = (((y + dy).min(height as usize - 1) * width as usize)
                        + (x + dx).min(width as usize - 1))
                        * 4;
                    let (b, g, r) = (
                        bgra[source] as i32,
                        bgra[source + 1] as i32,
                        bgra[source + 2] as i32,
                    );
                    output[(y + dy) * w + x + dx] =
                        (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
                    r_sum += r;
                    g_sum += g;
                    b_sum += b;
                }
            }
            let (r, g, b) = (r_sum / 4, g_sum / 4, b_sum / 4);
            let uv = w * h + (y / 2) * w + x;
            output[uv] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            output[uv + 1] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
    Ok(output)
}

#[derive(Debug)]
pub struct Timeline {
    start: Duration,
    paused_at: Option<Duration>,
    paused_for: Duration,
    stopped_at: Option<Duration>,
}

impl Timeline {
    pub fn new(start: Duration) -> Self {
        Self {
            start,
            paused_at: None,
            paused_for: Duration::ZERO,
            stopped_at: None,
        }
    }

    pub fn elapsed(&self, now: Duration) -> Duration {
        self.stopped_at.unwrap_or_else(|| {
            self.paused_at
                .unwrap_or(now)
                .saturating_sub(self.start)
                .saturating_sub(self.paused_for)
        })
    }

    pub fn pause(&mut self, now: Duration) {
        if self.stopped_at.is_none() && self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    pub fn resume(&mut self, now: Duration) {
        if self.stopped_at.is_none() {
            if let Some(paused) = self.paused_at.take() {
                self.paused_for += now.saturating_sub(paused);
            }
        }
    }

    pub fn stop(&mut self, now: Duration) {
        self.stopped_at = Some(self.elapsed(now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_options_validate_and_preserve_format_specific_geometry() {
        let mp4 = RecordingOptions::default();
        assert_eq!(mp4.format.extension(), "mp4");
        assert_eq!(mp4.format.label(), "MP4");
        assert_eq!(mp4.frame_rate(), 30);
        assert_eq!(mp4.output_size(49, 51).unwrap(), (50, 52));
        let gif = RecordingOptions {
            format: RecordingFormat::Gif,
            ..mp4
        };
        assert_eq!(gif.format.extension(), "gif");
        assert_eq!(gif.format.label(), "GIF");
        assert_eq!(gif.frame_rate(), 10);
        assert_eq!(gif.output_size(49, 51).unwrap(), (49, 51));
        assert_eq!(gif.output_size(1920, 1080).unwrap(), (960, 540));
        assert_eq!(gif.output_size(1080, 1920).unwrap(), (540, 960));
        assert_eq!(
            RecordingOptions {
                gif_max_edge: 0,
                ..gif
            }
            .output_size(1921, 1081)
            .unwrap(),
            (1921, 1081)
        );
        for fps in [0, 1, 30, u32::MAX] {
            assert!(RecordingOptions {
                gif_fps: fps,
                ..gif
            }
            .validate()
            .is_err());
        }
        assert!(RecordingOptions {
            gif_max_edge: u32::MAX,
            ..gif
        }
        .validate()
        .is_err());
        assert!(gif.output_size(47, 100).is_err());
        let decoded: RecordingOptions = serde_json::from_str(r#"{"format":"gif"}"#).unwrap();
        assert_eq!(decoded.gif_fps, 10);
        assert_eq!(decoded.gif_max_edge, 960);
        assert!(serde_json::from_str::<RecordingOptions>(r#"{"format":"other"}"#).is_err());
    }

    #[test]
    fn rgb_conversion_resizes_without_padding_or_changing_channel_order() {
        let solid = [4, 20, 240, 255].repeat(49 * 51);
        let rgb = bgra_to_rgb(&solid, 49, 51, 49, 51).unwrap();
        assert_eq!(rgb, [240, 20, 4].repeat(49 * 51));
        let mut checker = vec![0; 48 * 48 * 4];
        for y in 0..48 {
            for x in 0..48 {
                let color = if (x + y) % 2 == 0 { 0 } else { 255 };
                checker[(y * 48 + x) * 4..(y * 48 + x + 1) * 4]
                    .copy_from_slice(&[color, color, color, 255]);
            }
        }
        assert_eq!(
            bgra_to_rgb(&checker, 48, 48, 24, 24).unwrap(),
            vec![128; 24 * 24 * 3]
        );
        for (width, height) in [(0, 24), (24, 0), (50, 24), (24, 52)] {
            assert!(bgra_to_rgb(&solid, 49, 51, width, height).is_err());
        }
        assert!(bgra_to_rgb(&[], 49, 51, 24, 24).is_err());
    }

    #[test]
    fn pause_resume_and_stop_remove_only_paused_time() {
        let mut clock = Timeline::new(Duration::from_secs(10));
        clock.pause(Duration::from_secs(12));
        clock.pause(Duration::from_secs(13));
        assert_eq!(
            clock.elapsed(Duration::from_secs(20)),
            Duration::from_secs(2)
        );
        clock.resume(Duration::from_secs(20));
        clock.resume(Duration::from_secs(21));
        clock.pause(Duration::from_secs(23));
        clock.resume(Duration::from_secs(25));
        clock.stop(Duration::from_secs(26));
        assert_eq!(
            clock.elapsed(Duration::from_secs(100)),
            Duration::from_secs(6)
        );
        clock.resume(Duration::from_secs(100));
        clock.stop(Duration::from_secs(100));
        assert_eq!(clock.elapsed(Duration::ZERO), Duration::from_secs(6));
    }

    #[test]
    fn stop_during_pause_and_early_clock_reads_are_stable() {
        let mut clock = Timeline::new(Duration::from_secs(5));
        assert_eq!(clock.elapsed(Duration::ZERO), Duration::ZERO);
        clock.pause(Duration::from_secs(7));
        clock.stop(Duration::from_secs(50));
        assert_eq!(
            clock.elapsed(Duration::from_secs(100)),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn validates_dimensions_and_buffers() {
        for dimensions in [
            (0, 48),
            (48, 47),
            (3841, 1080),
            (1920, 2161),
            (u32::MAX, u32::MAX),
        ] {
            assert!(encoded_size(dimensions.0, dimensions.1).is_err());
        }
        assert_eq!(encoded_size(49, 51).unwrap(), (50, 52));
        assert_eq!(encoded_size(3840, 2160).unwrap(), (3840, 2160));
        assert!(bgra_to_nv12(&[], 48, 48).is_err());
    }

    #[test]
    fn nv12_uses_limited_range_and_repeats_odd_edges() {
        for (pixel, y, u, v) in [
            ([0, 0, 0, 255], 16, 128, 128),
            ([255, 255, 255, 255], 235, 128, 128),
            ([0, 0, 255, 255], 82, 90, 240),
        ] {
            let source = pixel.repeat(49 * 51);
            let nv12 = bgra_to_nv12(&source, 49, 51).unwrap();
            assert_eq!(nv12.len(), 50 * 52 * 3 / 2);
            assert!(nv12[..50 * 52].iter().all(|value| *value == y));
            assert!(nv12[50 * 52..].chunks_exact(2).all(|uv| uv == [u, v]));
        }
        let mut source = [0, 0, 0, 255].repeat(49 * 51);
        source[(49 * 51 - 1) * 4..].copy_from_slice(&[255, 255, 255, 255]);
        let nv12 = bgra_to_nv12(&source, 49, 51).unwrap();
        assert_eq!(&nv12[50 * 50 + 48..50 * 50 + 50], &[235, 235]);
        assert_eq!(&nv12[50 * 51 + 48..50 * 51 + 50], &[235, 235]);
    }
}
