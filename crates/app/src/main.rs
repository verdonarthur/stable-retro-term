use std::process::ExitCode;

fn main() -> ExitCode {
    let mut development = false;
    for argument in std::env::args_os().skip(1) {
        match argument.to_str() {
            Some("--development") if !development => development = true,
            Some("--help" | "-h") => {
                println!("Usage: retro-term [--development]");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("Usage: retro-term [--development]");
                return ExitCode::from(2);
            }
        }
    }
    #[cfg(target_os = "linux")]
    match retro_app::run(development) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("retro-term: {error}");
            ExitCode::FAILURE
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = development;
        eprintln!("retro-term requires Linux and Wayland");
        ExitCode::FAILURE
    }
}
