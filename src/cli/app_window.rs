use std::error::Error;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if !args.is_empty() {
        return Err("usage: zeff-rhythm app".into());
    }

    crate::app::native_shell::run()
}
