//! Drawing the menu.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};

use crate::app::{App, Focus};
use crate::config::Kind;
use crate::runner::Status;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Dune night palette: dark brown paper, sand text, spice-orange accents (24-bit colour).
const PAPER: Color = Color::Rgb(21, 16, 11);
const INK: Color = Color::Rgb(234, 217, 184);
const BLUE: Color = Color::Rgb(106, 176, 224);
const GREEN: Color = Color::Rgb(143, 191, 106);
const YELLOW: Color = Color::Rgb(224, 168, 60);
const RED: Color = Color::Rgb(232, 98, 74);
const DIM: Color = Color::Rgb(154, 132, 96);
const TEXT: Color = INK;
const SPICE: Color = Color::Rgb(240, 138, 43);
const SEL_DIM: Color = Color::Rgb(42, 31, 21);
const SELECTED_INK: Color = Color::Rgb(21, 16, 11);
const LINE: Color = Color::Rgb(74, 56, 38);

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Service => "web app",
        Kind::Task => "task",
        Kind::Terminal => "terminal",
        Kind::Open => "open",
    }
}

fn kind_color(kind: Kind) -> Color {
    match kind {
        Kind::Service => BLUE,
        Kind::Task => YELLOW,
        Kind::Terminal => GREEN,
        Kind::Open => DIM,
    }
}

/// The coloured badge after an action's name.
fn status_span(status: Option<&Status>, tick: usize) -> Span<'static> {
    let spin = SPINNER[tick % SPINNER.len()];
    match status {
        None => Span::raw(""),
        Some(Status::Preparing(_)) => {
            Span::styled(format!("{spin} preparing..."), Style::new().fg(YELLOW))
        }
        Some(Status::Starting) => {
            Span::styled(format!("{spin} starting..."), Style::new().fg(YELLOW))
        }
        Some(Status::Running(Some(url))) => Span::styled(
            format!("● running  {url}"),
            Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
        ),
        Some(Status::Running(None)) => Span::styled(
            "● running",
            Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
        ),
        Some(Status::Done) => Span::styled("✔ done", Style::new().fg(GREEN)),
        Some(Status::Failed(m)) => Span::styled(format!("✖ {m}"), Style::new().fg(RED)),
        Some(Status::Stopped) => Span::styled("■ stopped", Style::new().fg(DIM)),
    }
}

/// One line of program output, coloured by what it looks like.
fn log_line(text: &str) -> Line<'static> {
    let lower = text.to_lowercase();
    let style = if text.starts_with("$ ") {
        Style::new().fg(BLUE).add_modifier(Modifier::BOLD)
    } else if lower.starts_with("error") || lower.contains("error:") || lower.contains("panicked") {
        Style::new().fg(RED)
    } else if lower.contains("warning") {
        Style::new().fg(YELLOW)
    } else if text.trim_start().starts_with("Compiling")
        || text.trim_start().starts_with("Running")
        || text.trim_start().starts_with("Downloading")
    {
        Style::new().fg(DIM)
    } else if text.trim_start().starts_with("Finished") || lower.contains("listening") {
        Style::new().fg(GREEN)
    } else {
        Style::new().fg(TEXT)
    };
    Line::styled(text.to_string(), style)
}

/// `key label   key label ...` with the keys highlighted. The text reads the same as plain hints.
fn hints() -> Line<'static> {
    let pairs = [
        ("↑↓", "move"),
        ("Enter", "run"),
        ("→", "actions"),
        ("1-9", "run"),
        ("s", "stop"),
        ("o", "open again"),
        ("e", "edit list"),
        ("q", "quit"),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            (*k).to_string(),
            Style::new().fg(SPICE).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {label}"), Style::new().fg(DIM)));
    }
    spans.push(Span::styled(
        "   (mouse: click, click again to run)",
        Style::new().fg(DIM),
    ));
    Line::from(spans)
}

pub fn render(f: &mut Frame, app: &mut App) {
    f.render_widget(
        Block::new().style(Style::new().bg(PAPER).fg(INK)),
        f.area(),
    );
    let [header, body, log_area, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(9),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)]).areas(body);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " SIETCH ",
                Style::new().bold().fg(SELECTED_INK).bg(INK),
            ),
            Span::styled("  pick a project, press Enter, it opens", Style::new().fg(DIM)),
        ])),
        header,
    );
    let running = app.runner.running_count();
    let total = app.config.projects.len();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{total} projects"), Style::new().fg(DIM)),
            Span::styled("  ·  ", Style::new().fg(DIM)),
            if running > 0 {
                Span::styled(
                    format!("● {running} running "),
                    Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("nothing running ", Style::new().fg(DIM))
            },
        ]))
        .alignment(Alignment::Right),
        header,
    );

    let focus_projects = app.focus == Focus::Projects;
    let selected_style = |focused: bool| {
        if focused {
            Style::new()
                .bg(SPICE)
                .fg(SELECTED_INK)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().bg(SEL_DIM).fg(INK)
        }
    };
    let border = |focused: bool| {
        if focused {
            Style::new().fg(SPICE)
        } else {
            Style::new().fg(LINE)
        }
    };
    let title_style = |focused: bool| {
        if focused {
            Style::new().fg(SPICE).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(DIM)
        }
    };

    // ---------------------------------------------------------------- projects
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(Span::styled(" Projects ", title_style(focus_projects)))
        .border_style(border(focus_projects));
    app.areas.projects = block.inner(left);
    let items: Vec<ListItem> = if app.config.projects.is_empty() {
        vec![ListItem::new(Line::from("no projects yet - press e".fg(YELLOW)))]
    } else {
        app.config
            .projects
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut spans = vec![
                    if app.project_active(i) {
                        Span::styled("● ", Style::new().fg(GREEN))
                    } else {
                        "  ".into()
                    },
                    Span::styled(p.name.clone(), Style::new().fg(TEXT)),
                ];
                if p.dir_path().is_none() {
                    spans.push(" ⚠".fg(YELLOW));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    };
    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(selected_style(focus_projects))
            .highlight_symbol("▶ "),
        left,
        &mut app.projects,
    );

    // ---------------------------------------------------------------- selected project
    let right_block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(Span::styled(" Actions ", title_style(!focus_projects)))
        .border_style(border(!focus_projects));
    let right_inner = right_block.inner(right);
    f.render_widget(right_block, right);
    let [info, actions_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(right_inner);
    app.areas.actions = actions_area;

    let sel = app.sel_project();
    if let Some(project) = app.config.projects.get(sel) {
        let folder_line = match project.dir_path() {
            Some(p) => Line::from(vec![
                "folder  ".fg(DIM),
                Span::styled(p.display().to_string(), Style::new().fg(TEXT)),
            ]),
            None => Line::from(
                "⚠ folder not set or not found - press e to edit the config".fg(YELLOW),
            ),
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(project.name.clone().fg(SPICE).bold()),
                Line::from(project.description.clone().fg(DIM)),
                folder_line,
            ]),
            info,
        );
        let name_w = project
            .actions
            .iter()
            .map(|a| a.name.chars().count())
            .max()
            .unwrap_or(0)
            .max(24)
            + 2;
        let items: Vec<ListItem> = project
            .actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let status = app.status_of((sel, i));
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", i + 1), Style::new().fg(DIM)),
                    Span::styled(format!("{:<name_w$}", a.name), Style::new().fg(TEXT)),
                    Span::styled(
                        format!("{:<9}", kind_label(a.kind)),
                        Style::new().fg(kind_color(a.kind)),
                    ),
                    status_span(status, app.tick),
                ]))
            })
            .collect();
        f.render_stateful_widget(
            List::new(items)
                .highlight_style(selected_style(!focus_projects))
                .highlight_symbol("▶ "),
            actions_area,
            &mut app.actions,
        );
    }

    // ---------------------------------------------------------------- output
    let key = (sel, if focus_projects { 0 } else { app.sel_action() });
    let title = app
        .config
        .projects
        .get(key.0)
        .and_then(|p| p.actions.get(key.1))
        .map(|a| format!(" Output - {} ", a.name))
        .unwrap_or_else(|| " Output ".into());
    let log_block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(Span::styled(title, Style::new().fg(DIM)))
        .border_style(Style::new().fg(LINE));
    let inner = log_block.inner(log_area);
    let lines: Vec<Line> = match app.states.get(&key).filter(|s| !s.log.is_empty()) {
        Some(st) => {
            let skip = st.log.len().saturating_sub(inner.height as usize);
            st.log.iter().skip(skip).map(|l| log_line(l)).collect()
        }
        None => vec![Line::from(
            "Nothing has run yet. Press Enter to start the first action.".fg(DIM),
        )],
    };
    f.render_widget(Paragraph::new(lines).block(log_block), log_area);

    // ---------------------------------------------------------------- footer
    let footer_line = match &app.flash {
        Some((text, at)) if at.elapsed() < Duration::from_secs(5) => {
            Line::from(format!(" {text}").fg(YELLOW))
        }
        _ => hints(),
    };
    f.render_widget(Paragraph::new(footer_line), footer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, Config, Project};
    use crate::runner::{Msg, Runner};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn make_app() -> App {
        let dir = std::env::temp_dir().display().to_string();
        let act = |n: &str, k: Kind| Action {
            name: n.into(),
            kind: k,
            run: "x".into(),
            cwd: None,
            url: None,
            before: vec![],
            env: Default::default(),
            wait_secs: 5,
            stop_services: false,
        };
        let cfg = Config {
            projects: vec![
                Project {
                    name: "Alpha App".into(),
                    description: "does alpha things".into(),
                    dir,
                    actions: vec![
                        act("Open the web app", Kind::Service),
                        act("Run the tests", Kind::Task),
                    ],
                },
                Project {
                    name: "Beta Tool".into(),
                    description: "".into(),
                    dir: String::new(),
                    actions: vec![],
                },
            ],
        };
        let (tx, _rx) = mpsc::channel();
        App::new(cfg, PathBuf::from("projects.toml"), Runner::new(tx, true))
    }

    fn screen(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_screen_shows_projects_actions_and_hints() {
        let mut app = make_app();
        let s = screen(&mut app, 110, 32);
        for want in [
            "SIETCH",
            "Projects",
            "Alpha App",
            "Beta Tool",
            "does alpha things",
            "1 Open the web app",
            "2 Run the tests",
            "↑↓ move",
            "Enter run",
            "q quit",
        ] {
            assert!(s.contains(want), "missing {want:?} in:\n{s}");
        }
        assert!(
            s.contains("Beta Tool ⚠"),
            "a project without a folder is flagged"
        );
        assert!(s.contains("Nothing has run yet"));
    }

    #[test]
    fn selecting_another_project_changes_the_right_panel() {
        let mut app = make_app();
        app.projects.select(Some(1));
        let s = screen(&mut app, 110, 32);
        assert!(s.contains("folder not set or not found"), "{s}");
        assert!(!s.contains("Open the web app"));
    }

    #[test]
    fn statuses_and_output_appear() {
        let mut app = make_app();
        app.on_msg(Msg::Status(
            (0, 0),
            Status::Running(Some("http://127.0.0.1:8501".into())),
        ));
        app.on_msg(Msg::Status((0, 1), Status::Failed("exit code 101".into())));
        app.on_msg(Msg::Log((0, 0), "server listening".into()));
        let s = screen(&mut app, 120, 34);
        assert!(s.contains("● running  http://127.0.0.1:8501"), "{s}");
        assert!(s.contains("✖ exit code 101"));
        assert!(s.contains("server listening"));
        assert!(
            s.contains("● Alpha App"),
            "the project list shows a service is up"
        );
    }

    #[test]
    fn a_flash_message_replaces_the_hints() {
        let mut app = make_app();
        app.flash("something happened");
        let s = screen(&mut app, 110, 32);
        assert!(s.contains("something happened") && !s.contains("↑↓ move"));
    }

    #[test]
    fn a_tiny_terminal_and_an_empty_config_do_not_crash() {
        let mut app = make_app();
        for (w, h) in [(20, 8), (1, 1), (60, 12), (200, 60)] {
            screen(&mut app, w, h);
        }
        app.config.projects.clear();
        let s = screen(&mut app, 100, 30);
        assert!(s.contains("no projects yet"));
    }

    #[test]
    fn the_drawn_areas_match_the_lists_for_mouse_clicks() {
        let mut app = make_app();
        screen(&mut app, 110, 32);
        assert!(app.areas.projects.width > 0 && app.areas.actions.width > 0);
        assert!(app.areas.actions.x > app.areas.projects.x);
    }
}
