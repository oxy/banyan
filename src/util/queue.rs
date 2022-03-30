use super::aparc::APArc;
use super::{openat, PString};
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::os::unix::prelude::RawFd;
use std::os::raw::c_char;
use std::ptr::{null, self};
use std::sync::atomic::{AtomicIsize, AtomicUsize, AtomicPtr, Ordering};
use std::sync::{Arc};
use std::time::Duration;
use parking_lot::Mutex;

const NODE_LEN: usize = 4096 - (3 * 64);

#[derive(Debug)]
pub(crate) struct NodeData {
    basedir: Arc<PString>,
    offset: AtomicIsize,
    data: [u8; NODE_LEN],
    size: isize,
}

pub(crate) struct NodeSlice {
    node: Arc<NodeData>,
    start: isize,
}

impl std::fmt::Debug for NodeSlice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeSlice")
            .field("d_ino", &self.inode())
            .field("d_off", &self.offset())
            .field("d_reclen", &self.size())
            .field("d_type", &self.filetype())
            .field("filename", &self.filename())
            .finish()
    }
}

/*
struct linux_dirent64 {
    ino64_t        d_ino;    /* 64-bit inode number */
    off64_t        d_off;    /* 64-bit offset to next structure */
    unsigned short d_reclen; /* Size of this dirent */
    unsigned char  d_type;   /* File type */
    char           d_name[]; /* Filename (null-terminated) */
};
*/

impl<'a> NodeSlice {
    #[inline]
    pub(crate) fn inode(&self) -> u64 {
        unsafe { *(self.node.data.as_ptr().offset(self.start) as *const u64) }
    }

    #[inline]
    pub(crate) fn offset(&self) -> u64 {
        unsafe {
            *(self.node.data.as_ptr().offset(self.start + 8) as *const u64)
        }
    }

    #[inline]
    pub(crate) fn size(&self) -> u16 {
        unsafe {
            *(self.node.data.as_ptr().offset(self.start + 16) as *const u16)
        }
    }

    #[inline]
    pub(crate) fn filetype(&self) -> u8 {
        unsafe {
            *(self.node.data.as_ptr().offset(self.start + 18) as *const u8)
        }
    }

    #[inline]
    pub(crate) fn filename(&'a self) -> &'a CStr {
        unsafe {
            CStr::from_ptr(
                self.node.data.as_ptr().offset(self.start + 19) as *const c_char
            )
        }
    }

    #[inline]
    pub(crate) fn fullpath(&'a self) -> PString {
        self.node.basedir.append_path(self.filename())
    }
}

impl<'a> NodeData {
    #[inline]
    pub(crate) fn new(
        fd: RawFd,
        path: Arc<PString>,
    ) -> Result<Option<NodeData>, std::io::Error> {
        let n = NodeData::new_internal(fd, path)?;
        if n.size == 0 {
            Ok(None)
        } else {
            Ok(Some(n))
        }
    }

    /// Create a new Node, and populate it by reading data from a
    /// file descriptor that points to a folder.
    ///
    /// Requires that the file descriptor is valid and refers to an
    /// open folder.
    fn new_internal(
        fd: RawFd,
        path: Arc<PString>,
    ) -> Result<NodeData, std::io::Error> {
        let mut n = NodeData {
            basedir: path.clone(),
            offset: AtomicIsize::new(0),
            // SAFETY: we don't care about the data here,
            // and update it immediately after with the syscall.
            data: unsafe {
                MaybeUninit::<[u8; NODE_LEN]>::uninit().assume_init()
            },
            // SAFETY: we should always validate for nulls
            size: 0,
        };
        let ret: i64 =
            unsafe { libc::syscall(217, fd, n.data.as_ptr(), NODE_LEN) };
        if ret < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            n.size = ret as isize;
            Ok(n)
        }
    }

    /// Advance the current Node, optionally returning a NodeSlice
    /// that points to a linux_dirent, or None if all dirents
    /// have been exhausted in the current Node.
    #[inline]
    pub(crate) fn advance(self: &Arc<NodeData>) -> Option<NodeSlice> {
        let mut old = self.offset.load(Ordering::Relaxed);
        while old < self.size {
            let size: u16 = unsafe {
                *(self.data.as_ptr().offset(old + 16) as *const u16)
            };
            let new: isize = old + size as isize;
            match self.offset.compare_exchange_weak(
                old,
                new,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Some(NodeSlice { node: self.clone(), start: old });
                }
                Err(x) => old = x,
            }
        }
        None
    }
}

pub(crate) struct Node {
    pub(crate) data: Arc<NodeData>,
    pub(crate) next: AtomicPtr<Node>
}

#[derive(Default)]
pub(crate) struct Queue {
    head: Mutex<usize>,
    tail: AtomicPtr<Node>,
}

impl Queue {
    pub fn new_with_folder(
        dirfd: RawFd,
        path: Arc<PString>
    ) -> Result<Queue, std::io::Error> {
        let head_data = match NodeData::new(dirfd, path.clone())? {
            Some(data) => data,
            None => return Err(std::io::Error::new(std::io::ErrorKind::Other, "directory is empty."))
        };
        
        let mut head = Box::new(Node {
            data: Arc::new(head_data),
            next: AtomicPtr::default()
        });

        let ptr = Box::into_raw(head);

        let queue = Queue {
            head: Mutex::new(ptr as usize),
            tail: AtomicPtr::new(ptr)
        };


        queue.add_folder(dirfd, path)?;
        Ok(queue)
    }

    pub fn add_folder(
        &self,
        dirfd: RawFd,
        path: Arc<PString>,
    ) -> Result<(), std::io::Error> {
        while let Some(data) = NodeData::new(dirfd, path.clone())? {
            let endearly = data.size < 3072;
            let node = Box::new(Node {
                data: Arc::new(data),
                next: AtomicPtr::default()
            });
            self.add_node(node);
            if endearly {
                break;
            }
        }
        Ok(())
    }

    pub fn add_path_at(
        &self,
        parentfd: RawFd,
        path: Arc<PString>,
    ) -> Result<(), std::io::Error> {
        // NOTE: this pattern looks kind of ~weird~
        //                      /-> Arc<PString> -> &PString
        //                      |        /-> &PString -> &CStr
        let cpath: &CStr = path.as_ref().as_ref();
        let fd = openat(parentfd, cpath, libc::O_DIRECTORY)?;
        self.add_folder(fd, path)?;
        unsafe { libc::close(fd) };
        Ok(())
    }

    pub fn add_node(&self, next: Box<Node>) {
        let ptr = Box::into_raw(next);
        loop {
            // SAFETY: tail should never be null.
            let tailptr = self.tail.load(Ordering::Relaxed);
            let tail = unsafe { std::mem::transmute::<*mut Node, &Node>(tailptr) };

            match tail.next.compare_exchange(
                ptr::null_mut::<>(),
                ptr,
                Ordering::AcqRel,
                Ordering::Acquire
            ) {
                    // Succeeded at updating tail.next
                    Ok(_) => {
                        // Try to update our tracked tail.
                        // - success: we updated the tail!
                        // - failure: we stalled, and someone else fixed
                        //   it for us when we were not scheduled to run.
                        self.tail.compare_exchange(
                            tailptr, 
                            ptr,
                            Ordering::AcqRel,
                            Ordering::Acquire
                        );
                        return;
                    }
                    // We get here one of two ways:
                    // - we stalled between load and swap,
                    // - another thread stalled between swap_null(next)
                    //   and swap_existing(tail, next)
                    // Address this by trying to update the real tail ourselves.
                    Err(real_tail) => {
                        // One of two things can happen here:
                        // - success: thus fixing the stall
                        // - failure: because either we stalled or another
                        //   thread unstalled, fixing it for us.
                        self.tail.compare_exchange(
                            tailptr,
                            real_tail,
                            Ordering::AcqRel,
                            Ordering::Acquire
                        );
                    }
            }
        }
    }

    pub fn advance(&self) -> Option<NodeSlice> {
        loop {
            let mut headlock = self.head.lock();
            let headptr = *headlock;
            let head = unsafe { std::mem::transmute::<usize, &Node>(headptr) };

            match head.data.advance() {
                Some(slice) => {
                    return Some(slice)
                },
                None => {
                    let next = head.next.load(Ordering::Relaxed);
                    if next == ptr::null_mut::<>() {
                        return None;
                    } else {
                        if *headlock == headptr {
                            unsafe { Box::from_raw(*headlock as *mut Node) };
                            *headlock = next as usize;
                        }
                    }
                }
            }
        };
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        let mut head = self.head.lock();
        while *head != 0 {
            let b = unsafe {Box::from_raw(*head as *mut Node)};
            *head = b.next.load(Ordering::Relaxed) as usize;
        }
    }
}