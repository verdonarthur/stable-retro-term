use std::ffi::OsString;
use std::process::ExitCode;

const USAGE: &str = "Usage: retro-term [--development] [-- COMMAND [ARG ...]]";

enum Options {
    Help,
    Run {
        development: bool,
        command: Option<(String, Vec<String>)>,
    },
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, ()> {
    let mut args = args.into_iter();
    let mut development = false;
    for arg in args.by_ref() {
        match arg.to_str() {
            Some("--development") if !development => development = true,
            Some("--help" | "-h") => return Ok(Options::Help),
            Some("--") => {
                let executable = args.next().filter(|name| !name.is_empty()).ok_or(())?;
                let executable = executable.into_string().map_err(|_| ())?;
                let arguments = args
                    .map(OsString::into_string)
                    .collect::<Result<_, _>>()
                    .map_err(|_| ())?;
                return Ok(Options::Run {
                    development,
                    command: Some((executable, arguments)),
                });
            }
            _ => return Err(()),
        }
    }
    Ok(Options::Run {
        development,
        command: None,
    })
}

fn main() -> ExitCode {
    let (development, command) = match parse(std::env::args_os().skip(1)) {
        Ok(Options::Run {
            development,
            command,
        }) => (development, command),
        Ok(Options::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(()) => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    #[cfg(target_os = "linux")]
    match retro_app::run_with_program(
        development,
        command.map(|(executable, arguments)| retro_terminal::Program {
            executable,
            arguments,
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("retro-term: {error}");
            ExitCode::FAILURE
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (development, command);
        eprintln!("retro-term requires Linux and Wayland");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_default_shell_and_forwards_arguments_after_separator() {
        assert!(matches!(parse([]), Ok(Options::Run { command: None, .. })));
        let Ok(Options::Run {
            development,
            command: Some((executable, arguments)),
        }) = parse(["--development", "--", "a b", "--help", ""].map(OsString::from))
        else {
            panic!("expected an explicit program");
        };
        assert!(development);
        assert_eq!(executable, "a b");
        assert_eq!(arguments, ["--help", ""]);
        assert!(parse([OsString::from("--")]).is_err());
    }
}
