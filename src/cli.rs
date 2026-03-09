use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "xray",
    about = "Context-aware codebase explorer for agentic LLMs",
    version = env!("GIT_DESCRIBE"),
    after_help = "Examples:\n  xray                    # skeleton of current directory\n  xray skeleton src/      # skeleton of src/ only\n  xray outline -k source  # outline of source files only\n  xray -b 50              # skeleton with 50-line budget"
)]
pub struct Cli {
    /// Target directory (default: current directory)
    #[arg(global = true, default_value = ".")]
    pub path: PathBuf,

    /// Subcommand (defaults to skeleton)
    #[command(subcommand)]
    pub layer: Option<Layer>,

    /// Filter by file kind (repeatable)
    #[arg(short, long = "kind", global = true)]
    pub kinds: Vec<String>,

    /// Filter by language (repeatable, auto-detected if omitted)
    #[arg(short, long = "lang", global = true)]
    pub langs: Vec<String>,

    /// Scope to files/dirs matching glob (repeatable)
    #[arg(long, global = true)]
    pub pattern: Vec<String>,

    /// Exclude files matching glob (repeatable)
    #[arg(long, global = true)]
    pub exclude: Vec<String>,

    /// Maximum output lines (0 = unlimited)
    #[arg(short, long, global = true)]
    pub budget: Option<usize>,

    /// Output format: json, yaml, auto
    #[arg(short, long, global = true)]
    pub format: Option<String>,

    /// Config file override
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Layer {
    /// Smart directory tree — key files shown, noise collapsed
    Skeleton {
        /// Target directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Function/class/struct signatures with line numbers
    Outline {
        /// Target directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Public symbols only
        #[arg(long)]
        public: bool,

        /// Private symbols only
        #[arg(long)]
        private: bool,
    },
}

impl Cli {
    /// Get the effective target path (subcommand path takes priority if not default)
    pub fn effective_path(&self) -> &Path {
        use std::path::Path;
        match &self.layer {
            Some(Layer::Skeleton { path }) if path != Path::new(".") => path,
            Some(Layer::Outline { path, .. }) if path != Path::new(".") => path,
            _ => &self.path,
        }
    }
}

use std::path::Path;
