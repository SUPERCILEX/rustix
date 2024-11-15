//! Efficient decimal integer formatting.
//!
//! # Safety
//!
//! This uses `CStr::from_bytes_with_nul_unchecked` and
//! `str::from_utf8_unchecked`on the buffer that it filled itself.
#![allow(unsafe_code)]

use crate::backend::fd::{AsFd, AsRawFd};
use crate::ffi::CStr;
use core::any::TypeId;
use core::mem::{self, MaybeUninit};
use itoa::{Buffer, Integer};
#[cfg(all(feature = "std", unix))]
use std::os::unix::ffi::OsStrExt;
#[cfg(all(feature = "std", target_os = "wasi"))]
use std::os::wasi::ffi::OsStrExt;
#[cfg(feature = "std")]
use {core::fmt, std::ffi::OsStr, std::path::Path};

/// Format an integer into a decimal `Path` component, without constructing a
/// temporary `PathBuf` or `String`.
///
/// This is used for opening paths such as `/proc/self/fd/<fd>` on Linux.
///
/// # Examples
///
/// ```
/// # #[cfg(any(feature = "fs", feature = "net"))]
/// use rustix::path::DecInt;
///
/// # #[cfg(any(feature = "fs", feature = "net"))]
/// assert_eq!(
///     format!("hello {}", DecInt::new(9876).as_ref().display()),
///     "hello 9876"
/// );
/// ```
#[derive(Clone)]
pub struct DecInt {
    // Enough to hold an i64 and NUL terminator.
    buf: [MaybeUninit<u8>; "-9223372036854775808\0".len()],
    len: usize,
}

impl DecInt {
    /// Construct a new path component from an integer.
    #[inline]
    pub fn new<Int: Integer + 'static>(i: Int) -> Self {
        let mut buf = [MaybeUninit::uninit(); 21];

        let mut str_buf = Buffer::new();
        let str_buf = str_buf.format(i);
        {
            let max_buf_size = {
                let bits = match TypeId::of::<Int>() {
                    id if [TypeId::of::<i8>(), TypeId::of::<u8>()].contains(&id) => u8::BITS,
                    id if [TypeId::of::<i16>() , TypeId::of::<u16>()].contains(&id) => u16::BITS,
                    id if [TypeId::of::<i32>() , TypeId::of::<u32>()].contains(&id) => u32::BITS,
                    id if [TypeId::of::<i64>() , TypeId::of::<u64>()].contains(&id) => u64::BITS,
                    id if [TypeId::of::<i128>() , TypeId::of::<u128>()].contains(&id) => u128::BITS,
                    id if [TypeId::of::<isize>() , TypeId::of::<usize>()].contains(&id) => usize::BITS,
                    _ => unreachable!(),
                };
                match bits {
                    8 => "-128".len(),
                    16 => "-32768".len(),
                    32 => "-2147483648".len(),
                    64 => "-9223372036854775808".len(),
                    128 => "-170141183460469231731687303715884105728".len(),
                    _ => unreachable!(),
                }
            };
            if str_buf.len() > max_buf_size {
                unsafe { core::hint::unreachable_unchecked() }
            }
            assert!(str_buf.len() < buf.len(), "{} unsupported.", core::any::type_name::<Int>());

            buf[..str_buf.len()].copy_from_slice(unsafe {
                // SAFETY: you can always go from init to uninit
                mem::transmute::<&[u8], &[MaybeUninit<u8>]>(str_buf.as_bytes())
            });
            buf[str_buf.len()] = MaybeUninit::new(0);
        }

        Self {
            buf,
            len: str_buf.len(),
        }
    }

    /// Construct a new path component from a file descriptor.
    #[inline]
    pub fn from_fd<Fd: AsFd>(fd: Fd) -> Self {
        Self::new(fd.as_fd().as_raw_fd())
    }

    /// Return the raw byte buffer as a `&str`.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: `DecInt` always holds a formatted decimal number, so it's
        // always valid UTF-8.
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Return the raw byte buffer as a `&CStr`.
    #[inline]
    pub fn as_c_str(&self) -> &CStr {
        let bytes_with_nul = self.as_bytes_with_nul();
        debug_assert!(CStr::from_bytes_with_nul(bytes_with_nul).is_ok());

        // SAFETY: `self.buf` holds a single decimal ASCII representation and
        // at least one extra NUL byte.
        unsafe { CStr::from_bytes_with_nul_unchecked(bytes_with_nul) }
    }

    /// Return the raw byte buffer including the NUL byte.
    #[inline]
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        let init = &self.buf[..=self.len];
        // SAFETY: we're guaranteed to have initialized len+1 bytes.
        unsafe { mem::transmute::<&[MaybeUninit<u8>], &[u8]>(init) }
    }

    /// Return the raw byte buffer.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let bytes = self.as_bytes_with_nul();
        &bytes[..bytes.len() - 1]
    }
}

#[cfg(feature = "std")]
impl AsRef<Path> for DecInt {
    #[inline]
    fn as_ref(&self) -> &Path {
        let as_os_str: &OsStr = OsStrExt::from_bytes(self.as_bytes());
        Path::new(as_os_str)
    }
}

#[cfg(feature = "std")]
impl fmt::Debug for DecInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}
