use std::{io, process::ExitCode};

fn main() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    match poc2_windows_ocr::run(&mut reader, &mut writer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("poc2-windows-ocr: protocol I/O failed: {error}");
            ExitCode::FAILURE
        }
    }
}
