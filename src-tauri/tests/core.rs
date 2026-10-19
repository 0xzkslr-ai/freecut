use freecut_lib::{
    imaging::{self, Image},
    settings::{self, Mode, Settings},
};
use std::io::Cursor;

fn image(w: u32, h: u32) -> Image {
    let mut rgba = vec![0; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            rgba[i..i + 4].copy_from_slice(&[x as u8, y as u8, 123, 255]);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}
fn pixel(i: &Image, x: u32, y: u32) -> &[u8] {
    let p = (y * i.width + x) as usize * 4;
    &i.rgba[p..p + 4]
}

#[test]
fn source_pixel_density_and_tight_bounds() {
    let source = image(200, 120);
    let result = imaging::crop(
        &source,
        &[[10., 10.], [80., 10.], [80., 50.], [10., 50.]],
        100.,
        60.,
    )
    .unwrap();
    assert_eq!((result.width, result.height), (140, 80));
    assert_eq!(pixel(&result, 20, 20), pixel(&source, 40, 40));
}
#[test]
fn triangle_has_transparency_and_antialiasing_without_color_changes() {
    let source = image(100, 100);
    let result = imaging::crop(&source, &[[0., 0.], [100., 0.], [0., 100.]], 100., 100.).unwrap();
    assert_eq!(pixel(&result, 10, 10), pixel(&source, 10, 10));
    assert_eq!(pixel(&result, 90, 90), &[0, 0, 0, 0]);
    assert!(result.rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
    for y in 0..100 {
        for x in 0..100 {
            let p = pixel(&result, x, y);
            if p[3] > 0 {
                assert_eq!(&p[..3], &pixel(&source, x, y)[..3]);
            }
        }
    }
}
#[test]
fn concave_polygon_and_even_odd_self_intersection() {
    let source = image(100, 100);
    let result = imaging::crop(
        &source,
        &[
            [0., 0.],
            [100., 0.],
            [100., 30.],
            [30., 30.],
            [30., 100.],
            [0., 100.],
        ],
        100.,
        100.,
    )
    .unwrap();
    assert_eq!(pixel(&result, 10, 80)[3], 255);
    assert_eq!(pixel(&result, 80, 80)[3], 0);
    let result = imaging::crop(
        &source,
        &[[0., 0.], [100., 100.], [100., 0.], [0., 100.]],
        100.,
        100.,
    )
    .unwrap();
    assert_eq!(pixel(&result, 10, 50)[3], 255);
    assert_eq!(pixel(&result, 50, 10)[3], 0);
}
#[test]
fn clips_to_edges_and_rounds_fractional_bounds_outward() {
    let source = image(100, 100);
    let result = imaging::crop(
        &source,
        &[[-10., 10.2], [50.4, 10.2], [50.4, 80.3], [-10., 80.3]],
        100.,
        100.,
    )
    .unwrap();
    assert_eq!((result.width, result.height), (51, 71));
}
#[test]
fn invalid_inputs_are_rejected() {
    let source = image(100, 100);
    for points in [
        vec![],
        vec![[0., 0.], [1., 1.], [2., 2.]],
        vec![[f64::NAN, 0.], [4., 0.], [0., 4.]],
        vec![[150., 0.], [170., 0.], [150., 20.]],
        vec![[0., 0.], [1., 0.], [0., 1.]],
    ] {
        assert!(imaging::crop(&source, &points, 100., 100.).is_err());
    }
    let triangle = [[0., 0.], [30., 0.], [0., 30.]];
    for (w, h) in [
        (0., 100.),
        (-1., 100.),
        (100., f64::NAN),
        (f64::INFINITY, 100.),
    ] {
        assert!(imaging::crop(&source, &triangle, w, h).is_err());
    }
    assert!(!imaging::has_area(&[[0., 0.], [0., 0.], [0., 0.]]));
    assert!(imaging::crop(&source, &vec![[0., 0.]; 100_001], 100., 100.).is_err());
    let bad = Image {
        width: 100,
        height: 100,
        rgba: vec![],
    };
    assert!(bad.validate().is_err());
    assert!(bad.png().is_err());
    assert!(imaging::crop(&bad, &triangle, 100., 100.).is_err());
    assert!(Image {
        width: 0,
        height: 0,
        rgba: vec![]
    }
    .validate()
    .is_err());
    let transparent = Image {
        width: 10,
        height: 10,
        rgba: vec![0; 400],
    };
    assert!(imaging::crop(&transparent, &triangle, 100., 100.).is_err());
}
#[test]
fn alpha_multiplies_source_and_png_roundtrips() {
    let mut source = image(20, 20);
    for p in source.rgba.chunks_exact_mut(4) {
        p[3] = 128;
    }
    let result = imaging::crop(&source, &[[0., 0.], [20., 0.], [0., 20.]], 20., 20.).unwrap();
    assert_eq!(pixel(&result, 2, 2)[3], 128);
    let bytes = result.png().unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(&pixels[..info.buffer_size()], &result.rgba);
}
#[test]
fn saves_with_extension_and_atomic_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let source = image(20, 20);
    let path = dir.path().join("截图");
    let saved = imaging::save_png(&path, &source).unwrap();
    assert_eq!(saved, dir.path().join("截图.png"));
    assert!(std::fs::read(&saved).unwrap().starts_with(b"\x89PNG"));
    imaging::atomic_write(&saved, b"old").unwrap();
    imaging::save_png(&saved, &source).unwrap();
    assert!(std::fs::read(&saved).unwrap().starts_with(b"\x89PNG"));
    assert!(imaging::save_png(&dir.path().join("missing/file.png"), &source).is_err());
    assert!(imaging::atomic_write(dir.path(), b"fail").is_err());
    assert!(dir.path().is_dir());
    let upper = dir.path().join("upper.PNG");
    assert_eq!(imaging::save_png(&upper, &source).unwrap(), upper);
}
#[test]
fn invalid_image_never_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keep.png");
    std::fs::write(&path, b"keep").unwrap();
    assert!(imaging::save_png(
        &path,
        &Image {
            width: 0,
            height: 0,
            rgba: vec![]
        }
    )
    .is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
}
#[test]
fn shortcut_validation_and_normalization() {
    for (input, expected) in [
        ("alt+ctrl+a", "Ctrl+Alt+A"),
        (" shift + alt + f24 ", "Alt+Shift+F24"),
        ("Ctrl+1", "Ctrl+1"),
        ("Ctrl+F11", "Ctrl+F11"),
    ] {
        assert_eq!(settings::normalize_hotkey(input).unwrap(), expected);
    }
    for input in [
        "",
        "A",
        "Shift+A",
        "Ctrl+F12",
        "Ctrl+Space",
        "Ctrl+F25",
        "Ctrl+F0",
        "Ctrl+F01",
        "Ctrl+Ctrl+A",
        "Ctrl+Alt+Shift+Shift+A",
        "Ctrl+A+B",
        "Meta+A",
        "Ctrl+",
        "Ctrl+é",
        "Ctrl+F-1",
    ] {
        assert!(settings::normalize_hotkey(input).is_err(), "{input}");
    }
}
#[test]
fn settings_roundtrip_and_invalid_file_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config/settings.json");
    assert_eq!(settings::load(&path).hotkey, settings::DEFAULT_HOTKEY);
    let set = Settings {
        mode: Mode::Polygon,
        hotkey: "Alt+Shift+F24".into(),
    };
    settings::save(&path, &set).unwrap();
    let loaded = settings::load(&path);
    assert_eq!(loaded.mode, Mode::Polygon);
    assert_eq!(loaded.hotkey, set.hotkey);
    std::fs::write(&path, b"broken").unwrap();
    assert_eq!(settings::load(&path).mode, Mode::Lasso);
    std::fs::write(&path, r#"{"hotkey":"Ctrl+F12"}"#).unwrap();
    assert_eq!(settings::load(&path).hotkey, settings::DEFAULT_HOTKEY);
    assert!(settings::save(&dir.path().join("config/settings.json/file"), &set).is_err());
}

#[cfg(windows)]
#[test]
fn native_dib_preserves_alpha_and_channel_order() {
    let source = Image {
        width: 2,
        height: 1,
        rgba: vec![12, 34, 56, 78, 0, 0, 0, 0],
    };
    let bytes = freecut_lib::platform::dib_bytes(&source).unwrap();
    assert_eq!(&bytes[..4], &124u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &(-1i32).to_le_bytes());
    assert_eq!(&bytes[124..], &[56, 34, 12, 78, 0, 0, 0, 0]);
}
