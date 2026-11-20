//! Drawing the menu.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::app::{App, Focus};
use crate::config::Kind;
use crate::runner::Status;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Service => "web app",
        Kind::Task => "task",
        Kind::Terminal => "terminal",
        Kind::Open => "open",
    }
}

/// The coloured badge after an action's name.
fn status_span(status: Option<&Status>, tick: usize) -> Span<'static> {
    let spin = SPINNER[tick % SPINNER.len()];
    match status {
        None => Span::raw(""),
        Some(Status::Preparing(_)) => Span::styled(
            format!("{spin} preparing..."),
            Style::new().fg(Color::Yellow),
        ),
        Some(Status::Starting) => Span::styled(
            format!("{spin} starting..."),
            Style::new().fg(Color::Yellow),
        ),
        Some(Status::Running(Some(url))) => {
            Span::styled(format!("● running  {url}"), Style::new().fg(Color::Green))
        }
        Some(Status::Running(None)) => Span::styled("● running", Style::new().fg(Color::Green)),
        Some(Status::Done) => Span::styled("✔ done", Style::new().fg(Color::Green)),
        Some(Status::Failed(m)) => Span::styled(format!("✖ {m}"), Style::new().fg(Color::Red)),
        Some(Status::Stopped) => Span::styled("■ stopped", Style::new().fg(Color::DarkGray)),
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
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
                " LAUNCHPAD ",
                Style::new().bold().fg(Color::White).bg(Color::Blue),
            ),
            Span::styled(
                "  pick a project, press Enter, it opens",
                Style::new().fg(Color::Gray),
            ),
        ])),
        header,
    );

    let focus_projects = app.focus == Focus::Projects;
    let selected_style = |focused: bool| {
        if focused {
            Style::new()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().bg(Color::DarkGray).fg(Color::White)
        }
    };
    let border = |focused: bool| {
        if focused {
            Style::new().fg(Color::Cyan)
        } else {
            Style::new().fg(Color::DarkGray)
        }
    };

    // ---------------------------------------------------------------- projects
    let block = Block::bordered()
        .title(" Projects ")
        .border_style(border(focus_projects));
    app.areas.projects = block.inner(left);
    let items: Vec<ListItem> = if app.config.projects.is_empty() {
        vec![ListItem::new(Line::from(
            "no projects yet - press e".fg(Color::Yellow),
        ))]
    } else {
        app.config
            .projects
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut spans = vec![
                    if app.project_active(i) {
                        "● ".fg(Color::Green)
                    } else {
                        "  ".into()
                    },
                    Span::raw(p.name.clone()),
                ];
                if p.dir_path().is_none() {
                    spans.push(" ⚠".fg(Color::Yellow));
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
        .title(" Actions ")
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
                "folder  ".fg(Color::DarkGray),
                Span::raw(p.display().to_string()),
            ]),
            None => Line::from(
                "⚠ folder not set or not found - press e to edit the config".fg(Color::Yellow),
            ),
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(project.name.clone().bold()),
                Line::from(project.description.clone().fg(Color::Gray)),
                folder_line,
            ]),
            info,
        );
        let items: Vec<ListItem> = project
            .actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let status = app.status_of((sel, i));
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", i + 1), Style::new().fg(Color::DarkGray)),
                    Span::raw(format!("{:<26}", a.name)),
                    Span::styled(
                        format!("{:<9}", kind_label(a.kind)),
                        Style::new().fg(Color::DarkGray),
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
        .title(title)
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = log_block.inner(log_area);
    let lines: Vec<Line> = match app.states.get(&key).filter(|s| !s.log.is_empty()) {
        Some(st) => {
            let skip = st.log.len().saturating_sub(inner.height as usize);
            st.log
                .iter()
                .skip(skip)
                .map(|l| Line::from(l.clone()))
                .collect()
        }
        None => vec![Line::from(
            "Nothing has run yet. Press Enter to start the first action.".fg(Color::DarkGray),
        )],
    };
    f.render_widget(Paragraph::new(lines).block(log_block), log_area);

    // ---------------------------------------------------------------- footer
    let footer_line = match &app.flash {
        Some((text, at)) if at.elapsed() < Duration::from_secs(5) => Line::from(format!(" {text}").fg(Color::Yellow)),
        _ => Line::from(
            " ↑↓ move   Enter run   → actions   1-9 run   s stop   o open again   e edit list   q quit   (mouse: click, click again to run)"
                .fg(Color::DarkGray),
        ),
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
            "LAUNCHPAD",
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
