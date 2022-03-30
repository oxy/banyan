// (Full example with detailed comments in examples/01d_quick_example.rs)
//
// This example demonstrates clap's full 'custom derive' style of creating arguments which is the
// simplest method of use, but sacrifices some flexibility.

// TODO: remove in future
#![allow(dead_code)]
#![feature(decl_macro)]
#![deny(unsafe_op_in_unsafe_fn)]

mod cli_parser;

mod error;
mod init;
mod repo;
mod util;

use std::{
    error::Error, ffi::CString, fs::File, os::unix::prelude::AsRawFd,
    sync::Arc,
};

use clap::Parser;
use cli_parser::{Opts, Commands};

use crate::util::PString;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[allow(unreachable_code)]
fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    
    let args = Opts::parse();

    match args.cmd {
        Commands::Init => {
            let mut path = std::path::PathBuf::new();
            path.push(&args.repo);
            std::fs::create_dir_all(&path)?;

            let mut objects_path = path.clone();
            let mut layers_path = path.clone();

            objects_path.push("objects");
            layers_path.push("layers");

            std::fs::create_dir(objects_path)?;
            std::fs::create_dir(layers_path)?;
        },
        Commands::Import { path, same_device } => {
            let res = repo::layer::import(
                &path,
                &args.repo,
            )?;
            println!("Successfully serialized state to {:?}.", res);
        },
    };

    return Ok(());


    Ok(())
}
