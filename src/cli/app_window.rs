use std::error::Error;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    crate::app::native_shell::run(args)
}
