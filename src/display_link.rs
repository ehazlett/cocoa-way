//! Opt-in host display synchronization for compositor redraw experiments.

use std::ffi::OsStr;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event_loop::EventLoopProxy;

const FRAME_PACING_ENV: &str = "COCOA_WAY_FRAME_PACING";
static DISPLAY_TICK: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct CVDisplayLink(c_void);

type CVDisplayLinkRef = *mut CVDisplayLink;
type CVReturn = i32;
type CVOptionFlags = u64;

#[repr(C)]
struct CVTimeStamp {
    _version: u32,
    _video_time_scale: i32,
    _video_time: i64,
    _host_time: u64,
    _rate_scalar: f64,
    _video_refresh_period: i64,
    _smpte_time: [u8; 24],
    _flags: u64,
    _reserved: u64,
}

type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    CVDisplayLinkRef,
    *const CVTimeStamp,
    *const CVTimeStamp,
    CVOptionFlags,
    *mut CVOptionFlags,
    *mut c_void,
) -> CVReturn;

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVDisplayLinkCreateWithCGDisplay(
        display_id: u32,
        display_link: *mut CVDisplayLinkRef,
    ) -> CVReturn;
    fn CVDisplayLinkSetOutputCallback(
        display_link: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn;
    fn CVDisplayLinkStart(display_link: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkStop(display_link: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkRelease(display_link: CVDisplayLinkRef);
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
}

pub fn enabled() -> bool {
    enabled_value(std::env::var_os(FRAME_PACING_ENV).as_deref())
}

fn enabled_value(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("display-link"))
}

pub fn take_tick() -> bool {
    DISPLAY_TICK.swap(false, Ordering::AcqRel)
}

pub struct DisplayLink {
    raw: NonNull<CVDisplayLink>,
    proxy: *mut EventLoopProxy<()>,
}

impl DisplayLink {
    pub fn start(proxy: EventLoopProxy<()>) -> Result<Self, String> {
        let proxy = Box::into_raw(Box::new(proxy));
        let mut raw = std::ptr::null_mut();
        let display_id = unsafe { CGMainDisplayID() };
        let create_status = unsafe { CVDisplayLinkCreateWithCGDisplay(display_id, &mut raw) };
        let Some(raw) = NonNull::new(raw) else {
            unsafe { drop(Box::from_raw(proxy)) };
            return Err(format!(
                "CVDisplayLink creation failed with status {create_status}"
            ));
        };
        if create_status != 0 {
            unsafe {
                CVDisplayLinkRelease(raw.as_ptr());
                drop(Box::from_raw(proxy));
            }
            return Err(format!(
                "CVDisplayLink creation failed with status {create_status}"
            ));
        }
        let callback_status = unsafe {
            CVDisplayLinkSetOutputCallback(raw.as_ptr(), display_link_callback, proxy.cast())
        };
        if callback_status != 0 {
            unsafe {
                CVDisplayLinkRelease(raw.as_ptr());
                drop(Box::from_raw(proxy));
            }
            return Err(format!(
                "CVDisplayLink callback setup failed with status {callback_status}"
            ));
        }
        let start_status = unsafe { CVDisplayLinkStart(raw.as_ptr()) };
        if start_status != 0 {
            unsafe {
                CVDisplayLinkRelease(raw.as_ptr());
                drop(Box::from_raw(proxy));
            }
            return Err(format!(
                "CVDisplayLink start failed with status {start_status}"
            ));
        }
        Ok(Self { raw, proxy })
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        unsafe {
            CVDisplayLinkStop(self.raw.as_ptr());
            CVDisplayLinkRelease(self.raw.as_ptr());
            drop(Box::from_raw(self.proxy));
        }
    }
}

unsafe extern "C" fn display_link_callback(
    _display_link: CVDisplayLinkRef,
    _now: *const CVTimeStamp,
    _output_time: *const CVTimeStamp,
    _flags_in: CVOptionFlags,
    _flags_out: *mut CVOptionFlags,
    user_info: *mut c_void,
) -> CVReturn {
    DISPLAY_TICK.store(true, Ordering::Release);
    let proxy = unsafe { &*user_info.cast::<EventLoopProxy<()>>() };
    let _ = proxy.send_event(());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_link_pacing_requires_the_explicit_experimental_value() {
        assert!(enabled_value(Some(OsStr::new("display-link"))));
        assert!(!enabled_value(None));
        assert!(!enabled_value(Some(OsStr::new("fixed"))));
        assert!(!enabled_value(Some(OsStr::new("1"))));
    }
}
