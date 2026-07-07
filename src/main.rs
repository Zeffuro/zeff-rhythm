mod app;
mod cli;
mod platform;
mod play;
mod render;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    cli::run()
}
