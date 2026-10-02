#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = zeff_rhythm::app::wgpu_shell::run(&args) {
        show_error(&error.to_string());
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Zeff Rhythm".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn show_error(message: &str) {
    eprintln!("Zeff Rhythm: {message}");
}
