use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::CString;
use std::fs::{File, Metadata};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::prelude::{AsRawFd, RawFd, OsStrExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use libc::{DT_LNK, DT_UNKNOWN, O_DIRECTORY, O_NOFOLLOW, S_IFLNK, S_IFMT};
use serde::{Deserialize, Serialize};

use crate::repo::object;
use crate::util::queue::{NodeSlice, Queue};
use crate::util::{
    self, close, lstatat, openat, os_to_utf, readlinkat, PString,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    hash: String,
    perms: u32,
    uid: u32,
    gid: u32,
    xattrs: Option<BTreeMap<String, Vec<u8>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirState {
    perms: u32,
    uid: u32,
    gid: u32,
    xattrs: Option<BTreeMap<String, Vec<u8>>>,
}

pub struct Layer {
    fs: FsState,
    timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FsState {
    dirs: BTreeMap<PString, DirState>,
    objects: BTreeMap<PString, Object>,
    links: BTreeMap<PString, String>,
}

impl FsState {
    fn extend(&mut self, other: Self) {
        self.dirs.extend(other.dirs);
        self.objects.extend(other.objects);
    }
}

impl FsState {
    fn new() -> FsState {
        FsState {
            dirs: BTreeMap::new(),
            objects: BTreeMap::new(),
            links: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
struct Work {
    /// The directory entry.
    relpath: PathBuf,
    /// Metadata; we always get the metadata to check the root device,
    /// so just keep it around :p
    metadata: Metadata,
}

struct WalkOptions {
    ignore_errors: bool,
    root_device: Option<u64>,
}

#[derive(Debug)]
struct WalkError {
    path: PString,
    error: io::Error,
}

struct NQWorker {
    queue: Arc<Queue>,
    /// Whether all workers should terminate at the next opportunity. Note
    /// that we need this because we don't want other `Work` to be done after
    /// we quit. We wouldn't need this if have a priority channel.
    quit_now: Arc<AtomicBool>,
    active_workers: Arc<AtomicUsize>,
    state: FsState,
    options: Arc<WalkOptions>,
    errors: Arc<Mutex<Vec<WalkError>>>,
    fd: RawFd,
    objectfd: RawFd,
}

enum WalkState {
    Continue,
    Quit,
}

impl NQWorker {
    /// Runs this worker until there is no more work left to do.
    ///
    /// The worker will call the caller's callback for all entries that aren't
    /// skipped by the ignore matcher.
    fn run(mut self) -> FsState {
        while !self.should_quit() {
            match self.queue.advance() {
                Some(dent) => {
                    // Add to current working thread count, process work item
                    // and then subtract when done.
                    self.active_workers.fetch_add(1, Ordering::Relaxed);
                    let res = self.visit(dent);
                    self.active_workers.fetch_sub(1, Ordering::Relaxed);
                    
                    // If we're expected to quit, quit now.
                    if let WalkState::Quit = res {
                        self.quit_now();
                        break;
                    }
                }
                // If there's nothing in the queue, and no threads are busy,
                // then we end the worker. If there's work being done, we spin.
                None => {
                    if self.active_workers.load(Ordering::Relaxed) == 0 {
                        break;
                    }
                }
            }
        }

        self.state
    }

    fn visit(&mut self, dent: NodeSlice) -> WalkState {
        let fname = dent.filename();
        if fname == CString::new(".").unwrap().as_ref() || fname == CString::new("..").unwrap().as_ref() {
            return WalkState::Continue;
        }

        let path = dent.fullpath();

        match self.visit_internal(&dent, path) {
            Ok(_) => WalkState::Continue,
            Err(e) => self.handle_error(&dent.fullpath(), e),
        }
    }

    fn handle_error(&mut self, path: &PString, error: io::Error) -> WalkState {
        self.errors
            .lock()
            .unwrap()
            .push(WalkError { path: path.clone(), error });

        match self.options.ignore_errors {
            true => WalkState::Continue,
            false => WalkState::Quit,
        }
    }

    fn visit_internal(
        &mut self,
        dent: &NodeSlice,
        path: PString,
    ) -> Result<(), std::io::Error> {
        let (stat, link) = if dent.filetype() == DT_UNKNOWN {
            let stat = util::lstatat(self.fd, path.as_ref())?;
            let link = stat.st_mode & S_IFMT == S_IFLNK;
            (Some(stat), link)
        } else {
            (None, dent.filetype() == DT_LNK)
        };

        if link {
            let link = readlinkat(self.fd, path.as_ref())?;
            self.state.links.insert(path, os_to_utf(link.as_os_str())?);
            return Ok(());
        }

        let stat = match stat {
            Some(stat) => stat,
            None => lstatat(self.fd, path.as_ref())?,
        };

        // TODO: check if same device

        let dir = stat.st_mode & libc::S_IFMT == libc::S_IFDIR;

        let fd = openat(
            self.fd,
            path.as_ref(),
            O_NOFOLLOW & if dir { O_DIRECTORY } else { 0 },
        )?;
        if dir {
            self.queue.add_folder(fd, Arc::new(path.clone()))?;
            self.state.dirs.insert(
                path,
                DirState {
                    perms: stat.st_mode
                        & (libc::S_IRWXU | libc::S_IRWXG | libc::S_IRWXO),
                    uid: stat.st_uid,
                    gid: stat.st_gid,
                    xattrs: util::xattrs(fd)?,
                },
            );
        } else {
            // we assume its a file, TOCTOU be damned
            let hash = object::import(fd, self.objectfd)?;
            self.state.objects.insert(
                path,
                Object {
                    hash,
                    perms: stat.st_mode
                        & (libc::S_IRWXU | libc::S_IRWXG | libc::S_IRWXO),
                    uid: stat.st_uid,
                    gid: stat.st_gid,
                    xattrs: util::xattrs(fd)?,
                },
            );
        }

        close(fd)?;

        Ok(())
    }

    /// Indicates that all workers should quit immediately.
    fn quit_now(&self) {
        self.quit_now.store(true, Ordering::Release);
    }

    /// Returns true if this worker should quit immediately.
    fn should_quit(&self) -> bool {
        self.quit_now.load(Ordering::Relaxed)
    }
}

fn visit(
    basepath: PathBuf,
    mut repo: PathBuf,
    ignore_errors: bool,
    same_device: bool,
) -> Result<FsState, io::Error> {
    let threads = std::thread::available_parallelism()?.get();
    let threads = if (threads > 4) {
        threads - 2
    } else {
        threads
    };
    let metadata = basepath.metadata()?;
    let dev = metadata.dev();

    let dirfd = util::openat(libc::AT_FDCWD, &CString::new(basepath.as_os_str().as_bytes().to_vec())?, O_DIRECTORY)?;
    repo.push("objects");
    let objectfd = util::openat(libc::AT_FDCWD, &CString::new(repo.as_os_str().as_bytes().to_vec())?, O_DIRECTORY)?;
    let queue = Arc::new(
        util::queue::Queue::new_with_folder(dirfd, Arc::new(util::PString::from_str(".")))?
    );

    let options = Arc::new(WalkOptions {
        ignore_errors,
        root_device: if same_device { Some(dev) } else { None },
    });

    // Create the workers and then wait for them to finish.
    let quit_now = Arc::new(AtomicBool::new(false));
    let active_workers = Arc::new(AtomicUsize::new(0));
    let mut final_state = FsState::new();
    let errors: Arc<Mutex<Vec<WalkError>>> = Arc::new(Mutex::new(vec![]));
    crossbeam_utils::thread::scope(|s| {
        let mut handles = vec![];
        for _ in 0..threads {
            let worker = NQWorker {
                queue: queue.clone(),
                quit_now: quit_now.clone(),
                active_workers: active_workers.clone(),
                state: FsState::new(),
                errors: errors.clone(),
                options: options.clone(),
                fd: dirfd,
                objectfd
            };
            handles.push(s.spawn(|_| worker.run()));
        }
        for handle in handles {
            final_state.extend(handle.join().unwrap());
        }
    })
    .unwrap(); // Pass along panics from threads
    
    util::close(dirfd)?;
    util::close(objectfd)?;

    Ok(final_state)
}

/// Import a filesystem tree.
pub fn import(
    path: &str,
    repo_basedir: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let path = PathBuf::from(path.trim_end_matches('/'));
    let state = visit(path, PathBuf::from(&repo_basedir), false, true)?;
    println!("Visited {:?} directories and {:?} objects", state.dirs.len(), state.objects.len() + state.links.len());
    
    let ser = bincode::serialize(&state)?;
    let statehash = base64::encode_config(
        blake3::hash(&ser).as_bytes(),
        base64::URL_SAFE_NO_PAD,
    );

    let mut path = PathBuf::from(&repo_basedir);
    path.push("layers");
    path.push(&statehash);

    let mut layer = std::fs::File::create(path)?;
    layer.write(&ser);

    Ok(statehash)
}