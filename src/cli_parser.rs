use clap::Parser;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields
#[derive(Parser, Debug)]
#[clap(
    version = "0.1",
    author = "Oxylibrium <oxylibrium@gmail.com>",
    about = "Rust filesystem traversal/backup toolkit",
    long_about = None
)]
pub(crate) struct Opts {
    /// Banyan repository folder location
    #[clap(short, long, default_value = "repo")]
    pub repo: String,
    /// Print more detailed logs and debug info
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: i32,
    #[clap(subcommand)]
    pub cmd: Commands,
}

#[derive(Parser, Debug)]
pub(crate) enum Commands {
    /// Initializes an object store
    Init,
    /// Imports a filesystem tree into the object store
    Import { 
        /// Path to import into the store
        path: String,
        /// Do not traverse across block devices
        #[clap(short, long)]
        same_device: bool,
    },
}
