use std::error::Error;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    crate::app::wgpu_shell::run(args)
}
