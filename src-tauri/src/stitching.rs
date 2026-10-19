//! Fixed-scale, translation-only stitching. Failed matches never advance the reference frame.
use crate::imaging::Image;

const MAX_PIXELS: u64 = 40_000_000;
const MATCH_ERROR: &str = "无法可靠对齐，已暂停。请将地图移回上一次成功采集的位置附近，再继续；保持缩放不变，缓慢移动并保留重叠区域。";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub fn from_selection(
        image: &Image,
        points: &[[f64; 2]],
        width: f64,
        height: f64,
    ) -> Result<Self, String> {
        image.validate()?;
        if points.len() != 4
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || !points.iter().flatten().all(|v| v.is_finite())
        {
            return Err("请先框选一个矩形采集区域。".into());
        }
        let left = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let right = points
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let top = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let bottom = points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let corners = [[left, top], [right, top], [right, bottom], [left, bottom]];
        if points != corners || left < 0. || top < 0. || right > width || bottom > height {
            return Err("采集区域必须是屏幕内的矩形，请重新框选。".into());
        }
        let x = (left * image.width as f64 / width).floor() as u32;
        let y = (top * image.height as f64 / height).floor() as u32;
        let right = (right * image.width as f64 / width)
            .ceil()
            .min(image.width as f64) as u32;
        let bottom = (bottom * image.height as f64 / height)
            .ceil()
            .min(image.height as f64) as u32;
        if right.saturating_sub(x) < 48 || bottom.saturating_sub(y) < 48 {
            return Err("滚动采集区域的宽高至少需要 48 个物理像素。".into());
        }
        Ok(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn extract(&self, image: &Image) -> Result<Image, String> {
        image.validate()?;
        if self.width == 0
            || self.height == 0
            || self.x as u64 + self.width as u64 > image.width as u64
            || self.y as u64 + self.height as u64 > image.height as u64
        {
            return Err("采集区域已超出屏幕，请重新截图。".into());
        }
        let mut rgba = allocation(self.width, self.height)?;
        for y in 0..self.height as usize {
            let src = ((y + self.y as usize) * image.width as usize + self.x as usize) * 4;
            let dst = y * self.width as usize * 4;
            rgba[dst..dst + self.width as usize * 4]
                .copy_from_slice(&image.rgba[src..src + self.width as usize * 4]);
        }
        Ok(Image {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}

#[derive(Clone)]
struct Gray {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

fn pyramid(image: &Image) -> Vec<Gray> {
    let mut levels = vec![Gray {
        width: image.width as i32,
        height: image.height as i32,
        pixels: image
            .rgba
            .chunks_exact(4)
            .map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8)
            .collect(),
    }];
    while levels
        .last()
        .unwrap()
        .width
        .max(levels.last().unwrap().height)
        > 48
    {
        let previous = levels.last().unwrap();
        if previous.width < 16 || previous.height < 16 {
            break;
        }
        let (width, height) = (previous.width / 2, previous.height / 2);
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let i = (y * 2 * previous.width + x * 2) as usize;
                let stride = previous.width as usize;
                pixels.push(
                    ((previous.pixels[i] as u16
                        + previous.pixels[i + 1] as u16
                        + previous.pixels[i + stride] as u16
                        + previous.pixels[i + stride + 1] as u16)
                        / 4) as u8,
                );
            }
        }
        levels.push(Gray {
            width,
            height,
            pixels,
        });
    }
    levels
}

#[derive(Clone, Copy, Debug)]
struct Match {
    x: i32,
    y: i32,
    error: f64,
    agreement: f64,
    variance: f64,
}

fn score(a: &Gray, b: &Gray, dx: i32, dy: i32) -> Option<Match> {
    let left = 0.max(-dx) + 1;
    let top = 0.max(-dy) + 1;
    let right = b.width.min(a.width - dx) - 1;
    let bottom = b.height.min(a.height - dy) - 1;
    if right - left < a.width * 3 / 10
        || bottom - top < a.height * 3 / 10
        || (right - left) * (bottom - top) < a.width * a.height / 4
    {
        return None;
    }
    let step = ((right - left).max(bottom - top) / 44).max(1) as usize;
    let (mut error, mut good, mut sum, mut squared, mut count) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for y in (top..bottom).step_by(step) {
        for x in (left..right).step_by(step) {
            let av = a.pixels[((y + dy) * a.width + x + dx) as usize] as i32;
            let bv = b.pixels[(y * b.width + x) as usize] as i32;
            let difference = (av - bv).unsigned_abs();
            error += difference.min(48) as u64;
            good += u64::from(difference <= 12);
            sum += av as u64;
            squared += (av * av) as u64;
            count += 1;
        }
    }
    if count < 36 {
        return None;
    }
    let mean = sum as f64 / count as f64;
    Some(Match {
        x: dx,
        y: dy,
        error: error as f64 / count as f64,
        agreement: good as f64 / count as f64,
        variance: squared as f64 / count as f64 - mean * mean,
    })
}

fn best_candidates(mut candidates: Vec<Match>) -> Vec<Match> {
    candidates.sort_unstable_by(|a, b| a.error.total_cmp(&b.error));
    let mut best: Vec<Match> = Vec::new();
    for candidate in candidates {
        if best
            .iter()
            .all(|m| (m.x - candidate.x).abs().max((m.y - candidate.y).abs()) > 2)
        {
            best.push(candidate);
            if best.len() == 12 {
                break;
            }
        }
    }
    best
}

fn translation(a: &[Gray], b: &[Gray]) -> Result<(i32, i32), String> {
    let level = a.len() - 1;
    let (first, second) = (&a[level], &b[level]);
    let mut candidates = Vec::new();
    for y in -first.height * 7 / 10..=first.height * 7 / 10 {
        for x in -first.width * 7 / 10..=first.width * 7 / 10 {
            if let Some(m) = score(first, second, x, y) {
                candidates.push(m);
            }
        }
    }
    candidates = best_candidates(candidates);
    for level in (0..level).rev() {
        let mut refined = Vec::new();
        for candidate in &candidates {
            for y in candidate.y * 2 - 3..=candidate.y * 2 + 3 {
                for x in candidate.x * 2 - 3..=candidate.x * 2 + 3 {
                    if let Some(m) = score(&a[level], &b[level], x, y) {
                        refined.push(m);
                    }
                }
            }
        }
        candidates = best_candidates(refined);
    }
    let best = candidates.first().ok_or(MATCH_ERROR)?;
    // A second distant hypothesis with an equally good fit means repeated/ambiguous terrain.
    let ambiguous = candidates.iter().skip(1).any(|m| {
        (m.x - best.x).abs().max((m.y - best.y).abs()) > 4
            && m.error <= best.error * 1.35 + 1.5
            && m.agreement > 0.75
    });
    if best.error > 9. || best.agreement < 0.82 || best.variance < 25. || ambiguous {
        return Err(MATCH_ERROR.into());
    }
    Ok((best.x, best.y))
}

pub struct Stitcher {
    canvas: Image,
    reference: Image,
    levels: Vec<Gray>,
    position: (i32, i32),
    origin: (i32, i32),
    frames: u32,
}

fn allocation(width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width > 32768 || height > 32768 || width as u64 * height as u64 > MAX_PIXELS {
        return Err(
            "拼接图已达到尺寸上限（4000 万像素或单边 32768 像素）。请结束并保存，分段采集。".into(),
        );
    }
    let length = width as usize * height as usize * 4;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(length)
        .map_err(|_| "内存不足，请结束并保存当前拼接图。")?;
    pixels.resize(length, 0);
    Ok(pixels)
}

impl Stitcher {
    pub fn new(image: Image) -> Result<Self, String> {
        image.validate()?;
        if image.width < 48 || image.height < 48 {
            return Err("滚动采集区域的宽高至少需要 48 个物理像素。".into());
        }
        // Validate the same limits before retaining the initial frame.
        if image.width > 32768
            || image.height > 32768
            || image.width as u64 * image.height as u64 > MAX_PIXELS
        {
            return Err("采集区域过大，请缩小区域后重试。".into());
        }
        let levels = pyramid(&image);
        Ok(Self {
            canvas: image.clone(),
            reference: image,
            levels,
            position: (0, 0),
            origin: (0, 0),
            frames: 1,
        })
    }

    pub fn image(&self) -> &Image {
        &self.canvas
    }
    pub fn frames(&self) -> u32 {
        self.frames
    }
    pub fn into_image(self) -> Image {
        self.canvas
    }

    pub fn push(&mut self, image: Image) -> Result<bool, String> {
        image.validate()?;
        if image.width != self.reference.width || image.height != self.reference.height {
            return Err("采集区域尺寸已改变，请结束本次截图后重新开始。".into());
        }
        if image.rgba == self.reference.rgba {
            return Ok(false);
        }
        let levels = pyramid(&image);
        let (dx, dy) = translation(&self.levels, &levels)?;
        if dx == 0 && dy == 0 {
            self.reference = image;
            self.levels = levels;
            return Ok(false);
        }
        let position = (self.position.0 + dx, self.position.1 + dy);
        let origin = (self.origin.0.min(position.0), self.origin.1.min(position.1));
        let right = (self.origin.0 + self.canvas.width as i32).max(position.0 + image.width as i32);
        let bottom =
            (self.origin.1 + self.canvas.height as i32).max(position.1 + image.height as i32);
        let (width, height) = ((right - origin.0) as u32, (bottom - origin.1) as u32);
        if width != self.canvas.width || height != self.canvas.height {
            let mut pixels = allocation(width, height)?;
            let offset_x = (self.origin.0 - origin.0) as usize;
            let offset_y = (self.origin.1 - origin.1) as usize;
            for y in 0..self.canvas.height as usize {
                let src = y * self.canvas.width as usize * 4;
                let dst = ((y + offset_y) * width as usize + offset_x) * 4;
                pixels[dst..dst + self.canvas.width as usize * 4]
                    .copy_from_slice(&self.canvas.rgba[src..src + self.canvas.width as usize * 4]);
            }
            self.canvas = Image {
                width,
                height,
                rgba: pixels,
            };
        }
        for y in 0..image.height as usize {
            for x in 0..image.width as usize {
                let dst = ((y + (position.1 - origin.1) as usize) * width as usize
                    + x
                    + (position.0 - origin.0) as usize)
                    * 4;
                // Preserve previously captured content; unvisited areas stay transparent.
                if self.canvas.rgba[dst + 3] == 0 {
                    let src = (y * image.width as usize + x) * 4;
                    self.canvas.rgba[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
                }
            }
        }
        self.position = position;
        self.origin = origin;
        self.reference = image;
        self.levels = levels;
        self.frames += 1;
        Ok(true)
    }
}
