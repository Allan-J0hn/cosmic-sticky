// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

pub const USAGE: &str = "\
Usage: cosmic-sticky [OPTION]

Sticky notes on the COSMIC desktop. Options act on the running instance, or start one.

  --toggle             show or hide all notes
  --new                new note (pick its category)
  --new --category NAME
                       new note in category NAME
  --list               open the notes list
  -h, --help           show this help";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    New { category: Option<String> },
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    Run(Option<Command>),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Unknown(String),
    MissingValue(&'static str),
    Conflict,
    CategoryWithoutNew,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(a) => write!(f, "unknown argument '{a}'"),
            Self::MissingValue(o) => write!(f, "{o} needs a value"),
            Self::Conflict => write!(f, "only one of --toggle, --new, --list can be given"),
            Self::CategoryWithoutNew => write!(f, "--category only goes with --new"),
        }
    }
}

impl std::error::Error for CliError {}

impl Command {
    /// The name sent to the running instance over D-Bus.
    pub fn action(&self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::New { .. } => "new",
            Self::List => "list",
        }
    }

    pub fn action_args(&self) -> Vec<String> {
        match self {
            Self::New {
                category: Some(name),
            } => vec![name.clone()],
            _ => Vec::new(),
        }
    }

    pub fn from_action(action: &str, args: &[String]) -> Option<Self> {
        match action {
            "toggle" => Some(Self::Toggle),
            "new" => Some(Self::New {
                category: args.first().filter(|s| !s.trim().is_empty()).cloned(),
            }),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Cli, CliError> {
    let mut command = None;
    let mut category = None;
    let mut args = args.into_iter();
    let mut set = |c: Command| {
        if command.replace(c).is_some() {
            Err(CliError::Conflict)
        } else {
            Ok(())
        }
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Cli::Help),
            "--toggle" => set(Command::Toggle)?,
            "--new" => set(Command::New { category: None })?,
            "--list" => set(Command::List)?,
            "--category" => {
                let value = args
                    .next()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or(CliError::MissingValue("--category"))?;
                category = Some(value);
            }
            _ => match arg.strip_prefix("--category=") {
                Some(value) if !value.is_empty() => category = Some(value.to_owned()),
                Some(_) => return Err(CliError::MissingValue("--category")),
                None => return Err(CliError::Unknown(arg)),
            },
        }
    }
    match (command, category) {
        (Some(Command::New { .. }), category) => Ok(Cli::Run(Some(Command::New { category }))),
        (_, Some(_)) => Err(CliError::CategoryWithoutNew),
        (command, None) => Ok(Cli::Run(command)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Cli, CliError> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn commands() {
        assert_eq!(p(&[]), Ok(Cli::Run(None)));
        assert_eq!(p(&["--toggle"]), Ok(Cli::Run(Some(Command::Toggle))));
        assert_eq!(p(&["--list"]), Ok(Cli::Run(Some(Command::List))));
        assert_eq!(
            p(&["--new"]),
            Ok(Cli::Run(Some(Command::New { category: None })))
        );
        assert_eq!(p(&["-h"]), Ok(Cli::Help));
        assert_eq!(p(&["--toggle", "--help"]), Ok(Cli::Help));
    }

    #[test]
    fn category() {
        let new = |c: &str| {
            Ok(Cli::Run(Some(Command::New {
                category: Some(c.into()),
            })))
        };
        assert_eq!(p(&["--new", "--category", "To-do's"]), new("To-do's"));
        assert_eq!(p(&["--category", "Self care", "--new"]), new("Self care"));
        assert_eq!(p(&["--new", "--category=Reminders"]), new("Reminders"));
    }

    #[test]
    fn bad_input() {
        assert_eq!(p(&["--bogus"]), Err(CliError::Unknown("--bogus".into())));
        assert_eq!(p(&["toggle"]), Err(CliError::Unknown("toggle".into())));
        assert_eq!(p(&["--toggle", "--list"]), Err(CliError::Conflict));
        assert_eq!(p(&["--new", "--new"]), Err(CliError::Conflict));
        assert_eq!(
            p(&["--new", "--category"]),
            Err(CliError::MissingValue("--category"))
        );
        assert_eq!(
            p(&["--new", "--category", "--list"]),
            Err(CliError::MissingValue("--category"))
        );
        assert_eq!(
            p(&["--new", "--category="]),
            Err(CliError::MissingValue("--category"))
        );
        assert_eq!(p(&["--category", "x"]), Err(CliError::CategoryWithoutNew));
        assert_eq!(
            p(&["--toggle", "--category", "x"]),
            Err(CliError::CategoryWithoutNew)
        );
    }

    #[test]
    fn action_round_trip() {
        for c in [
            Command::Toggle,
            Command::List,
            Command::New { category: None },
            Command::New {
                category: Some("Self care".into()),
            },
        ] {
            assert_eq!(Command::from_action(c.action(), &c.action_args()), Some(c));
        }
        assert_eq!(Command::from_action("explode", &[]), None);
    }
}
