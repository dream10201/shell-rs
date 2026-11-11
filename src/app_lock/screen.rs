//! Whether the computer's own lock screen is showing, for 电脑锁屏时锁定.
use std::sync::Arc;

/// Asked from the UI thread once a second: must not block.
pub trait ScreenState: Send + Sync + 'static {
    fn is_locked(&self) -> bool;
}

/// A computer whose screen never locks: tests, and systems that cannot say.
pub struct NoScreen;

impl ScreenState for NoScreen {
    fn is_locked(&self) -> bool {
        false
    }
}

/// The system's lock screen: the login session on macOS and Windows,
/// logind's `LockedHint` on Linux (GNOME and KDE set it).
pub fn system_screen() -> Arc<dyn ScreenState> {
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    return Arc::new(system::Screen::new());
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    Arc::new(NoScreen)
}

#[cfg(target_os = "macos")]
mod system {
    use std::ffi::{c_char, c_void};

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFBooleanFalse: *const c_void;
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            string: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
        fn CFRelease(object: *const c_void);
    }

    const UTF8: u32 = 0x0800_0100;

    pub struct Screen;

    impl Screen {
        pub fn new() -> Self {
            Self
        }
    }

    impl super::ScreenState for Screen {
        fn is_locked(&self) -> bool {
            // SAFETY: plain Core Foundation calls; both objects made here are
            // released, and the value belongs to the dictionary.
            unsafe {
                let session = CGSessionCopyCurrentDictionary();
                if session.is_null() {
                    return false;
                }
                let key = CFStringCreateWithCString(
                    std::ptr::null(),
                    c"CGSSessionScreenIsLocked".as_ptr(),
                    UTF8,
                );
                let locked = !key.is_null() && {
                    let value = CFDictionaryGetValue(session, key);
                    CFRelease(key);
                    !value.is_null() && value != kCFBooleanFalse
                };
                CFRelease(session);
                locked
            }
        }
    }
}

#[cfg(windows)]
mod system {
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory,
        WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
    };

    pub struct Screen;

    impl Screen {
        pub fn new() -> Self {
            Self
        }
    }

    impl super::ScreenState for Screen {
        fn is_locked(&self) -> bool {
            let mut buffer = std::ptr::null_mut();
            let mut len = 0;
            // SAFETY: the system allocates `buffer` and says how long it is;
            // it is read within that length and freed.
            unsafe {
                if WTSQuerySessionInformationW(
                    WTS_CURRENT_SERVER_HANDLE,
                    WTS_CURRENT_SESSION,
                    WTSSessionInfoEx,
                    &mut buffer,
                    &mut len,
                ) == 0
                    || buffer.is_null()
                {
                    return false;
                }
                let locked = len as usize >= size_of::<WTSINFOEXW>() && {
                    let info = buffer.cast::<WTSINFOEXW>().read_unaligned();
                    info.Level == 1
                        && info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK as i32
                };
                WTSFreeMemory(buffer.cast());
                locked
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod system {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    pub struct Screen(Arc<AtomicBool>);

    impl Screen {
        /// logind says when the hint changes; a thread waits for that.
        pub fn new() -> Self {
            let locked = Arc::new(AtomicBool::new(false));
            let flag = locked.clone();
            let _ = std::thread::Builder::new()
                .name("screen-lock".into())
                .spawn(move || {
                    let _ = watch(&flag);
                });
            Self(locked)
        }
    }

    impl super::ScreenState for Screen {
        fn is_locked(&self) -> bool {
            self.0.load(Ordering::Relaxed)
        }
    }

    fn watch(locked: &AtomicBool) -> zbus::Result<()> {
        let connection = zbus::blocking::Connection::system()?;
        let session = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )?;
        locked.store(session.get_property("LockedHint")?, Ordering::Relaxed);
        for change in session.receive_property_changed::<bool>("LockedHint") {
            if let Ok(hint) = change.get() {
                locked.store(hint, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}
