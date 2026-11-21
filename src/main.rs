//! Sietch - a terminal menu for opening your projects.
//!
//! Pick a project with the arrow keys (or the mouse), press Enter, and it starts and opens in
//! your browser. See `config.rs` for the `projects.toml` format.

mod app;
mod config;
mod handoff;
mod runner;
mod ui;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ratatui::crossterm::execute;

use app::{App, Effect};
use config::{Config, default_config_path};
use runner::{Key, Runner, shell, step_needed};

#[derive(Parser)]
#[command(
    name = "sietch",
    version,
    about = "A terminal menu to open and test your projects"
)]
struct Cli {
    /// Use this project list instead of the default file.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Never open a browser or file (the log says what would have been opened). For testing.
    #[arg(long)]
    no_browser: bool,
    /// Print where the project list is stored and whether each project folder was found, then exit.
    #[arg(long)]
    check: bool,
    /// Replace the project list with the current defaults (your old one is saved as projects.toml.bak).
    #[arg(long)]
    reset_config: bool,
    /// Do not `git pull` the handoff notes or open the briefing at start.
    #[arg(long)]
    no_briefing: bool,
    /// Print the path of the project list and exit.
    #[arg(long)]
    print_config_path: bool,
}

fn enter_terminal() -> Result<DefaultTerminal> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    Ok(terminal)
}

fn leave_terminal() {
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
}

/// Run an interactive action (a chat) with the real terminal, then come back to the menu.
fn run_in_terminal(app: &App, key: Key) {
    leave_terminal();
    let project = &app.config.projects[key.0];
    let action = &project.actions[key.1];
    let Some(project_dir) = project.dir_path() else {
        return;
    };
    let dir = runner::action_dir(&project_dir, action);
    println!("\n── {} · {} ──\n", project.name, action.name);

    let mut ok = true;
    for step in &action.before {
        if let Some(cmd) = step_needed(step, &project_dir) {
            println!("$ {cmd}");
            match shell(&cmd, &dir, &action.env).status() {
                Ok(s) if s.success() => {}
                Ok(s) => {
                    println!(
                        "\nThat step failed (exit {}). Not starting {}.",
                        s.code().unwrap_or(-1),
                        action.name
                    );
                    ok = false;
                    break;
                }
                Err(e) => {
                    println!("\nCould not run `{cmd}`: {e}");
                    ok = false;
                    break;
                }
            }
        }
    }
    if ok {
        println!("$ {}\n", action.run);
        match shell(&action.run, &dir, &action.env).status() {
            Ok(s) => println!("\n[finished, exit code {}]", s.code().unwrap_or(-1)),
            Err(e) => println!("\nCould not start: {e}"),
        }
    }
    print!("\nPress Enter to go back to Sietch... ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}

fn load_config(path: &Path) -> Result<Config> {
    config::load(path)
}

/// Read the handoff notes (after a quick `git pull` of the notes repo) into the briefing.
fn load_briefing(app: &mut App, pull: bool, show: bool) {
    let Some(file) = app.config.handoff_path() else {
        return;
    };
    if pull && let Some(dir) = file.parent().filter(|d| d.join(".git").exists()) {
        println!("Syncing handoff notes...");
        if let Err(e) = handoff::pull(dir, Duration::from_secs(6)) {
            app.flash(format!("handoff notes not updated ({e}); showing the saved copy"));
        }
    }
    let entries = handoff::latest_per_project(&handoff::load(&file));
    app.set_briefing(entries, show);
}

fn run_menu(mut app: App, rx: mpsc::Receiver<runner::Msg>) -> Result<()> {
    let mut terminal = enter_terminal()?;
    let result = (|| -> Result<()> {
        while !app.quit {
            while let Ok(msg) = rx.try_recv() {
                app.on_msg(msg);
            }
            terminal.draw(|f| ui::render(f, &mut app))?;
            let mut effect = Effect::None;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) => effect = app.on_key(k),
                    Event::Mouse(m) => effect = app.on_mouse(m),
                    _ => {}
                }
            }
            app.tick += 1;
            match effect {
                Effect::None => {}
                Effect::Terminal(key) => {
                    run_in_terminal(&app, key);
                    terminal = enter_terminal()?;
                }
                Effect::EditConfig => match open::that_detached(&app.config_path) {
                    Ok(()) => {
                        app.flash("opened the project list in your editor - save it, then press r")
                    }
                    Err(e) => {
                        app.flash(format!("could not open {}: {e}", app.config_path.display()))
                    }
                },
                Effect::ReloadConfig => match load_config(&app.config_path) {
                    Ok(cfg) => {
                        app.config = cfg;
                        load_briefing(&mut app, false, false);
                        app.projects.select(Some(0));
                        app.actions.select(Some(0));
                        app.flash("project list reloaded");
                    }
                    Err(e) => app.flash(format!("{e:#}")),
                },
            }
        }
        Ok(())
    })();
    leave_terminal();
    // stop every service we started, so nothing is left running in the background
    let running = app.runner.running_count();
    if running > 0 {
        println!("Stopping {running} running service(s)...");
        app.runner.stop_all();
    }
    result
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.config.clone().unwrap_or_else(default_config_path);
    if cli.print_config_path {
        println!("{}", path.display());
        return Ok(());
    }
    if cli.reset_config {
        match config::reset_config(&path)? {
            Some(b) => println!("Old project list saved as {}", b.display()),
            None => println!("No old project list to save."),
        }
        println!("Fresh project list written to {}", path.display());
    }
    let created =
        config::ensure_config(&path).with_context(|| format!("creating {}", path.display()))?;
    let cfg = load_config(&path)?;

    if cli.check {
        println!(
            "Project list: {}{}",
            path.display(),
            if created { "  (just created)" } else { "" }
        );
        let mut missing = 0;
        for p in &cfg.projects {
            match p.dir_path() {
                Some(d) => println!(
                    "  ✔ {:<14} {}   ({} actions)",
                    p.name,
                    d.display(),
                    p.actions.len()
                ),
                None => {
                    missing += 1;
                    println!(
                        "  ✖ {:<14} folder not set or not found: {:?}",
                        p.name, p.dir
                    );
                }
            }
        }
        match cfg.handoff_path() {
            Some(f) if f.exists() => println!(
                "  ✔ handoff notes  {}   ({} entries)",
                f.display(),
                handoff::load(&f).len()
            ),
            Some(f) => println!("  ✖ handoff notes  file not found: {}", f.display()),
            None => println!("  - handoff notes  not set up (see [handoff] in the project list)"),
        }
        if missing > 0 {
            println!("\nEdit the file above and set `dir` for the projects marked ✖.");
        }
        return Ok(());
    }

    // Ctrl+C belongs to whatever is in front: in the menu it is a key press, and while an
    // interactive action runs it should stop that action, not Sietch.
    let _ = ctrlc::set_handler(|| {});

    let (tx, rx) = mpsc::channel();
    let mut app = App::new(cfg, path.clone(), Runner::new(tx, cli.no_browser));
    if created {
        app.flash(format!(
            "first run: created {} - set each project's folder with e",
            path.display()
        ));
    }
    if !cli.no_briefing {
        load_briefing(&mut app, true, true);
    }
    run_menu(app, rx)
}
