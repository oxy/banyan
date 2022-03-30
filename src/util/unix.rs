use std::collections::BTreeMap;
use std::ffi::{CStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_char};
use std::os::unix::prelude::{OsStringExt, RawFd};
use std::path::PathBuf;
use std::ptr;

pub(crate) fn xattrs(
    fd: RawFd,
) -> Result<Option<BTreeMap<String, Vec<u8>>>, std::io::Error> {
    // Get size of list of xattrs
    let size = unsafe {
        let ret = libc::flistxattr(fd, ptr::null_mut(), 0);
        if ret == -1 {
            return Err(std::io::Error::last_os_error());
        }
        ret as usize
    };

    // If there are no keys, there must be no xattrs
    if size == 0 {
        return Ok(None);
    };

    let mut result = BTreeMap::new();

    // Get the null-separated list of xattr keys
    let mut xattrs: Vec<u8> = vec![0; size];
    {
        let size = unsafe {
            libc::flistxattr(fd, xattrs.as_mut_ptr() as *mut c_char, size)
        };
        if size == -1 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe { xattrs.set_len(size as usize) };
    };

    let mut i: usize = 0;

    // Step through each xattr key using libc::strlen to find nulls,
    // since libc::strlen is almost guaranteed to be faster than
    // looping in Rust (I hope?)
    while i < xattrs.len() {
        // Horrible, terrible, no good, very bad hack

        // SAFETY: Kernel guarantees null-separated list
        let len =
            unsafe { libc::strlen(xattrs.as_ptr().add(i) as *const c_char) } + 1;

        // SAFETY: We just found the null
        let name = unsafe {
            CStr::from_bytes_with_nul_unchecked(&xattrs[i..i + len])
        };

        // Get the size of the value of the xattr
        let value_size = {
            let ret = unsafe {
                libc::fgetxattr(fd, name.as_ptr(), ptr::null_mut(), 0)
            };
            if ret == -1 {
                return Err(std::io::Error::last_os_error());
            }
            ret as usize
        };

        // Allocate space for the xattr
        let mut value: Vec<u8> = vec![0; value_size];

        // Load the xattr
        let value_size = unsafe {
            libc::fgetxattr(
                fd,
                name.as_ptr(),
                value.as_mut_ptr() as *mut libc::c_void,
                value_size,
            )
        };
        if value_size == -1 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe { value.set_len(size as usize) };

        // Store in result!
        // TODO: change to Vec<u8> because keys are not guaranteed
        // to be valid UTF-8 on Linux
        result.insert(name.to_string_lossy().to_string(), value);

        // Move forward in the xattr set
        i += len;
    }

    Ok(Some(result))
}

#[inline]
pub(crate) fn open(
    path: &CStr,
    oflag: c_int,
) -> Result<RawFd, std::io::Error> {
    let fd = unsafe { libc::open64(path.as_ptr(), oflag) };
    if fd == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(fd)
    }
}

#[inline]
pub(crate) fn openat(
    dirfd: RawFd,
    path: &CStr,
    oflag: c_int,
) -> Result<RawFd, std::io::Error> {
    let fd = unsafe { libc::openat64(dirfd, path.as_ptr(), oflag) };
    if fd == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(fd)
    }
}

#[inline]
pub(crate) fn lstatat(
    dirfd: RawFd,
    path: &CStr,
) -> Result<libc::stat, std::io::Error> {
    let mut meta = MaybeUninit::uninit();
    let ret = unsafe {
        libc::fstatat(
            dirfd,
            path.as_ptr(),
            meta.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if ret == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { meta.assume_init() })
    }
}

/// A wrapper for libc::readlinkat that manages generating a buffer and figuring.
///
#[inline]
pub(crate) fn readlinkat(dirfd: RawFd, path: &CStr) -> io::Result<PathBuf> {
    let mut buf = Vec::with_capacity(256);

    loop {
        let ret = unsafe {
            libc::readlinkat(
                dirfd,
                path.as_ptr(),
                buf.as_mut_ptr() as *mut _,
                buf.capacity(),
            )
        };
        if ret == -1 {
            return Err(std::io::Error::last_os_error());
        }

        let ret = ret as usize;

        unsafe {
            buf.set_len(ret);
        }

        if ret != buf.capacity() {
            buf.shrink_to_fit();

            return Ok(PathBuf::from(OsString::from_vec(buf)));
        }

        // Trigger the internal buffer resizing logic of `Vec` by requiring
        // more space than the current capacity. The length is guaranteed to be
        // the same as the capacity due to the if statement above.
        buf.reserve(1);
    }
}

pub(crate) fn close(fd: RawFd) -> io::Result<()> {
    let ret = unsafe { libc::close(fd) };
    if ret == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
