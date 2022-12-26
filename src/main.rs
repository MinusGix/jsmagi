use std::path::PathBuf;

use jsmagi::{transform, MagiConfig};
use swc_common::{Globals, GLOBALS};

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "jsmagi")]
#[command(about = "A JavaScript Unminifier", long_about = None)]
struct Cli {
    #[arg(short, long)]
    verbose: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    // TODO: Support multiple files at once, also allowing them to share globals
    #[command(
        about = "Applies transformations to a JavaScript file",
        arg_required_else_help = true
    )]
    Transform {
        // TODO: Let the user request output to stdout
        file: PathBuf,
        #[arg(long, short)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = true)]
        typescript: bool,
    },
    // TODO: command to generate a typescript config file which matches our loose
    // application. Obviously, we can't generate good types in many cases, so allowing implicit-any
    // is a must. Etc.
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Transform {
            file,
            output,
            typescript,
        } => {
            let conf = MagiConfig { typescript };
            let output = output.unwrap_or_else(|| {
                let mut path = file
                    .parent()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| PathBuf::from("./"));
                path.push("output");
                if typescript {
                    path.set_extension("ts");
                } else {
                    path.set_extension("js");
                }

                path
            });

            let globals = Globals::new();
            GLOBALS.set(&globals, || {
                let code = transform(&file, conf);
                std::fs::write(output, code).unwrap();
            })
        }
    }
}
