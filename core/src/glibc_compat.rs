//! The prebuilt onnxruntime that fastembed links was compiled with glibc 2.38+ headers, which
//! redirect strtol/strtoll/strtoull to the C23 `__isoc23_*` entry points. Defining them here
//! keeps release binaries linkable and runnable on glibc 2.35 (Ubuntu 22.04, Debian 12).
//! shortcut: unlike C23 these do not parse a `0b` prefix; onnxruntime's callers do not rely on it.
use std::ffi::{c_char, c_int, c_long, c_longlong, c_ulonglong};

unsafe extern "C" {
    fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
    fn strtoll(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_longlong;
    fn strtoull(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_ulonglong;
}

/// # Safety
/// Same contract as C `strtol`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtol(
    s: *const c_char,
    end: *mut *mut c_char,
    base: c_int,
) -> c_long {
    unsafe { strtol(s, end, base) }
}

/// # Safety
/// Same contract as C `strtoll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoll(
    s: *const c_char,
    end: *mut *mut c_char,
    base: c_int,
) -> c_longlong {
    unsafe { strtoll(s, end, base) }
}

/// # Safety
/// Same contract as C `strtoull`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __isoc23_strtoull(
    s: *const c_char,
    end: *mut *mut c_char,
    base: c_int,
) -> c_ulonglong {
    unsafe { strtoull(s, end, base) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_to_the_classic_parsers() {
        let mut end = std::ptr::null_mut();
        unsafe {
            assert_eq!(__isoc23_strtol(c"-42x".as_ptr(), &mut end, 10), -42);
            assert_eq!(*end as u8, b'x');
            assert_eq!(__isoc23_strtoll(c"0x10".as_ptr(), std::ptr::null_mut(), 0), 16);
            assert_eq!(
                __isoc23_strtoull(c"18446744073709551615".as_ptr(), std::ptr::null_mut(), 10),
                u64::MAX
            );
        }
    }
}
