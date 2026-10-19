pub mod imaging;
pub mod settings;
pub mod stitching;

#[cfg(all(windows, feature = "desktop"))]
pub mod desktop;
#[cfg(windows)]
pub mod platform;
