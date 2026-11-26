use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "Usage: retro-term [--development] [--plain] [--config PATH] [-- COMMAND [ARG ...]]";

enum Options {
    Help,
    Run {
        development: bool,
        plain: bool,
        config: Option<PathBuf>,
        command: Option<(String, Vec<String>)>,
    },
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, ()> {
    let mut args = args.into_iter();
    let mut development = false;
    let mut plain = false;
    let mut config = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--development") if !development => development = true,
            Some("--plain") if !plain => plain = true,
            Some("--config") if config.is_none() => {
                let path = args.next().filter(|path| !path.is_empty()).ok_or(())?;
                config = Some(PathBuf::from(path));
            }
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
                    plain,
                    config,
                    command: Some((executable, arguments)),
                });
            }
            _ => return Err(()),
        }
    }
    Ok(Options::Run {
        development,
        plain,
        config,
        command: None,
    })
}

fn main() -> ExitCode {
    let (development, plain, config, command) = match parse(std::env::args_os().skip(1)) {
        Ok(Options::Run {
            development,
            plain,
            config,
            command,
        }) => (development, plain, config, command),
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
    match retro_app::run_with_config(
        development,
        config.map_or_else(
            || retro_app::config::Source::Default(retro_app::config_path()),
            retro_app::config::Source::Explicit,
        ),
        retro_app::config::Overrides {
            plain,
            program: command,
        },
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("retro-term: {error}");
            ExitCode::FAILURE
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (development, plain, config, command);
        eprintln!("retro-term requires Linux and Wayland");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_default_shell_and_forwards_arguments_after_separator() {
        assert!(matches!(
            parse([]),
            Ok(Options::Run {
                command: None,
                plain: false,
                ..
            })
        ));
        let Ok(Options::Run {
            development,
            plain,
            config: None,
            command: Some((executable, arguments)),
        }) = parse(
            [
                "--development",
                "--plain",
                "--",
                "a b",
                "--help",
                "--plain",
                "--config",
                "",
            ]
            .map(OsString::from),
        )
        else {
            panic!("expected an explicit program");
        };
        assert!(development && plain);
        assert_eq!(executable, "a b");
        assert_eq!(arguments, ["--help", "--plain", "--config", ""]);
        assert!(parse([OsString::from("--")]).is_err());
    }

    #[test]
    fn config_flag_takes_one_path() {
        let Ok(Options::Run {
            config: Some(path), ..
        }) = parse(["--config", "a.toml", "--plain"].map(OsString::from))
        else {
            panic!("expected a config path");
        };
        assert_eq!(path, PathBuf::from("a.toml"));
        for invalid in [
            &["--config"][..],
            &["--config", ""],
            &["--config", "a.toml", "--config", "b.toml"],
        ] {
            assert!(
                parse(invalid.iter().map(OsString::from)).is_err(),
                "{invalid:?}"
            );
        }
    }
}
