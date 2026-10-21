//! Windows H.264 / MP4 encoding on the recording worker's COM apartment.
use crate::video::FRAME_RATE;
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    core::{w, Interface, HRESULT, PCWSTR},
    Win32::{Media::MediaFoundation::*, System::Com::*},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
};

struct Library(HMODULE);
impl Library {
    fn load(name: PCWSTR) -> windows::core::Result<Self> {
        let module = unsafe {
            LoadLibraryExW(
                name.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self(module))
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

type StartupFn = unsafe extern "system" fn(u32, u32) -> HRESULT;
type ShutdownFn = unsafe extern "system" fn() -> HRESULT;
type AttributesFn = unsafe extern "system" fn(*mut *mut c_void, u32) -> HRESULT;
type CreateFn = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type BufferFn = unsafe extern "system" fn(u32, *mut *mut c_void) -> HRESULT;
type WriterFn =
    unsafe extern "system" fn(PCWSTR, *mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT;

struct MediaApi {
    startup: StartupFn,
    shutdown: ShutdownFn,
    attributes: AttributesFn,
    media_type: CreateFn,
    buffer: BufferFn,
    sample: CreateFn,
    writer: WriterFn,
    _libraries: [Library; 2],
}

impl MediaApi {
    fn load() -> windows::core::Result<Self> {
        // Optional media DLLs are loaded here, never through the executable's import
        // table: Windows N / stripped systems must still be able to use screenshots.
        let platform = Library::load(w!("mfplat.dll"))?;
        let readwrite = Library::load(w!("mfreadwrite.dll"))?;
        macro_rules! function {
            ($library:expr, $name:literal, $signature:ty) => {{
                let address = unsafe { GetProcAddress($library.0, concat!($name, "\0").as_ptr()) }
                    .ok_or_else(windows::core::Error::from_thread)?;
                // SAFETY: each documented export is cast to its exact Win32 ABI signature.
                unsafe {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $signature>(address)
                }
            }};
        }
        Ok(Self {
            startup: function!(platform, "MFStartup", StartupFn),
            shutdown: function!(platform, "MFShutdown", ShutdownFn),
            attributes: function!(platform, "MFCreateAttributes", AttributesFn),
            media_type: function!(platform, "MFCreateMediaType", CreateFn),
            buffer: function!(platform, "MFCreateMemoryBuffer", BufferFn),
            sample: function!(platform, "MFCreateSample", CreateFn),
            writer: function!(readwrite, "MFCreateSinkWriterFromURL", WriterFn),
            _libraries: [platform, readwrite],
        })
    }
}

unsafe fn interface<T: Interface>(
    create: impl FnOnce(*mut *mut c_void) -> HRESULT,
) -> windows::core::Result<T> {
    let mut pointer = std::ptr::null_mut();
    create(&mut pointer).ok()?;
    if pointer.is_null() {
        return Err(windows::core::Error::from_hresult(HRESULT(
            0x80004003u32 as i32,
        )));
    }
    // SAFETY: a successful factory transfers ownership of this non-null interface.
    Ok(unsafe { T::from_raw(pointer) })
}

struct MediaPlatform {
    api: MediaApi,
}

impl MediaPlatform {
    fn new() -> windows::core::Result<Self> {
        let api = MediaApi::load()?;
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            if let Err(error) = (api.startup)(MF_VERSION, MFSTARTUP_FULL).ok() {
                CoUninitialize();
                return Err(error);
            }
        }
        Ok(Self { api })
    }
}

impl Drop for MediaPlatform {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.api.shutdown)();
            CoUninitialize();
        }
    }
}

pub(super) struct VideoEncoder {
    // COM objects must be released before Media Foundation and COM shut down.
    writer: IMFSinkWriter,
    _platform: MediaPlatform,
    stream: u32,
    frame_bytes: usize,
}

impl VideoEncoder {
    pub(super) fn new(
        path: &Path,
        width: u32,
        height: u32,
        hardware: bool,
    ) -> Result<Self, String> {
        let platform = MediaPlatform::new().map_err(|error| format!("无法初始化系统视频编码组件：{error}。Windows N 版本可能需要安装 Media Feature Pack。"))?;
        let initialize = || -> windows::core::Result<(IMFSinkWriter, u32)> {
            unsafe {
                let api = &platform.api;
                let attributes: IMFAttributes = interface(|pointer| (api.attributes)(pointer, 2))?;
                attributes.SetUINT32(
                    &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
                    u32::from(hardware),
                )?;
                // Keep the sink's backpressure when an encoder or disk is slower
                // than capture. The worker supplies real, pause-adjusted timestamps.
                attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 0)?;
                let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                let writer: IMFSinkWriter = interface(|pointer| {
                    (api.writer)(
                        PCWSTR(filename.as_ptr()),
                        std::ptr::null_mut(),
                        attributes.as_raw(),
                        pointer,
                    )
                })?;
                let output: IMFMediaType = interface(|pointer| (api.media_type)(pointer))?;
                output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
                output.SetUINT32(
                    &MF_MT_AVG_BITRATE,
                    (width * height * FRAME_RATE / 8).clamp(2_000_000, 20_000_000),
                )?;
                output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
                output.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
                output.SetUINT64(&MF_MT_FRAME_RATE, ((FRAME_RATE as u64) << 32) | 1)?;
                output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
                output.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT601.0 as u32)?;
                output.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
                let stream = writer.AddStream(&output)?;
                let input: IMFMediaType = interface(|pointer| (api.media_type)(pointer))?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
                input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                input.SetUINT32(&MF_MT_DEFAULT_STRIDE, width)?;
                input.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
                input.SetUINT64(&MF_MT_FRAME_RATE, ((FRAME_RATE as u64) << 32) | 1)?;
                input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
                input.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT601.0 as u32)?;
                input.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
                writer.SetInputMediaType(stream, &input, None)?;
                writer.BeginWriting()?;
                Ok((writer, stream))
            }
        };
        let (writer, stream) =
            initialize().map_err(|error| format!("无法创建 H.264 / MP4 编码器：{error}"))?;
        Ok(Self {
            writer,
            _platform: platform,
            stream,
            frame_bytes: width as usize * height as usize * 3 / 2,
        })
    }

    pub(super) fn write(&self, nv12: &[u8], timestamp: i64, duration: i64) -> Result<(), String> {
        if nv12.len() != self.frame_bytes || timestamp < 0 || duration <= 0 {
            return Err("录屏帧尺寸或时间戳无效。".into());
        }
        let write = || -> windows::core::Result<()> {
            unsafe {
                let api = &self._platform.api;
                let buffer: IMFMediaBuffer =
                    interface(|pointer| (api.buffer)(nv12.len() as u32, pointer))?;
                let mut data = std::ptr::null_mut();
                buffer.Lock(&mut data, None, None)?;
                std::ptr::copy_nonoverlapping(nv12.as_ptr(), data, nv12.len());
                buffer.Unlock()?;
                buffer.SetCurrentLength(nv12.len() as u32)?;
                let sample: IMFSample = interface(|pointer| (api.sample)(pointer))?;
                sample.AddBuffer(&buffer)?;
                sample.SetSampleTime(timestamp)?;
                sample.SetSampleDuration(duration)?;
                self.writer.WriteSample(self.stream, &sample)
            }
        };
        write().map_err(|error| format!("写入录屏帧失败：{error}"))
    }

    pub(super) fn finish(self) -> Result<(), String> {
        unsafe { self.writer.Finalize() }.map_err(|error| format!("无法完成 MP4 文件：{error}"))
    }
}
