//! The project list. It lives in a plain text file you can edit (`projects.toml`).
//!
//! ```toml
//! [[projects]]
//! name = "My App"
//! description = "What it is"
//! dir = "~/code/my-app"            # where the project lives
//!
//!   [[projects.actions]]
//!   name = "Open the app"
//!   kind = "service"               # keeps running; opens `url` in the browser when it answers
//!   run  = "cargo run --release"
//!   url  = "http://127.0.0.1:8080"
//! ```
//!
//! Action `kind`:
//! * `service`  - a long-running command (a web server). Started in the background, the browser
//!   opens when `url` answers, `s` stops it.
//! * `task`     - a command that finishes (tests, a build); its output streams into the log pane.
//! * `terminal` - an interactive program (a chat): the menu steps aside and gives you the terminal.
//! * `open`     - open a file, folder or web address with your system's default app.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Project folder. `~` means your home folder. Empty = not set yet.
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Task,
    Service,
    Terminal,
    Open,
}

/// A command to run before the main one.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum Step {
    /// Always run.
    Cmd(String),
    /// Run only if the file or folder `unless` does not exist yet (relative to the project folder).
    When { unless: String, run: String },
}

fn default_wait() -> u64 {
    600
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Action {
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    /// The command (or, for `kind = "open"`, the file/folder/address to open).
    pub run: String,
    /// Subfolder of the project to run in.
    #[serde(default)]
    pub cwd: Option<String>,
    /// For services: the address to wait for, then open in the browser.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub before: Vec<Step>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// For services: how long to wait for `url` (a first build can take minutes).
    #[serde(default = "default_wait")]
    pub wait_secs: u64,
    /// Stop this project's running apps first (used by "Update from GitHub").
    #[serde(default)]
    pub stop_services: bool,
}

/// `~` and `~/x` -> the home folder.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest.trim_start_matches(['/', '\\']));
    }
    PathBuf::from(path)
}

impl Project {
    /// The project folder if it is set and exists.
    pub fn dir_path(&self) -> Option<PathBuf> {
        if self.dir.trim().is_empty() {
            return None;
        }
        let p = expand_home(self.dir.trim());
        p.is_dir().then_some(p)
    }
}

/// Where the config file lives: `--config`, else the platform's config folder.
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("launchpad")
        .join("projects.toml")
}

pub fn load(path: &Path) -> Result<Config> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| {
        format!(
            "{} is not valid: check the quotes and brackets",
            path.display()
        )
    })
}

/// Look for a folder called `name` in the usual places (where people keep projects).
pub fn discover_dir(name: &str, extra_roots: &[PathBuf]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = extra_roots.to_vec();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        if let Some(parent) = cwd.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    if let Some(home) = dirs::home_dir() {
        for sub in [
            "",
            "projects",
            "Projects",
            "code",
            "Code",
            "dev",
            "src",
            "repos",
            "Documents",
            "Desktop",
        ] {
            roots.push(if sub.is_empty() {
                home.clone()
            } else {
                home.join(sub)
            });
        }
    }
    for drive in ["D:\\Gilberto", "C:\\Gilberto", "E:\\Gilberto"] {
        roots.push(PathBuf::from(drive));
    }
    roots.into_iter().map(|r| r.join(name)).find(|p| p.is_dir())
}

/// A TOML literal string (single quotes, no escapes), so Windows paths need no doubling.
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', ""))
}

/// The starting config: the three Rust remasters, with folders found automatically where possible.
pub fn default_config_text(discover: &dyn Fn(&str) -> Option<PathBuf>) -> String {
    let dir = |name: &str| {
        discover(name)
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    };
    let cargo = "cargo run --release --manifest-path rust/Cargo.toml --";
    format!(
        r#"# Launchpad - your projects. Edit this file; press r in Launchpad to reload it.
# Folders left empty (dir = '') were not found: set them to where the project lives.
# Kinds: service (keeps running, opens `url`), task (runs and finishes), terminal (interactive), open.

[[projects]]
name = "tbjp-oracle"
description = "Forum archive search (Rust remaster)"
dir = {tbjp}

  [[projects.actions]]
  name = "Open the web app"
  kind = "service"
  before = [{{ unless = "data/index/docs.jsonl", run = "{cargo} ingest" }}]
  run = "{cargo} serve"
  url = "http://127.0.0.1:8501"

  [[projects.actions]]
  name = "Run the tests"
  run = "cargo test --release --manifest-path rust/Cargo.toml"

  [[projects.actions]]
  name = "Rebuild the search index"
  run = "{cargo} ingest"

  [[projects.actions]]
  name = "Update from GitHub (git pull)"
  run = "git pull --ff-only"
  stop_services = true

  [[projects.actions]]
  name = "Open the project folder"
  kind = "open"
  run = "."

  [[projects.actions]]
  name = "Read the learning guide"
  kind = "open"
  run = "docs/RUST_LEARNING.md"

[[projects]]
name = "logbook"
description = "Workout log (Rust / WebAssembly remaster)"
dir = {logbook}

  [[projects.actions]]
  name = "Run the app (live reload)"
  kind = "service"
  cwd = "rust/web"
  before = [{{ unless = "{trunk}", run = "cargo install trunk --locked" }}, "rustup target add wasm32-unknown-unknown"]
  run = "trunk serve"
  url = "http://127.0.0.1:8080"

  [[projects.actions]]
  name = "Run the tests"
  run = "cargo test --workspace --manifest-path rust/Cargo.toml"

  [[projects.actions]]
  name = "Update from GitHub (git pull)"
  run = "git pull --ff-only"
  stop_services = true

  [[projects.actions]]
  name = "Open the project folder"
  kind = "open"
  run = "."

  [[projects.actions]]
  name = "Read the learning guide"
  kind = "open"
  run = "docs/RUST_LEARNING.md"

[[projects]]
name = "rhcp-chatbot"
description = "Trivia chatbot (Rust remaster)"
dir = {rhcp}

  [[projects.actions]]
  name = "Chat in the terminal"
  kind = "terminal"
  before = [{{ unless = "~/nltk_data/tokenizers/punkt_tab/english/abbrev_types.txt", run = "{cargo} setup-nltk" }}]
  run = "{cargo} chat"

  [[projects.actions]]
  name = "Chat (answers more often)"
  kind = "terminal"
  before = [{{ unless = "~/nltk_data/tokenizers/punkt_tab/english/abbrev_types.txt", run = "{cargo} setup-nltk" }}]
  run = "{cargo} chat"
  env = {{ RHCP_CONFIDENCE_THRESHOLD = "0.05" }}

  [[projects.actions]]
  name = "Start the web API"
  kind = "service"
  before = [{{ unless = "~/nltk_data/tokenizers/punkt_tab/english/abbrev_types.txt", run = "{cargo} setup-nltk" }}]
  run = "{cargo} serve --host 127.0.0.1"
  url = "http://127.0.0.1:3000/healthz"
  env = {{ DATABASE_URL = "sqlite:///./data/dev_auth.db" }}

  [[projects.actions]]
  name = "Run the tests"
  run = "cargo test --release --manifest-path rust/Cargo.toml"

  [[projects.actions]]
  name = "Update from GitHub (git pull)"
  run = "git pull --ff-only"
  stop_services = true

  [[projects.actions]]
  name = "Open the project folder"
  kind = "open"
  run = "."

  [[projects.actions]]
  name = "Read the learning guide"
  kind = "open"
  run = "docs/RUST_LEARNING.md"

[[projects]]
name = "gilbertordx.github.io"
description = "Portfolio and log (Rust static site)"
dir = {portfolio}

  [[projects.actions]]
  name = "Open the portfolio"
  kind = "service"
  run = "cargo run --release -- serve"
  url = "http://127.0.0.1:4321"

  [[projects.actions]]
  name = "Build the site (./dist)"
  run = "cargo run --release"

  [[projects.actions]]
  name = "Update from GitHub (git pull)"
  run = "git pull --ff-only"
  stop_services = true

  [[projects.actions]]
  name = "Open the project folder"
  kind = "open"
  run = "."

  [[projects.actions]]
  name = "Open the live site"
  kind = "open"
  run = "https://gilbertordx.github.io"

[[projects]]
name = "launchpad"
description = "This menu (Rust terminal app)"
dir = {launchpad}

  [[projects.actions]]
  name = "Update Launchpad + desktop icon"
  run = "{update}"

  [[projects.actions]]
  name = "Run the tests"
  run = "cargo test --release"

  [[projects.actions]]
  name = "Edit the project list"
  kind = "open"
  run = {config}

  [[projects.actions]]
  name = "Open the project folder"
  kind = "open"
  run = "."
"#,
        tbjp = lit(&dir("tbjp-oracle")),
        logbook = lit(&dir("logbook")),
        rhcp = lit(&dir("rhcp-chatbot")),
        portfolio = lit(&dir("gilbertordx.github.io")),
        launchpad = lit(&dir("launchpad")),
        trunk = if cfg!(windows) { "~/.cargo/bin/trunk.exe" } else { "~/.cargo/bin/trunk" },
        update = if cfg!(windows) {
            "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/update.ps1"
        } else {
            "bash scripts/update.sh"
        },
        config = lit(&default_config_path().display().to_string()),
        cargo = cargo,
    )
}

/// Back up the current project list (if any) and write a fresh default one.
pub fn reset_config(path: &Path) -> Result<Option<PathBuf>> {
    let backup = if path.exists() {
        let b = path.with_extension("toml.bak");
        std::fs::copy(path, &b)?;
        std::fs::remove_file(path)?;
        Some(b)
    } else {
        None
    };
    ensure_config(path)?;
    Ok(backup)
}

/// Create the config file the first time (never overwrites an existing one).
pub fn ensure_config(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, default_config_text(&|n| discover_dir(n, &[])))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_is_valid_and_complete() {
        let text = default_config_text(&|_| None);
        let cfg: Config = toml::from_str(&text).expect("default config parses");
        let names: Vec<&str> = cfg.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["tbjp-oracle", "logbook", "rhcp-chatbot", "gilbertordx.github.io", "launchpad"]);
        assert!(
            cfg.projects.iter().all(|p| p.dir.is_empty()),
            "nothing was discovered, so dirs are empty"
        );
        let web = &cfg.projects[0].actions[0];
        assert_eq!(web.kind, Kind::Service);
        assert_eq!(web.url.as_deref(), Some("http://127.0.0.1:8501"));
        assert_eq!(web.wait_secs, 600);
        assert!(
            matches!(&web.before[0], Step::When { unless, .. } if unless == "data/index/docs.jsonl")
        );
        assert_eq!(
            cfg.projects[2].actions[1]
                .env
                .get("RHCP_CONFIDENCE_THRESHOLD")
                .map(String::as_str),
            Some("0.05")
        );
        assert!(cfg.projects.iter().all(|p| !p.actions.is_empty()));
    }

    #[test]
    fn folders_found_by_discovery_are_written_in_and_windows_paths_survive() {
        let text = default_config_text(&|n| {
            (n == "logbook").then(|| PathBuf::from(r"D:\Gilberto\logbook"))
        });
        let cfg: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg.projects[1].dir, r"D:\Gilberto\logbook");
        assert_eq!(cfg.projects[0].dir, "");
    }

    #[test]
    fn minimal_entries_get_sensible_defaults() {
        let cfg: Config = toml::from_str(
            "[[projects]]\nname='x'\n[[projects.actions]]\nname='a'\nrun='echo hi'\n",
        )
        .unwrap();
        let a = &cfg.projects[0].actions[0];
        assert_eq!(
            (a.kind, a.cwd.clone(), a.url.clone(), a.before.len()),
            (Kind::Task, None, None, 0)
        );
    }

    #[test]
    fn home_is_expanded() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/a/b"), home.join("a/b"));
        assert_eq!(expand_home("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_home("rel"), PathBuf::from("rel"));
    }

    #[test]
    fn discovery_finds_a_folder_in_an_extra_root() {
        let root = std::env::temp_dir().join(format!("launchpad-disc-{}", std::process::id()));
        std::fs::create_dir_all(root.join("my-project")).unwrap();
        assert_eq!(
            discover_dir("my-project", std::slice::from_ref(&root)),
            Some(root.join("my-project"))
        );
        assert_eq!(
            discover_dir("definitely-not-here-xyz", std::slice::from_ref(&root)),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_bad_config_gives_a_helpful_error() {
        let p = std::env::temp_dir().join(format!("launchpad-bad-{}.toml", std::process::id()));
        std::fs::write(&p, "[[projects]\nname=").unwrap();
        let err = load(&p).unwrap_err().to_string();
        assert!(err.contains("not valid"), "{err}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn ensure_config_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("launchpad-ensure-{}", std::process::id()));
        let p = dir.join("projects.toml");
        assert!(ensure_config(&p).unwrap());
        std::fs::write(&p, "# mine\n").unwrap();
        assert!(!ensure_config(&p).unwrap());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "# mine\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
