use freecut_lib::{
    imaging::{self, Image},
    stitching::{Region, Stitcher},
};

fn frame(x: i32, y: i32, width: u32, height: u32) -> Image {
    let mut rgba = Vec::new();
    for row in 0..height as i32 {
        for col in 0..width as i32 {
            let (x, y) = (x + col, y + row);
            let mut n = (x.div_euclid(11) as u32).wrapping_mul(0x45d9f3b)
                ^ (y.div_euclid(13) as u32).wrapping_mul(0x119de1f3);
            n = (n ^ (n >> 16)).wrapping_mul(0x45d9f3b);
            let detail = ((x * 17 + y * 29).rem_euclid(29)) as u8;
            rgba.extend_from_slice(&[
                (n as u8 / 2) + detail,
                ((n >> 8) as u8 / 2) + detail,
                ((n >> 16) as u8 / 2) + detail,
                255,
            ]);
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

#[test]
fn stitches_all_directions_and_diagonal_without_changing_pixels() {
    for (dx, dy) in [
        (45, 0),
        (-39, 0),
        (0, 53),
        (0, -41),
        (43, 37),
        (-33, -29),
        (1, -1),
    ] {
        let mut stitcher = Stitcher::new(frame(100, 100, 180, 160)).unwrap();
        assert!(
            stitcher.push(frame(100 + dx, 100 + dy, 180, 160)).unwrap(),
            "{dx}, {dy}"
        );
        let result = stitcher.image();
        assert_eq!(
            (result.width, result.height),
            (180 + dx.unsigned_abs(), 160 + dy.unsigned_abs())
        );
        let expected = frame(
            100 + dx.min(0),
            100 + dy.min(0),
            result.width,
            result.height,
        );
        for (actual, expected) in result
            .rgba
            .chunks_exact(4)
            .zip(expected.rgba.chunks_exact(4))
        {
            if actual[3] != 0 {
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(stitcher.frames(), 2);
    }
}

#[test]
fn revisiting_and_changing_direction_does_not_duplicate_content() {
    let mut stitcher = Stitcher::new(frame(100, 100, 180, 160)).unwrap();
    for (x, y) in [(145, 100), (145, 145), (100, 145), (100, 100), (65, 100)] {
        stitcher.push(frame(x, y, 180, 160)).unwrap();
        let view = stitcher.coverage().viewport;
        assert_eq!(
            (view.x, view.y),
            (x - 100, y - 100),
            "viewport drift after moving to ({x}, {y})"
        );
    }
    let image = stitcher.into_image();
    assert_eq!((image.width, image.height), (260, 205));
    // The lower-left corner was never visited.
    assert_eq!(
        &image.rgba[((204 * 260) * 4)..((204 * 260) * 4 + 4)],
        &[0, 0, 0, 0]
    );
    let expected = frame(65, 100, 260, 205);
    for (index, (actual, expected)) in image
        .rgba
        .chunks_exact(4)
        .zip(expected.rgba.chunks_exact(4))
        .enumerate()
    {
        if actual[3] != 0 {
            assert_eq!(actual, expected, "misaligned output pixel at index {index}");
        }
    }
}

#[test]
fn stationary_and_small_dynamic_changes_do_not_grow_canvas() {
    let initial = frame(0, 0, 180, 160);
    let mut stitcher = Stitcher::new(initial.clone()).unwrap();
    assert!(!stitcher.push(initial.clone()).unwrap());
    let mut dynamic = initial.clone();
    for y in 40..50 {
        for x in 40..50 {
            dynamic.rgba[(y * 180 + x) * 4..(y * 180 + x) * 4 + 3].fill(255);
        }
    }
    assert!(!stitcher.push(dynamic).unwrap());
    assert_eq!(stitcher.image().rgba, initial.rgba);
    assert_eq!(stitcher.frames(), 1);
}

#[test]
fn insufficient_overlap_is_rejected_and_reference_is_retained() {
    let mut stitcher = Stitcher::new(frame(0, 0, 180, 160)).unwrap();
    let original = stitcher.image().rgba.clone();
    assert!(stitcher.push(frame(500, 700, 180, 160)).is_err());
    assert_eq!(stitcher.image().rgba, original);
    assert_eq!(stitcher.frames(), 1);
    assert!(stitcher.push(frame(31, 29, 180, 160)).unwrap());
}

#[test]
fn repeated_terrain_and_featureless_images_are_rejected() {
    let solid = Image {
        width: 160,
        height: 120,
        rgba: vec![128; 160 * 120 * 4],
    };
    let mut stitcher = Stitcher::new(solid.clone()).unwrap();
    let mut other = solid;
    other.rgba[0] = 127;
    assert!(stitcher.push(other).is_err());
    let repeated = |offset: u32| {
        let mut image = Image {
            width: 160,
            height: 120,
            rgba: Vec::new(),
        };
        for y in 0..120 {
            for x in 0..160 {
                let value = (((x + offset) % 16 * 11 + y % 16 * 7) % 256) as u8;
                image.rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        image
    };
    let mut stitcher = Stitcher::new(repeated(0)).unwrap();
    assert!(stitcher.push(repeated(5)).is_err());
}

#[test]
fn dimensions_and_invalid_input_are_checked() {
    assert!(Stitcher::new(frame(0, 0, 47, 100)).is_err());
    assert!(Stitcher::new(Image {
        width: 100,
        height: 100,
        rgba: vec![]
    })
    .is_err());
    let mut stitcher = Stitcher::new(frame(0, 0, 100, 100)).unwrap();
    assert!(stitcher.push(frame(0, 0, 101, 100)).is_err());
    assert!(stitcher
        .push(Image {
            width: 100,
            height: 100,
            rgba: vec![]
        })
        .is_err());
}

#[test]
fn stitched_image_supports_transparent_polygon_crop_and_png() {
    let mut stitcher = Stitcher::new(frame(0, 0, 180, 160)).unwrap();
    stitcher.push(frame(45, 0, 180, 160)).unwrap();
    let cropped = imaging::crop(
        stitcher.image(),
        &[[0., 0.], [225., 0.], [0., 160.]],
        225.,
        160.,
    )
    .unwrap();
    assert_eq!(cropped.rgba[3], 255);
    assert_eq!(*cropped.rgba.last().unwrap(), 0);
    assert!(cropped.png().unwrap().starts_with(b"\x89PNG"));
}

#[test]
fn physical_region_mapping_rounds_outward_and_extracts_exact_pixels() {
    let image = frame(0, 0, 300, 200);
    let region = Region::from_selection(
        &image,
        &[[10.2, 20.2], [80.2, 20.2], [80.2, 70.2], [10.2, 70.2]],
        150.,
        100.,
    )
    .unwrap();
    assert_eq!(
        region,
        Region {
            x: 20,
            y: 40,
            width: 141,
            height: 101
        }
    );
    let extracted = region.extract(&image).unwrap();
    assert_eq!(extracted.rgba, frame(20, 40, 141, 101).rgba);
    for points in [
        vec![],
        vec![[0., 0.]; 4],
        vec![[0., 0.], [70., 0.], [60., 60.], [0., 60.]],
        vec![[-1., 0.], [70., 0.], [70., 60.], [-1., 60.]],
        vec![[0., 0.], [f64::NAN, 0.], [70., 60.], [0., 60.]],
    ] {
        assert!(Region::from_selection(&image, &points, 150., 100.).is_err());
    }
    assert!(Region::from_selection(
        &image,
        &[[0., 0.], [70., 0.], [70., 60.], [0., 60.]],
        f64::INFINITY,
        100.
    )
    .is_err());
    assert!(Region {
        x: 290,
        y: 190,
        width: 20,
        height: 20
    }
    .extract(&image)
    .is_err());
    assert!(Region {
        x: 0,
        y: 0,
        width: 0,
        height: 100
    }
    .extract(&image)
    .is_err());
}

#[test]
fn full_hd_odd_pixel_motion_keeps_exact_alignment() {
    let mut stitcher = Stitcher::new(frame(100, 100, 1920, 1080)).unwrap();
    let start = std::time::Instant::now();
    stitcher.push(frame(237, 189, 1920, 1080)).unwrap();
    assert_eq!(
        (stitcher.image().width, stitcher.image().height),
        (2057, 1169)
    );
    eprintln!("Full HD alignment: {:?}", start.elapsed());
}
