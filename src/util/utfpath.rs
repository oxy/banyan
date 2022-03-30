use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;

pub fn os_to_utf(str: &OsStr) -> Result<String, io::Error> {
    Ok(str
        .to_str()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{:?} is not valid UTF-8", str),
            )
        })?
        .to_owned())
}

pub fn joinpath<T: AsRef<OsStr>, T2: AsRef<OsStr>>(
    base: T,
    relpath: T2,
) -> PathBuf {
    let base = base.as_ref();
    let relpath = relpath.as_ref();
    let mut x = OsString::with_capacity(base.len() + relpath.len());
    x.push(base);
    x.push(relpath);
    PathBuf::from(x)
}
