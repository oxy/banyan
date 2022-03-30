use std::{
    ffi::{CStr, CString, OsStr},
    ops::Add,
    os::unix::prelude::OsStrExt,
    path::Path,
    str::Utf8Error,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd)]
pub struct PString {
    length: usize,
    cstr: CString,
}

impl Ord for PString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cstr.cmp(&other.cstr)
    }
}

impl AsRef<[u8]> for PString {
    fn as_ref(&self) -> &[u8] {
        self.cstr.as_bytes()
    }
}

impl AsRef<Path> for PString {
    fn as_ref(&self) -> &Path {
        Path::new(OsStr::from_bytes(self.as_ref()))
    }
}

impl AsRef<CStr> for PString {
    fn as_ref(&self) -> &CStr {
        &self.cstr
    }
}

impl AsRef<str> for PString {
    fn as_ref(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.cstr.as_bytes()) }
    }
}

impl<'a> Add<&'a PString> for PString {
    type Output = PString;

    fn add(self, rhs: &'a PString) -> Self::Output {
        let length = self.length + rhs.length;

        let cstr = unsafe {
            let mut x: Vec<u8> = Vec::with_capacity(length + 1);
            std::ptr::copy_nonoverlapping::<u8>(
                self.cstr.as_ptr() as *const u8,
                x.as_mut_ptr(),
                self.length,
            );
            std::ptr::copy_nonoverlapping::<u8>(
                rhs.cstr.as_ptr() as *const u8,
                x.as_mut_ptr().add(self.length),
                rhs.length + 1,
            );
            x.set_len(length + 1);
            CString::from_vec_with_nul_unchecked(x)
        };
        PString { length, cstr }
    }
}

impl<'a> Add<&'a CStr> for PString {
    type Output = PString;

    fn add(self, rhs: &'a CStr) -> Self::Output {
        let rhs_len_with_nul = rhs.to_bytes_with_nul().len();
        let len_with_nul = self.length + rhs_len_with_nul;
        let cstr = unsafe {
            let mut x: Vec<u8> = Vec::with_capacity(len_with_nul);
            std::ptr::copy_nonoverlapping::<u8>(
                self.cstr.as_ptr() as *const u8,
                x.as_mut_ptr(),
                self.length,
            );
            std::ptr::copy_nonoverlapping::<u8>(
                rhs.as_ptr() as *const u8,
                x.as_mut_ptr().add(self.length),
                rhs_len_with_nul,
            );
            x.set_len(len_with_nul);
            CString::from_vec_with_nul_unchecked(x)
        };

        PString { length: len_with_nul - 1, cstr }
    }
}
impl std::fmt::Debug for PString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cstr.fmt(f)
    }
}

impl PString {
    pub fn from_str(s: &str) -> PString {
        let length = s.len();
        let cstr =
            unsafe { CString::from_vec_unchecked(s.as_bytes().to_owned()) };
        PString { length, cstr }
    }

    pub fn from_cstring(cstr: CString) -> Result<PString, Utf8Error> {
        std::str::from_utf8(cstr.to_bytes())?;
        Ok(unsafe { PString::from_cstring_unchecked(cstr) })
    }

    pub unsafe fn from_cstring_unchecked(cstr: CString) -> PString {
        let length = cstr.to_bytes().len();
        PString { length, cstr }
    }

    pub fn append_path(&self, filename: &CStr) -> PString {
        let bytes = self.cstr.as_bytes();
        let filebytes = filename.to_bytes();

        let addsep = bytes[bytes.len() - 1] != b'/';

        let len = bytes.len() + filebytes.len() + addsep as usize;

        let cstr = unsafe {
            let mut res: Vec<u8> = Vec::with_capacity(len + 1);
            res.set_len(len + 1);
            std::ptr::copy_nonoverlapping::<u8>(
                bytes.as_ptr() as *const u8,
                res.as_mut_ptr(),
                self.length,
            );
            if addsep {
                res[self.length] = b'/'
            };
            std::ptr::copy_nonoverlapping::<u8>(
                filebytes.as_ptr() as *const u8,
                res.as_mut_ptr().add(self.length).add(addsep as usize),
                filebytes.len() + 1,
            );
            res.set_len(len + 1);
            CString::from_vec_with_nul_unchecked(res)
        };

        PString { length: len, cstr }
    }
}
