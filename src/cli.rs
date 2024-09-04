use std::path::PathBuf;

use clap::{Parser, Subcommand, arg};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Command {
    #[clap(subcommand)]
    pub task: Task,
    #[arg(short, long)]
    pub verbose: bool,
}

impl Command {
    // to avoid importing clap::Parser in main file
    pub fn clap_parse() -> Self {
        Command::parse()
    }
}

#[derive(Debug, Subcommand)]
pub enum Task {
    Create {
        path: PathBuf,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long = "command")]
        commands: Vec<String>,
        #[arg(short, long)]
        ignore: Vec<String>,
        #[arg(short='r', long="definition")]
        definition_file: Option<PathBuf>,
        #[arg(short, long)]
        force: bool,
    },
    Expand {
        name: String,
        #[arg(short, long)]
        path: Option<PathBuf>,
        #[arg(short, long = "env")]
        envs: Vec<String>,  // --env key=value
        #[arg(short = 'a', long = "as")]
        create_as: Option<String>,
        #[arg(short, long)]
        no_exec: bool,
    },
    List {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        commands: bool,
    },
    Delete {
        name: String,
    },
    Edit {
        name: String,
    },
}
