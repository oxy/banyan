use std::fs;
use std::path;

pub fn init_repo(repo_path: &str) -> Result<(), std::io::Error> {
    let pathbuf = path::PathBuf::from(repo_path);
    fs::create_dir(&pathbuf)?;

    for subdir in &["objects", "layers", "info", "localstate"] {
        fs::create_dir({
            let mut object_dir = pathbuf.clone();
            object_dir.extend(&[subdir]);
            object_dir
        })?;
    }

    Ok(())
}
