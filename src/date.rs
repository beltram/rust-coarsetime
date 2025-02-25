#[allow(unused_imports)]
use std::mem::MaybeUninit;
use std::ops::*;
#[allow(unused_imports)]
use std::ptr::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(
    any(target_arch = "wasm32", target_arch = "wasm64"),
    target_os = "unknown"
))]
use wasm_bindgen::prelude::*;

use super::duration::*;
#[allow(unused_imports)]
use super::helpers::*;

/// A representation of the current, actual date.
#[derive(Copy, Clone, Debug, Hash, Ord, Eq, PartialOrd, PartialEq)]
pub struct Date(u64);

static RECENT: AtomicU64 = AtomicU64::new(0);

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
#[allow(non_camel_case_types)]
type clockid_t = libc::c_int;

#[cfg(target_os = "macos")]
const CLOCK_REALTIME: clockid_t = 0;

#[cfg(target_os = "macos")]
extern "system" {
    fn clock_gettime_nsec_np(clk_id: clockid_t) -> u64;
}

#[cfg(target_os = "freebsd")]
const CLOCK_REALTIME_COARSE: clockid_t = 10;

impl Date {
    /// Returns a date corresponding to "now"
    ///
    /// This function also updates the stored date.
    pub fn now() -> Date {
        let now = Self::_now();
        Self::_update(now);
        Date(now)
    }

    /// Returns a date corresponding to "now" without updating the cached value.
    ///
    /// After this, `recent()` will still return the old date.
    /// `now()` is generally preferred over this function.
    pub fn now_without_cache_update() -> Date {
        let now = Self::_now();
        Date(now)
    }

    /// Returns a date corresponding to the latest update
    pub fn recent() -> Date {
        match Self::_recent() {
            0 => Date::now(),
            recent => Date(recent),
        }
    }

    /// Update the stored date
    ///
    /// This function should be called frequently, for example in an event loop
    /// or using an `Updater` task.
    pub fn update() {
        let now = Self::_now();
        Self::_update(now);
    }

    /// Returns the amount of time elapsed from another date to this one
    #[inline]
    pub fn duration_since(&self, earlier: Date) -> Duration {
        *self - earlier
    }

    /// Returns the amount of time elapsed between the this date was created
    /// and the latest update
    #[inline]
    pub fn elapsed_since_recent(&self) -> Duration {
        Self::recent() - *self
    }

    /// Returns the amount of time elapsed since this date was created
    ///
    /// This function also updates the stored date.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// Return a representation of this date as a number of "ticks".
    ///
    /// Note that length of a 'tick' is not guaranteed to represent
    /// the same amount of time across different platforms, or from
    /// one version of `coarsetime` to another.
    ///
    /// Note also that the date represented by "0" ticks is
    /// unspecified.  It is not guaranteed to be the same time across
    /// different platforms, or from one version of `coarsetime` to
    /// another.
    ///
    /// This API is mainly intended for applications that need to
    /// store the value of an `Date` in an
    /// [`AtomicU64`](std::sync::atomic::AtomicU64).
    #[inline]
    pub fn as_ticks(&self) -> u64 {
        self.as_u64()
    }

    #[doc(hidden)]
    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn _now() -> u64 {
        let mut tp = MaybeUninit::<libc::timespec>::uninit();
        let tp = unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME_COARSE, tp.as_mut_ptr());
            tp.assume_init()
        };
        _timespec_to_u64(tp.tv_sec as u64, tp.tv_nsec as u32)
    }

    #[cfg(target_os = "macos")]
    fn _now() -> u64 {
        let nsec = unsafe { clock_gettime_nsec_np(CLOCK_REALTIME) };
        _nsecs_to_u64(nsec)
    }

    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    fn _now() -> u64 {
        let mut tp = MaybeUninit::<libc::timespec>::uninit();
        let tp = unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME_COARSE, tp.as_mut_ptr());
            tp.assume_init()
        };
        _timespec_to_u64(tp.tv_sec as u64, tp.tv_nsec as u32)
    }

    #[cfg(all(
        unix,
        not(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "dragonfly"
        ))
    ))]
    fn _now() -> u64 {
        let mut tv = MaybeUninit::<libc::timeval>::uninit();
        let tv = unsafe {
            libc::gettimeofday(tv.as_mut_ptr(), null_mut());
            tv.assume_init()
        };
        _timeval_to_u64(tv.tv_sec as u64, tv.tv_usec as u32)
    }

    #[cfg(windows)]
    fn _now() -> u64 {
        _millis_to_u64(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("SystemTime before UNIX EPOCH!")
                .as_millis() as u64,
        )
    }

    #[cfg(target_os = "wasi")]
    fn _now() -> u64 {
        use wasi::{clock_time_get, CLOCKID_REALTIME};
        let nsec =
            unsafe { clock_time_get(CLOCKID_REALTIME, 1_000_000) }.expect("Clock not available");
        _nsecs_to_u64(nsec)
    }

    #[cfg(all(
        any(target_arch = "wasm32", target_arch = "wasm64"),
        target_os = "unknown"
    ))]
    fn _now() -> u64 {
        _millis_to_u64(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("SystemTime before UNIX EPOCH!")
                .as_millis() as u64,
        )
    }

    #[cfg(all(target_arch = "x86_64", target_env = "sgx", target_vendor = "fortanix"))]
    fn _now() -> u64 {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        timestamp.as_secs() * 1_000_000_000 + (timestamp.subsec_nanos() as u64)
    }

    #[inline]
    fn _update(now: u64) {
        RECENT.store(now, Ordering::Relaxed)
    }

    #[inline]
    fn _recent() -> u64 {
        let recent = RECENT.load(Ordering::Relaxed);
        if recent != 0 {
            recent
        } else {
            let now = Self::_now();
            Self::_update(now);
            Self::_recent()
        }
    }
}

impl Default for Date {
    fn default() -> Date {
        Self::now()
    }
}

impl Sub<Date> for Date {
    type Output = Duration;

    #[inline]
    fn sub(self, other: Date) -> Duration {
        Duration::from_u64(self.0.saturating_sub(other.0))
    }
}

impl Sub<Duration> for Date {
    type Output = Date;

    #[inline]
    fn sub(self, rhs: Duration) -> Date {
        Date(self.0 - rhs.as_u64())
    }
}

impl SubAssign<Duration> for Date {
    #[inline]
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl Add<Duration> for Date {
    type Output = Date;

    #[inline]
    fn add(self, rhs: Duration) -> Date {
        Date(self.0 + rhs.as_u64())
    }
}

impl AddAssign<Duration> for Date {
    #[inline]
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}
