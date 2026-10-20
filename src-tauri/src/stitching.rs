//! Fixed-scale, translation-only stitching. Failed matches never advance the reference frame.
use crate::imaging::Image;
use rustfft::{num_complex::Complex, FftPlanner};
use std::{
    collections::VecDeque,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

const MAX_PIXELS: u64 = 40_000_000;
const MATCH_ERROR: &str = "正在重新定位，已有画面已保留。请放慢移动，或返回刚才经过的区域。";

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
    texture_floor: f64,
}

fn pyramid(image: &Image) -> Vec<Gray> {
    let mut levels = vec![Gray {
        width: image.width as i32,
        height: image.height as i32,
        texture_floor: 12.,
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
        let texture_floor = (previous.texture_floor / 4.).max(0.05);
        levels.push(Gray {
            width,
            height,
            pixels,
            texture_floor,
        });
    }
    levels
}

#[derive(Clone, Copy, Debug)]
struct Match {
    x: i32,
    y: i32,
    error: f64,
    pixel_error: f64,
    agreement: f64,
    variance: f64,
}

fn score(a: &Gray, b: &Gray, dx: i32, dy: i32) -> Option<Match> {
    let left = 0.max(-dx) + 1;
    let top = 0.max(-dy) + 1;
    let right = b.width.min(a.width - dx) - 1;
    let bottom = b.height.min(a.height - dy) - 1;
    if right - left < (a.width / 5).max(6)
        || bottom - top < (a.height / 5).max(6)
        || (right - left) * (bottom - top) < a.width * a.height / 6
    {
        return None;
    }
    let mut tiles = Vec::new();
    let mut variance = 0.;
    let (mut pixel_error, mut samples) = (0., 0.);
    for row in 0..4 {
        for col in 0..4 {
            let x0 = left + (right - left) * col / 4;
            let x1 = left + (right - left) * (col + 1) / 4;
            let y0 = top + (bottom - top) * row / 4;
            let y1 = top + (bottom - top) * (row + 1) / 4;
            let step = ((x1 - x0).max(y1 - y0) / 9).max(1) as usize;
            let (mut sa, mut sb, mut aa, mut bb, mut ab, mut n) = (0., 0., 0., 0., 0., 0.);
            for y in (y0..y1).step_by(step) {
                for x in (x0..x1).step_by(step) {
                    let av = a.pixels[((y + dy) * a.width + x + dx) as usize] as f64;
                    let bv = b.pixels[(y * b.width + x) as usize] as f64;
                    sa += av;
                    sb += bv;
                    aa += av * av;
                    bb += bv * bv;
                    ab += av * bv;
                    pixel_error += (av - bv).abs();
                    samples += 1.;
                    n += 1.;
                }
            }
            if n < 4. {
                continue;
            }
            let va = (aa - sa * sa / n).max(0.) / n;
            let vb = (bb - sb * sb / n).max(0.) / n;
            if va < a.texture_floor || vb < b.texture_floor {
                continue;
            }
            let correlation = ((ab - sa * sb / n) / (va * vb).sqrt() / n).clamp(-1., 1.);
            tiles.push(1. - correlation);
            variance += va;
        }
    }
    if tiles.len() < 4 {
        return None;
    }
    tiles.sort_unstable_by(f64::total_cmp);
    let agreement = tiles.iter().filter(|&&v| v < 0.22).count() as f64 / tiles.len() as f64;
    // A minority of animated tiles may disagree; static textured tiles must agree spatially.
    let keep = (tiles.len() * 3 / 4).max(4);
    Some(Match {
        x: dx,
        y: dy,
        error: tiles[..keep].iter().sum::<f64>() / keep as f64 * 32.,
        pixel_error: pixel_error / samples,
        agreement,
        variance: variance / tiles.len() as f64,
    })
}

fn compare_matches(a: &Match, b: &Match) -> std::cmp::Ordering {
    // Exact correlation can differ by floating-point noise. When the robust
    // scores tie, include samples from every tile so a one-pixel mismatch in
    // discarded tiles cannot beat the exact translation. Clamping only the
    // numerical zero region keeps this comparator transitive.
    a.error
        .max(1e-9)
        .total_cmp(&b.error.max(1e-9))
        .then_with(|| a.pixel_error.total_cmp(&b.pixel_error))
        .then_with(|| a.y.cmp(&b.y))
        .then_with(|| a.x.cmp(&b.x))
}

fn pixel_error(a: &Gray, b: &Gray, dx: i32, dy: i32) -> f64 {
    let left = 0.max(-dx) + 1;
    let top = 0.max(-dy) + 1;
    let right = b.width.min(a.width - dx) - 1;
    let bottom = b.height.min(a.height - dy) - 1;
    let difference = |x: i32, y: i32| {
        let first = a.pixels[((y + dy) * a.width + x + dx) as usize];
        let second = b.pixels[(y * b.width + x) as usize];
        first.abs_diff(second) as u64
    };
    let mut sum = 0;
    // Full rows and columns cover every pixel phase along each axis. A second
    // sparse grid can miss exactly the same edges as the correlation samples.
    for stripe in 0..8 {
        let x = left + (right - left - 1) * stripe / 7;
        let y = top + (bottom - top - 1) * stripe / 7;
        for row in top..bottom {
            sum += difference(x, row);
        }
        for col in left..right {
            sum += difference(col, y);
        }
    }
    sum as f64 / (8 * (right - left + bottom - top)) as f64
}

fn refine_pixel_alignment(a: &Gray, b: &Gray, best: Match) -> Match {
    let rank = best.error.max(1e-9);
    let mut tied = Vec::new();
    // Candidate suppression merges peaks within two pixels. Recover those
    // neighbours before resolving sparse-score ties against independent pixels.
    for y in best.y - 2..=best.y + 2 {
        for x in best.x - 2..=best.x + 2 {
            if let Some(candidate) = score(a, b, x, y) {
                if candidate.error.max(1e-9) == rank {
                    tied.push(candidate);
                }
            }
        }
    }
    if tied.len() <= 1 {
        return best;
    }
    for candidate in &mut tied {
        candidate.pixel_error = pixel_error(a, b, candidate.x, candidate.y);
    }
    tied.into_iter().min_by(compare_matches).unwrap_or(best)
}

fn best_candidates(mut candidates: Vec<Match>) -> Vec<Match> {
    candidates.sort_unstable_by(compare_matches);
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

fn fft2(
    data: &mut [Complex<f32>],
    width: usize,
    height: usize,
    inverse: bool,
    planner: &mut FftPlanner<f32>,
) {
    let row = if inverse {
        planner.plan_fft_inverse(width)
    } else {
        planner.plan_fft_forward(width)
    };
    for line in data.chunks_exact_mut(width) {
        row.process(line);
    }
    let column = if inverse {
        planner.plan_fft_inverse(height)
    } else {
        planner.plan_fft_forward(height)
    };
    let mut values = vec![Complex::new(0., 0.); height];
    for x in 0..width {
        for y in 0..height {
            values[y] = data[y * width + x];
        }
        column.process(&mut values);
        for y in 0..height {
            data[y * width + x] = values[y];
        }
    }
}

fn phase_candidates(a: &Gray, b: &Gray, planner: &mut FftPlanner<f32>) -> Vec<Match> {
    let width = (a.width as usize * 2).next_power_of_two();
    let height = (a.height as usize * 2).next_power_of_two();
    let spectrum = |image: &Gray| {
        let mut values = vec![Complex::new(0., 0.); width * height];
        let mean = image.pixels.iter().map(|&p| p as f32).sum::<f32>() / image.pixels.len() as f32;
        for y in 1..image.height - 1 {
            for x in 1..image.width - 1 {
                let i = (y * image.width + x) as usize;
                let wx = (std::f32::consts::PI * x as f32 / (image.width - 1) as f32).sin();
                let wy = (std::f32::consts::PI * y as f32 / (image.height - 1) as f32).sin();
                values[y as usize * width + x as usize].re =
                    (image.pixels[i] as f32 - mean) * wx * wy;
            }
        }
        values
    };
    let mut first = spectrum(a);
    let mut second = spectrum(b);
    fft2(&mut first, width, height, false, planner);
    fft2(&mut second, width, height, false, planner);
    for (a, b) in first.iter_mut().zip(second) {
        let cross = *a * b.conj();
        *a = cross / cross.norm().max(1e-8);
    }
    fft2(&mut first, width, height, true, planner);
    let mut peaks = Vec::new();
    for y in -a.height * 4 / 5..=a.height * 4 / 5 {
        for x in -a.width * 4 / 5..=a.width * 4 / 5 {
            let index =
                y.rem_euclid(height as i32) as usize * width + x.rem_euclid(width as i32) as usize;
            peaks.push((first[index].re, x, y));
        }
    }
    peaks.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    let mut candidates: Vec<Match> = Vec::new();
    for (_, x, y) in peaks {
        if candidates
            .iter()
            .all(|m| (m.x - x).abs().max((m.y - y).abs()) > 2)
        {
            if let Some(candidate) = score(a, b, x, y) {
                candidates.push(candidate);
            }
        }
        if candidates.len() == 10 {
            break;
        }
    }
    candidates
}

fn translation(a: &[Gray], b: &[Gray], planner: &mut FftPlanner<f32>) -> Result<Match, String> {
    let level = a
        .iter()
        .position(|g| g.width.max(g.height) <= 128)
        .unwrap_or(a.len() - 1);
    let (first, second) = (&a[level], &b[level]);
    let mut candidates = phase_candidates(first, second, planner);
    if let Some(stationary) = score(first, second, 0, 0) {
        candidates.push(stationary);
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
    let best = candidates.first().copied().ok_or(MATCH_ERROR)?;
    let best = refine_pixel_alignment(&a[0], &b[0], best);
    // A second distant hypothesis with an equally good fit means repeated/ambiguous terrain.
    let ambiguous = candidates.iter().skip(1).any(|m| {
        (m.x - best.x).abs().max((m.y - best.y).abs()) > 4
            && m.error <= best.error * 1.15 + 0.1
            && m.agreement > 0.8
    });
    if best.error > 4.5 || best.agreement < 0.65 || best.variance < 20. || ambiguous {
        return Err(MATCH_ERROR.into());
    }
    Ok(best)
}

pub struct Stitcher {
    canvas: OnceLock<Image>,
    reference: Located,
    pending: Option<Located>,
    keyframes: Vec<Keyframe>,
    edges: Vec<Edge>,
    cache: VecDeque<(usize, Arc<Frame>)>,
    unplaced: VecDeque<Arc<Frame>>,
    archive: tempfile::TempDir,
    archived_bytes: usize,
    planner: FftPlanner<f32>,
    misses: u32,
    attempts: u32,
    search_cursor: usize,
    frames: u32,
}

struct Frame {
    image: Image,
    levels: Vec<Gray>,
}
#[derive(Clone)]
struct Located {
    frame: Arc<Frame>,
    anchor: usize,
    offset: (i32, i32),
}
struct Keyframe {
    position: (f64, f64),
    path: PathBuf,
}
struct Edge {
    from: usize,
    to: usize,
    delta: (f64, f64),
    weight: f64,
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct CoverageRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, serde::Serialize)]
pub struct Coverage {
    pub bounds: CoverageRect,
    pub viewport: CoverageRect,
    pub regions: Vec<CoverageRect>,
    pub unplaced: usize,
}

fn uncovered(rect: CoverageRect, covering: &[CoverageRect]) -> bool {
    let mut remaining = vec![rect];
    for cover in covering {
        let mut next = Vec::new();
        for r in remaining {
            let left = r.x.max(cover.x);
            let top = r.y.max(cover.y);
            let right = (r.x + r.width as i32).min(cover.x + cover.width as i32);
            let bottom = (r.y + r.height as i32).min(cover.y + cover.height as i32);
            if left >= right || top >= bottom {
                next.push(r);
                continue;
            }
            for (x, y, w, h) in [
                (r.x, r.y, r.width as i32, top - r.y),
                (r.x, bottom, r.width as i32, r.y + r.height as i32 - bottom),
                (r.x, top, left - r.x, bottom - top),
                (right, top, r.x + r.width as i32 - right, bottom - top),
            ] {
                if w > 0 && h > 0 {
                    next.push(CoverageRect {
                        x,
                        y,
                        width: w as u32,
                        height: h as u32,
                    });
                }
            }
        }
        if next.is_empty() {
            return false;
        }
        remaining = next;
    }
    !remaining.is_empty()
}

pub fn is_alignment_error(error: &str) -> bool {
    error == MATCH_ERROR
}

fn decode_frame(path: &std::path::Path) -> Result<Arc<Frame>, String> {
    let bytes = std::fs::read(path).map_err(|_| "无法读取采集缓存，请检查临时目录。")?;
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .map_err(|_| "采集缓存 PNG 无法读取。")?;
    let size = reader.output_buffer_size().ok_or("采集缓存尺寸无效。")?;
    let mut rgba = vec![0; size];
    let info = reader
        .next_frame(&mut rgba)
        .map_err(|_| "采集缓存 PNG 无法解码。")?;
    rgba.truncate(info.buffer_size());
    let image = Image {
        width: info.width,
        height: info.height,
        rgba,
    };
    image.validate()?;
    let levels = pyramid(&image);
    Ok(Arc::new(Frame { image, levels }))
}

fn check_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width > 32768 || height > 32768 || width as u64 * height as u64 > MAX_PIXELS {
        return Err(
            "拼接图已达到尺寸上限（4000 万像素或单边 32768 像素）。请结束并保存，分段采集。".into(),
        );
    }
    Ok(())
}

fn allocation(width: u32, height: u32) -> Result<Vec<u8>, String> {
    check_dimensions(width, height)?;
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
        let archive = tempfile::Builder::new()
            .prefix("freecut-map-")
            .tempdir()
            .map_err(|_| "无法创建滚动采集缓存目录。")?;
        let path = archive.path().join("0.png");
        let png = image.png()?;
        std::fs::write(&path, &png).map_err(|_| "无法写入滚动采集缓存。")?;
        let frame = Arc::new(Frame { image, levels });
        Ok(Self {
            canvas: OnceLock::new(),
            reference: Located {
                frame: frame.clone(),
                anchor: 0,
                offset: (0, 0),
            },
            pending: None,
            keyframes: vec![Keyframe {
                position: (0., 0.),
                path,
            }],
            edges: Vec::new(),
            cache: VecDeque::from([(0, frame)]),
            unplaced: VecDeque::new(),
            archive,
            archived_bytes: png.len(),
            planner: FftPlanner::new(),
            misses: 0,
            attempts: 0,
            search_cursor: 0,
            frames: 1,
        })
    }

    pub fn image(&self) -> &Image {
        self.canvas.get_or_init(|| {
            self.render()
                .unwrap_or_else(|_| self.reference.frame.image.clone())
        })
    }
    pub fn frames(&self) -> u32 {
        self.frames
    }
    pub fn into_image(mut self) -> Image {
        self.finalize()
            .unwrap_or_else(|_| self.reference.frame.image.clone())
    }

    fn position(&self, located: &Located) -> (i32, i32) {
        let base = self.keyframes[located.anchor].position;
        (
            base.0.round() as i32 + located.offset.0,
            base.1.round() as i32 + located.offset.1,
        )
    }

    fn rect(&self, located: &Located) -> CoverageRect {
        let (x, y) = self.position(located);
        CoverageRect {
            x,
            y,
            width: located.frame.image.width,
            height: located.frame.image.height,
        }
    }

    pub fn coverage(&self) -> Coverage {
        let (width, height) = (
            self.reference.frame.image.width,
            self.reference.frame.image.height,
        );
        let mut regions: Vec<_> = self
            .keyframes
            .iter()
            .map(|k| CoverageRect {
                x: k.position.0.round() as i32,
                y: k.position.1.round() as i32,
                width,
                height,
            })
            .collect();
        if let Some(pending) = &self.pending {
            regions.push(self.rect(pending));
        }
        let left = regions.iter().map(|r| r.x).min().unwrap_or(0);
        let top = regions.iter().map(|r| r.y).min().unwrap_or(0);
        let right = regions
            .iter()
            .map(|r| r.x + r.width as i32)
            .max()
            .unwrap_or(width as i32);
        let bottom = regions
            .iter()
            .map(|r| r.y + r.height as i32)
            .max()
            .unwrap_or(height as i32);
        Coverage {
            bounds: CoverageRect {
                x: left,
                y: top,
                width: (right - left) as u32,
                height: (bottom - top) as u32,
            },
            viewport: self.rect(&self.reference),
            regions,
            unplaced: self.unplaced.len(),
        }
    }

    fn cached_frame(&mut self, index: usize) -> Result<Arc<Frame>, String> {
        if let Some((_, frame)) = self.cache.iter().find(|(i, _)| *i == index) {
            return Ok(frame.clone());
        }
        let frame = decode_frame(&self.keyframes[index].path)?;
        self.cache.push_back((index, frame.clone()));
        let bytes = self.reference.frame.image.rgba.len().max(1);
        let limit = (64 * 1024 * 1024 / (bytes * 4 / 3)).clamp(1, 8);
        while self.cache.len() > limit {
            self.cache.pop_front();
        }
        Ok(frame)
    }

    fn optimize(&mut self) {
        self.canvas.take();
        for _ in 0..24 {
            let mut movement: f64 = 0.;
            for node in 1..self.keyframes.len() {
                let (mut x, mut y, mut weight) = (0., 0., 0.);
                for edge in &self.edges {
                    let estimate = if edge.to == node {
                        Some((
                            self.keyframes[edge.from].position.0 + edge.delta.0,
                            self.keyframes[edge.from].position.1 + edge.delta.1,
                        ))
                    } else if edge.from == node {
                        Some((
                            self.keyframes[edge.to].position.0 - edge.delta.0,
                            self.keyframes[edge.to].position.1 - edge.delta.1,
                        ))
                    } else {
                        None
                    };
                    if let Some(p) = estimate {
                        x += p.0 * edge.weight;
                        y += p.1 * edge.weight;
                        weight += edge.weight;
                    }
                }
                if weight > 0. {
                    let next = (x / weight, y / weight);
                    let old = self.keyframes[node].position;
                    movement = movement.max((old.0 - next.0).abs().max((old.1 - next.1).abs()));
                    self.keyframes[node].position = next;
                }
            }
            if movement < 0.05 {
                break;
            }
        }
    }

    fn archive_frame(&mut self, located: &Located) -> Result<(), String> {
        if self.keyframes.len() >= 1024 {
            return Err("关键画面已达上限，请结束并分段采集。".into());
        }
        let png = located.frame.image.png()?;
        if self.archived_bytes + png.len() > 512 * 1024 * 1024 {
            return Err("采集缓存已达 512 MB，请结束并保存当前图片。".into());
        }
        let index = self.keyframes.len();
        let path = self.archive.path().join(format!("{index}.png"));
        std::fs::write(&path, &png).map_err(|_| "采集缓存写入失败，请检查磁盘空间。")?;
        let position = self.position(located);
        self.keyframes.push(Keyframe {
            position: (position.0 as f64, position.1 as f64),
            path,
        });
        self.edges.push(Edge {
            from: located.anchor,
            to: index,
            delta: (located.offset.0 as f64, located.offset.1 as f64),
            weight: 1.,
        });
        self.archived_bytes += png.len();
        self.cache.push_back((index, located.frame.clone()));
        let limit =
            (64 * 1024 * 1024 / (located.frame.image.rgba.len().max(1) * 4 / 3)).clamp(1, 8);
        while self.cache.len() > limit {
            self.cache.pop_front();
        }
        Ok(())
    }

    fn locate(&mut self, frame: Arc<Frame>) -> Result<Located, String> {
        let local = translation(
            &self.reference.frame.levels,
            &frame.levels,
            &mut self.planner,
        )
        .ok();
        let predicted = local.map(|m| Located {
            frame: frame.clone(),
            anchor: self.reference.anchor,
            offset: (self.reference.offset.0 + m.x, self.reference.offset.1 + m.y),
        });
        if local.is_some() && self.attempts % 6 != 0 && self.misses == 0 {
            return Ok(predicted.unwrap());
        }
        let center = predicted
            .as_ref()
            .map(|p| self.position(p))
            .unwrap_or_else(|| self.position(&self.reference));
        let mut indices: Vec<_> = (0..self.keyframes.len()).collect();
        indices.sort_unstable_by_key(|&i| {
            let p = self.keyframes[i].position;
            ((p.0 - center.0 as f64).abs() + (p.1 - center.1 as f64).abs()) as u64
        });
        let budget = if self.misses > 0 { 5 } else { 2 };
        indices.truncate(budget);
        if self.misses > 0 {
            let index = self.search_cursor % self.keyframes.len();
            if !indices.contains(&index) {
                indices.push(index);
            }
            self.search_cursor += 1;
        }
        let mut found: Vec<(usize, Match)> = Vec::new();
        for index in indices {
            let anchor = self.cached_frame(index)?;
            if let Ok(m) = translation(&anchor.levels, &frame.levels, &mut self.planner) {
                found.push((index, m));
            }
        }
        found.sort_unstable_by(|a, b| compare_matches(&a.1, &b.1));
        if let Some(&(anchor, m)) = found.first() {
            let located = Located {
                frame: frame.clone(),
                anchor,
                offset: (m.x, m.y),
            };
            let position = self.position(&located);
            let ambiguous = found.iter().skip(1).any(|&(other, fit)| {
                let p = self.keyframes[other].position;
                (p.0 + fit.x as f64 - position.0 as f64)
                    .abs()
                    .max((p.1 + fit.y as f64 - position.1 as f64).abs())
                    > 8.
                    && fit.error <= m.error * 1.15 + 0.1
            });
            if !ambiguous && (local.is_none() || m.error <= local.unwrap().error + 0.5) {
                if let Some(predicted) = &predicted {
                    let previous = self.position(predicted);
                    let drift = (previous.0 - position.0)
                        .abs()
                        .max((previous.1 - position.1).abs());
                    if predicted.anchor != anchor
                        && (2..=64).contains(&drift)
                        && !self.edges.iter().any(|edge| {
                            edge.from == predicted.anchor && edge.to == anchor && edge.weight > 1.
                        })
                    {
                        self.edges.push(Edge {
                            from: predicted.anchor,
                            to: anchor,
                            delta: (
                                (predicted.offset.0 - m.x) as f64,
                                (predicted.offset.1 - m.y) as f64,
                            ),
                            weight: 4.,
                        });
                        self.optimize();
                    } else if drift > 64 && local.is_some() {
                        return Ok(predicted.clone());
                    }
                }
                return Ok(located);
            }
        }
        predicted.ok_or_else(|| MATCH_ERROR.into())
    }

    fn retain(&mut self, located: &Located) -> Result<(), String> {
        self.canvas.take();
        let mut covering = self.coverage().regions;
        // The pending frame is replaceable only when its entire footprint is covered
        // by archived frames plus the new view. Turning back must preserve its far edge.
        if self.pending.is_some() {
            covering.pop();
        }
        if let Some(previous) = self.pending.clone() {
            let mut with_new = covering.clone();
            with_new.push(self.rect(located));
            if uncovered(self.rect(&previous), &with_new) {
                self.archive_frame(&previous)?;
                covering.push(self.rect(&previous));
            }
        }
        self.pending = if uncovered(self.rect(located), &covering) {
            Some(located.clone())
        } else {
            None
        };
        Ok(())
    }

    pub fn render(&self) -> Result<Image, String> {
        let coverage = self.coverage();
        let bounds = coverage.bounds;
        let mut image = Image {
            width: bounds.width,
            height: bounds.height,
            rgba: allocation(bounds.width, bounds.height)?,
        };
        let mut place = |source: &Image, x: i32, y: i32| {
            let ox = (x - bounds.x) as usize;
            let oy = (y - bounds.y) as usize;
            for row in 0..source.height as usize {
                for col in 0..source.width as usize {
                    let dst = ((oy + row) * image.width as usize + ox + col) * 4;
                    let src = (row * source.width as usize + col) * 4;
                    if image.rgba[dst + 3] == 0 {
                        image.rgba[dst..dst + 4].copy_from_slice(&source.rgba[src..src + 4]);
                    }
                }
            }
        };
        for keyframe in &self.keyframes {
            let frame = decode_frame(&keyframe.path)?;
            place(
                &frame.image,
                keyframe.position.0.round() as i32,
                keyframe.position.1.round() as i32,
            );
        }
        if let Some(pending) = &self.pending {
            let p = self.position(pending);
            place(&pending.frame.image, p.0, p.1);
        }
        Ok(image)
    }

    pub fn finalize(&mut self) -> Result<Image, String> {
        self.optimize();
        self.render()
    }

    fn recover_views(&mut self) -> Result<(), String> {
        // Bound recovery work per frame so live tracking can continue between attempts.
        let mut unresolved = VecDeque::new();
        for _ in 0..self.unplaced.len().min(2) {
            let frame = self.unplaced.pop_back().unwrap();
            if let Ok(m) = translation(
                &self.reference.frame.levels,
                &frame.levels,
                &mut self.planner,
            ) {
                let located = Located {
                    frame,
                    anchor: self.reference.anchor,
                    offset: (self.reference.offset.0 + m.x, self.reference.offset.1 + m.y),
                };
                self.retain(&located)?;
            } else {
                unresolved.push_front(frame);
            }
        }
        unresolved.append(&mut self.unplaced);
        self.unplaced = unresolved;
        Ok(())
    }

    pub fn push(&mut self, image: Image) -> Result<bool, String> {
        image.validate()?;
        if image.width != self.reference.frame.image.width
            || image.height != self.reference.frame.image.height
        {
            return Err("采集区域尺寸已改变，请结束本次截图后重新开始。".into());
        }
        if image.rgba == self.reference.frame.image.rgba {
            self.misses = 0;
            self.recover_views()?;
            return Ok(false);
        }
        self.attempts += 1;
        let frame = Arc::new(Frame {
            levels: pyramid(&image),
            image,
        });
        let located = match self.locate(frame.clone()) {
            Ok(located) => located,
            Err(error) if is_alignment_error(&error) => {
                self.misses += 1;
                if self.misses == 1 || self.misses % 8 == 0 {
                    self.unplaced.push_back(frame);
                    let limit = (48 * 1024 * 1024
                        / (self.reference.frame.image.rgba.len().max(1) * 4 / 3))
                        .clamp(1, 6);
                    while self.unplaced.len() > limit {
                        self.unplaced.pop_front();
                    }
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let previous = self.position(&self.reference);
        let position = self.position(&located);
        if previous == position {
            self.reference = located;
            self.misses = 0;
            self.recover_views()?;
            return Ok(false);
        }
        let bounds = self.coverage().bounds;
        let rect = self.rect(&located);
        let left = bounds.x.min(rect.x);
        let top = bounds.y.min(rect.y);
        let right = (bounds.x + bounds.width as i32).max(rect.x + rect.width as i32);
        let bottom = (bounds.y + bounds.height as i32).max(rect.y + rect.height as i32);
        check_dimensions((right - left) as u32, (bottom - top) as u32)?;
        self.retain(&located)?;
        self.reference = located;
        self.misses = 0;
        self.frames += 1;
        // Revisit buffered views after recovering a known location. Unresolved views
        // remain visible as a count; they are never pasted at a guessed position.
        self.recover_views()?;
        Ok(true)
    }
}
