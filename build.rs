use std::error::Error;
use std::path::PathBuf;
use std::{env, fs};

use clap::CommandFactory;

pub mod cli {
    include!("src/cli.rs");
}

fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let crate_name = env::var("CARGO_PKG_NAME")?;

    let cli = cli::Cli::command();

    let man = clap_mangen::Man::new(cli);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;

    fs::write(out_dir.join(format!("{crate_name}.1")), buffer)?;

    Ok(())
}
