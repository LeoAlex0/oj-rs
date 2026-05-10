use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Args as ClapArgs, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use oj_pack::{pack_project, PackOptions, PackageInfo};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("oj-pack: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().into_action() {
        Action::Pack(pack) => pack_binary(pack),
        Action::List => list_binaries(),
        Action::Completions(shell) => {
            let mut command = Cli::command();
            generate(shell, &mut command, "oj-pack", &mut io::stdout());
            Ok(())
        }
    }
}

fn pack_binary(pack: PackArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output = pack_project(
        &env::current_dir()?,
        &pack.bin,
        PackOptions {
            check: !pack.flags.no_check,
            minify: pack.flags.minify,
            max_bytes: pack.flags.max_bytes,
            warn_bytes: 65_536,
        },
    )?;

    io::stdout().write_all(output.as_bytes())?;
    Ok(())
}

fn list_binaries() -> Result<(), Box<dyn std::error::Error>> {
    let project_root = env::current_dir()?;
    let package = PackageInfo::load(&project_root)?;

    for bin in package.bin_names() {
        eprintln!("{bin}");
    }
    Ok(())
}

#[derive(Debug, Parser)]
#[command(
    name = "oj-pack",
    version,
    about = "Bundle one Rust OJ binary into a single submission file",
    arg_required_else_help = true,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(value_name = "BIN", required_unless_present = "list")]
    bin: Option<String>,

    #[arg(long, conflicts_with_all = ["bin", "no_check", "minify", "max_bytes"])]
    list: bool,

    #[command(flatten)]
    flags: PackFlags,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Bundle one binary target")]
    Pack(PackArgs),
    #[command(about = "List packable binary targets")]
    List,
    #[command(about = "Generate shell completion script")]
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Clone, ClapArgs)]
struct PackArgs {
    #[arg(value_name = "BIN")]
    bin: String,

    #[command(flatten)]
    flags: PackFlags,
}

#[derive(Debug, Clone, Default, ClapArgs)]
struct PackFlags {
    #[arg(long, help = "Skip rustc validation of generated source")]
    no_check: bool,

    #[arg(long, help = "Remove unnecessary whitespace and blank lines")]
    minify: bool,

    #[arg(
        long,
        value_name = "N",
        help = "Fail if generated source exceeds N bytes"
    )]
    max_bytes: Option<usize>,
}

#[derive(Debug)]
enum Action {
    Pack(PackArgs),
    List,
    Completions(Shell),
}

impl Cli {
    fn into_action(self) -> Action {
        match self.command {
            Some(Command::Pack(pack)) => Action::Pack(pack),
            Some(Command::List) => Action::List,
            Some(Command::Completions { shell }) => Action::Completions(shell),
            None if self.list => Action::List,
            None => Action::Pack(PackArgs {
                bin: self.bin.expect("clap enforces BIN unless --list is set"),
                flags: self.flags,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("oj-pack").chain(args.iter().copied()))
            .expect("args parse")
    }

    #[test]
    fn parses_legacy_pack_args() {
        let cli = parse(&[
            "--no-check",
            "--minify",
            "--max-bytes",
            "65536",
            "luogu_p3372",
        ]);
        let Action::Pack(pack) = cli.into_action() else {
            panic!("expected pack action");
        };

        assert_eq!(pack.bin, "luogu_p3372");
        assert!(pack.flags.no_check);
        assert!(pack.flags.minify);
        assert_eq!(pack.flags.max_bytes, Some(65_536));
    }

    #[test]
    fn parses_pack_subcommand() {
        let cli = parse(&["pack", "--minify", "luogu_p3372"]);
        let Action::Pack(pack) = cli.into_action() else {
            panic!("expected pack action");
        };

        assert_eq!(pack.bin, "luogu_p3372");
        assert!(pack.flags.minify);
    }

    #[test]
    fn defaults_to_formatted_output() {
        let cli = parse(&["luogu_p3372"]);
        let Action::Pack(pack) = cli.into_action() else {
            panic!("expected pack action");
        };

        assert_eq!(pack.bin, "luogu_p3372");
        assert!(!pack.flags.minify);
    }

    #[test]
    fn parses_legacy_list_args() {
        let cli = parse(&["--list"]);

        assert!(matches!(cli.into_action(), Action::List));
    }

    #[test]
    fn parses_list_subcommand() {
        let cli = parse(&["list"]);

        assert!(matches!(cli.into_action(), Action::List));
    }

    #[test]
    fn parses_completion_subcommand() {
        let cli = parse(&["completions", "zsh"]);

        assert!(matches!(cli.into_action(), Action::Completions(Shell::Zsh)));
    }

    #[test]
    fn rejects_unknown_flag() {
        let err =
            Cli::try_parse_from(["oj-pack", "--wat", "luogu_p3372"]).expect_err("unknown flag");

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_duplicate_binary_args() {
        let err = Cli::try_parse_from(["oj-pack", "a", "b"]).expect_err("duplicate binary");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }
}
