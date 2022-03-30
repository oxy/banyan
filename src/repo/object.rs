use std::ffi::CString;
use std::fs;
use std::io;
use std::io::Read;
use std::os::unix::prelude::RawFd;
use std::cell::RefCell;

thread_local! {
    pub static READ_BUF: RefCell<Vec<u8>> = RefCell::new(vec![0u8; 16384]);
}

#[cfg(unix)]
pub fn import(file: RawFd, repofd: RawFd) -> Result<String, std::io::Error> {
    use std::{
        io::Seek,
        os::unix::prelude::{FromRawFd, IntoRawFd}, borrow::BorrowMut,
    };

    use libc::{O_CREAT, O_EXCL, O_WRONLY};

    use crate::util::openat;

    let mut hasher = blake3::Hasher::new();
    let mut file = unsafe { fs::File::from_raw_fd(file) };

    READ_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        #[allow(irrefutable_let_patterns)]
        while let n = file.read(&mut buf)? {
            if n != 16384 {
                let rest = &buf[0..n];
                hasher.update(rest);
                break;
            } else {
                hasher.update(&buf);
            }
        }
    
        let hash = base64::encode_config(
            hasher.finalize().as_bytes(),
            base64::URL_SAFE_NO_PAD,
        );
    
        file.rewind()?;
    
        let ret = match openat(
            repofd,
            &CString::new(hash.clone())?,
            O_CREAT | O_EXCL | O_WRONLY,
        ) {
            Ok(fd) => {
                let mut resfile = unsafe { fs::File::from_raw_fd(fd) };
                io::copy(&mut file, &mut resfile)?;
                Ok(hash)
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    Ok(hash)
                } else {
                    Err(e)
                }
            }
        };    
        // Do not close!
        file.into_raw_fd();

        ret    
    })
}
