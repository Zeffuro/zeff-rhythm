use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    zeff_rhythm::cli::run()
}
