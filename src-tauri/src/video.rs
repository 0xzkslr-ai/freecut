//! Portable recording geometry, color conversion, and pause-aware timestamps.
use std::time::Duration;

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
