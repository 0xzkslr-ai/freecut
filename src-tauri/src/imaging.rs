use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tiny_skia::{FillRule, Mask, PathBuilder, Transform};

pub type Point = [f64; 2];

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub id: u64,
    pub width: u32,
    pub height: u32,
}

impl Image {
    pub fn validate(&self) -> Result<(), String> {
        let length = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4));
        if self.width == 0 || self.height == 0 || length != Some(self.rgba.len()) {
            return Err("图像尺寸或像素数据无效。".into());
        }
        Ok(())
    }

    pub fn png(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder.write_header().map_err(|_| "PNG 编码失败。")?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|_| "PNG 编码失败。")?;
        }
        Ok(bytes)
    }
}

pub fn has_area(points: &[Point]) -> bool {
    if points.len() < 3 || !points.iter().flatten().all(|p| p.is_finite()) {
        return false;
    }
    let [x0, y0] = points[0];
    let Some([x1, y1]) = points.iter().copied().find(|p| p != &points[0]) else {
        return false;
    };
    points
        .iter()
        .any(|[x, y]| ((x1 - x0) * (y - y0) - (y1 - y0) * (x - x0)).abs() > 0.01)
}

pub fn crop(image: &Image, points: &[Point], width: f64, height: f64) -> Result<Image, String> {
    image.validate()?;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("截图尺寸无效。".into());
    }
    if points.len() > 100_000 || !has_area(points) {
        return Err("请圈出有面积的选区，至少需要三个不共线的点。".into());
    }
    // Restrict the IPC input to the actual viewport; preserve the physical source pixels.
    let points: Vec<Point> = points
        .iter()
        .map(|[x, y]| {
            [
                x.clamp(0., width) * image.width as f64 / width,
                y.clamp(0., height) * image.height as f64 / height,
            ]
        })
        .collect();
    if !has_area(&points) {
        return Err("选区太小或位于图像之外，请重新圈选。".into());
    }
    let left = points
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min)
        .floor() as u32;
    let top = points
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        .floor() as u32;
    let right = (points.iter().map(|p| p[0]).fold(0., f64::max).ceil() as u32).min(image.width);
    let bottom = (points.iter().map(|p| p[1]).fold(0., f64::max).ceil() as u32).min(image.height);
    let (w, h) = (right.saturating_sub(left), bottom.saturating_sub(top));
    if w < 2 || h < 2 {
        return Err("选区太小或位于图像之外，请重新圈选。".into());
    }
    let mut path = PathBuilder::new();
    path.move_to(
        (points[0][0] - left as f64) as f32,
        (points[0][1] - top as f64) as f32,
    );
    for [x, y] in &points[1..] {
        path.line_to((x - left as f64) as f32, (y - top as f64) as f32);
    }
    path.close();
    let path = path.finish().ok_or("选区轮廓无效。")?;
    let mut mask = Mask::new(w, h).ok_or("选区尺寸过大，无法创建蒙版。")?;
    mask.fill_path(&path, FillRule::EvenOdd, true, Transform::identity());
    let mut result = Image {
        width: w,
        height: h,
        rgba: vec![0; w as usize * h as usize * 4],
    };
    for y in 0..h {
        for x in 0..w {
            let src = ((y + top) as usize * image.width as usize + (x + left) as usize) * 4;
            let dst = (y as usize * w as usize + x as usize) * 4;
            let alpha =
                ((image.rgba[src + 3] as u16 * mask.data()[dst / 4] as u16 + 127) / 255) as u8;
            if alpha != 0 {
                result.rgba[dst..dst + 3].copy_from_slice(&image.rgba[src..src + 3]);
            }
            result.rgba[dst + 3] = alpha;
        }
    }
    if !result.rgba.chunks_exact(4).any(|p| p[3] > 0) {
        return Err("选区没有可见像素，请重新圈选。".into());
    }
    Ok(result)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "无法保存文件，请检查目录和写入权限。")?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "文件写入未完成，请检查磁盘空间。")?;
    file.persist(path)
        .map_err(|_| "无法完成保存，请检查文件权限或是否被其他程序占用。")?;
    Ok(())
}

pub fn save_png(path: &Path, image: &Image) -> Result<PathBuf, String> {
    let path = if path
        .extension()
        .is_some_and(|p| p.eq_ignore_ascii_case("png"))
    {
        path.to_owned()
    } else {
        let mut name = path.as_os_str().to_os_string();
        name.push(".png");
        PathBuf::from(name)
    };
    atomic_write(&path, &image.png()?)?;
    Ok(path)
}
