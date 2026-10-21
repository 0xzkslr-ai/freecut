//! Streaming GIF output. Only the current frame is quantized in memory.
use std::io::Write;

pub(crate) struct GifEncoder<W: Write> {
    encoder: gif::Encoder<W>,
    width: u16,
    height: u16,
    written_ticks: u64,
}

impl<W: Write> GifEncoder<W> {
    pub(crate) fn new(writer: W, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 3840 || height > 2160 {
            return Err("GIF 输出尺寸无效。".into());
        }
        let mut encoder = gif::Encoder::new(writer, width as u16, height as u16, &[])
            .map_err(|error| format!("无法创建 GIF 文件：{error}"))?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|error| format!("无法设置 GIF 循环播放：{error}"))?;
        Ok(Self {
            encoder,
            width: width as u16,
            height: height as u16,
            written_ticks: 0,
        })
    }

    /// Timestamps share MP4's 100 ns units; round cumulative time to avoid drift.
    pub(crate) fn write(
        &mut self,
        rgb: &[u8],
        timestamp: i64,
        duration: i64,
    ) -> Result<(), String> {
        if rgb.len() != self.width as usize * self.height as usize * 3
            || timestamp < 0
            || duration <= 0
        {
            return Err("GIF 帧的像素或时间无效。".into());
        }
        let end = ((timestamp as u64)
            .saturating_add(duration as u64)
            .saturating_add(50_000)
            / 100_000)
            .max(self.written_ticks.saturating_add(1));
        let mut remaining = end - self.written_ticks;
        // Sampling at the library's fastest setting keeps palette work bounded.
        let mut frame = gif::Frame::from_rgb_speed(self.width, self.height, rgb, 30);
        frame.dispose = gif::DisposalMethod::Keep;
        while remaining > 0 {
            frame.delay = remaining.min(u16::MAX as u64) as u16;
            self.encoder
                .write_frame(&frame)
                .map_err(|error| format!("无法写入 GIF 画面：{error}"))?;
            remaining -= frame.delay as u64;
        }
        self.written_ticks = end;
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        self.encoder
            .into_inner()
            .map_err(|error| format!("无法完成 GIF 文件：{error}"))?
            .flush()
            .map_err(|error| format!("无法保存 GIF 文件：{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    fn frames(bytes: &[u8]) -> (Vec<u16>, Vec<Vec<u8>>) {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(Cursor::new(bytes)).unwrap();
        let mut delays = Vec::new();
        let mut pixels = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            delays.push(frame.delay);
            pixels.push(frame.buffer.to_vec());
        }
        (delays, pixels)
    }

    #[test]
    fn writes_looping_animation_with_original_colors_and_odd_dimensions() {
        let mut bytes = Vec::new();
        let mut encoder = GifEncoder::new(&mut bytes, 3, 1).unwrap();
        let red = [255, 0, 0].repeat(3);
        let blue = [0, 0, 255].repeat(3);
        encoder.write(&red, 0, 1_000_000).unwrap();
        encoder.write(&blue, 1_000_000, 2_000_000).unwrap();
        encoder.finish().unwrap();
        assert!(bytes.starts_with(b"GIF89a"));
        assert!(bytes.windows(11).any(|part| part == b"NETSCAPE2.0"));
        assert_eq!(bytes.last(), Some(&0x3b));
        let (delays, pixels) = frames(&bytes);
        assert_eq!(delays, vec![10, 20]);
        assert_eq!(
            pixels,
            vec![[255, 0, 0, 255].repeat(3), [0, 0, 255, 255].repeat(3)]
        );
    }

    #[test]
    fn fractional_frame_rate_rounds_cumulative_time_without_drift() {
        let mut bytes = Vec::new();
        let mut encoder = GifEncoder::new(&mut bytes, 1, 1).unwrap();
        for index in 0..15 {
            let start = index * 10_000_000 / 15;
            let end = (index + 1) * 10_000_000 / 15;
            encoder.write(&[0, 0, 0], start, end - start).unwrap();
        }
        encoder.finish().unwrap();
        let (delays, _) = frames(&bytes);
        assert_eq!(delays.len(), 15);
        assert_eq!(delays.into_iter().map(u32::from).sum::<u32>(), 100);
    }

    #[test]
    fn long_frame_delays_split_without_truncation() {
        let mut bytes = Vec::new();
        let mut encoder = GifEncoder::new(&mut bytes, 1, 1).unwrap();
        encoder.write(&[0, 0, 0], 0, 70_000 * 100_000).unwrap();
        encoder.finish().unwrap();
        assert_eq!(
            frames(&bytes).0,
            vec![u16::MAX, (70_000u32 - u16::MAX as u32) as u16]
        );
    }

    #[test]
    fn rejects_invalid_dimensions_and_frame_buffers() {
        assert!(GifEncoder::new(Vec::new(), 0, 10).is_err());
        assert!(GifEncoder::new(Vec::new(), 3841, 10).is_err());
        assert!(GifEncoder::new(Vec::new(), 10, 2161).is_err());
        let mut encoder = GifEncoder::new(Vec::new(), 1, 1).unwrap();
        assert!(encoder.write(&[], 0, 1).is_err());
        assert!(encoder.write(&[0, 0, 0], -1, 1).is_err());
        assert!(encoder.write(&[0, 0, 0], 0, 0).is_err());
    }

    #[derive(Default)]
    struct FailingWriter {
        remaining: usize,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.remaining {
                return Err(io::Error::other("disk full"));
            }
            self.remaining -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn propagates_creation_frame_and_finalization_errors() {
        assert!(GifEncoder::new(FailingWriter::default(), 1, 1).is_err());
        let mut encoder = GifEncoder::new(FailingWriter { remaining: 10_000 }, 1, 1).unwrap();
        encoder.encoder.get_mut().remaining = 0;
        assert!(encoder.write(&[0, 0, 0], 0, 1_000_000).is_err());
        let encoder = GifEncoder::new(FailingWriter { remaining: 10_000 }, 1, 1).unwrap();
        assert!(encoder.finish().is_err());
    }
}
