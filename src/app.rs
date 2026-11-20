//! The menu's state and what each key or click does. No drawing happens here, so all of it
//! is easy to test.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::config::{Config, Kind};
use crate::runner::{Key, Msg, Runner, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Actions,
}

/// Things the main loop has to do because they need the real terminal or the file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Hand the terminal to an interactive action (a chat).
    Terminal(Key),
    EditConfig,
    ReloadConfig,
}

#[derive(Default)]
pub struct ActionState {
    pub status: Option<Status>,
    pub log: VecDeque<String>,
}

/// Where the lists were drawn last, so a mouse click can be matched to a row.
#[derive(Default, Clone, Copy)]
pub struct Areas {
    pub projects: Rect,
    pub actions: Rect,
}

pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub focus: Focus,
    pub projects: ListState,
    pub actions: ListState,
    pub states: HashMap<Key, ActionState>,
    pub runner: Runner,
    pub quit: bool,
    quit_armed: bool,
    pub flash: Option<(String, Instant)>,
    pub tick: usize,
    pub areas: Areas,
    last_click: Option<(Instant, Focus, usize)>,
}

const LOG_LIMIT: usize = 500;

impl App {
    pub fn new(config: Config, config_path: PathBuf, runner: Runner) -> Self {
        Self {
            config,
            config_path,
            focus: Focus::Projects,
            projects: ListState::default().with_selected(Some(0)),
            actions: ListState::default().with_selected(Some(0)),
            states: HashMap::new(),
            runner,
            quit: false,
            quit_armed: false,
            flash: None,
            tick: 0,
            areas: Areas::default(),
            last_click: None,
        }
    }

    pub fn flash(&mut self, text: impl Into<String>) {
        self.flash = Some((text.into(), Instant::now()));
    }

    pub fn sel_project(&self) -> usize {
        self.projects
            .selected()
            .unwrap_or(0)
            .min(self.config.projects.len().saturating_sub(1))
    }

    pub fn sel_action(&self) -> usize {
        let n = self
            .config
            .projects
            .get(self.sel_project())
            .map_or(0, |p| p.actions.len());
        self.actions
            .selected()
            .unwrap_or(0)
            .min(n.saturating_sub(1))
    }

    pub fn status_of(&self, key: Key) -> Option<&Status> {
        self.states.get(&key).and_then(|s| s.status.as_ref())
    }

    /// Is a service of this project up or starting?
    pub fn project_active(&self, project: usize) -> bool {
        self.states.iter().any(|((p, _), s)| {
            *p == project
                && matches!(
                    s.status,
                    Some(Status::Running(_) | Status::Starting | Status::Preparing(_))
                )
        })
    }

    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Log(key, line) => {
                let st = self.states.entry(key).or_default();
                st.log.push_back(line);
                while st.log.len() > LOG_LIMIT {
                    st.log.pop_front();
                }
            }
            Msg::Status(key, status) => self.states.entry(key).or_default().status = Some(status),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Projects => {
                let n = self.config.projects.len();
                if n > 0 {
                    let i = (self.sel_project() as isize + delta).rem_euclid(n as isize) as usize;
                    self.projects.select(Some(i));
                    self.actions.select(Some(0));
                }
            }
            Focus::Actions => {
                let n = self
                    .config
                    .projects
                    .get(self.sel_project())
                    .map_or(0, |p| p.actions.len());
                if n > 0 {
                    let i = (self.sel_action() as isize + delta).rem_euclid(n as isize) as usize;
                    self.actions.select(Some(i));
                }
            }
        }
    }

    /// Start (or re-open) an action.
    pub fn run_action(&mut self, key: Key) -> Effect {
        let Some(project) = self.config.projects.get(key.0).cloned() else {
            return Effect::None;
        };
        let Some(action) = project.actions.get(key.1).cloned() else {
            return Effect::None;
        };
        let Some(dir) = project.dir_path() else {
            self.flash(format!(
                "{}: folder not set or not found - press e to edit the config",
                project.name
            ));
            return Effect::None;
        };

        // "Update" actions first stop the project's running apps, so nothing stale keeps serving.
        if action.stop_services {
            let stopped = (0..project.actions.len())
                .filter(|i| self.runner.stop((key.0, *i)))
                .count();
            if stopped > 0 {
                let until = Instant::now() + Duration::from_secs(4);
                while (0..project.actions.len()).any(|i| self.runner.is_running((key.0, i)))
                    && Instant::now() < until
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                self.flash(format!("stopped {stopped} running app(s) before updating"));
            }
        }

        // A service that is already up: just open it again.
        if action.kind == Kind::Service && self.runner.is_running(key) {
            return match &action.url {
                Some(url) => {
                    match self.runner.open_target(key, url, &dir) {
                        Ok(()) => self.flash(format!("{} is running - opened {url}", action.name)),
                        Err(e) => self.flash(e),
                    }
                    Effect::None
                }
                None => {
                    self.flash(format!(
                        "{} is already running (press s to stop it)",
                        action.name
                    ));
                    Effect::None
                }
            };
        }
        if matches!(
            self.status_of(key),
            Some(Status::Preparing(_) | Status::Starting)
        ) && self.runner.is_running(key)
        {
            self.flash("still starting - the first build can take a few minutes");
            return Effect::None;
        }
        if action.kind == Kind::Terminal {
            return Effect::Terminal(key);
        }
        self.states.insert(key, ActionState::default());
        self.runner.start(key, dir, action.clone());
        self.flash(format!("starting: {}", action.name));
        Effect::None
    }

    /// Open the selected web app's address in the browser again (it must be running).
    fn open_selected(&mut self) {
        let p = self.sel_project();
        let a = if self.focus == Focus::Actions {
            self.sel_action()
        } else {
            0
        };
        let url = self.config.projects[p]
            .actions
            .get(a)
            .and_then(|act| act.url.clone());
        let dir = self.config.projects[p].dir_path();
        match (url, dir) {
            (Some(url), Some(dir)) if self.runner.is_running((p, a)) => {
                match self.runner.open_target((p, a), &url, &dir) {
                    Ok(()) => self.flash(format!("opened {url}")),
                    Err(e) => self.flash(e),
                }
            }
            _ => self.flash("nothing running to open - start the web app first (Enter)"),
        }
    }

    fn stop_selected(&mut self) {
        let p = self.sel_project();
        let keys: Vec<Key> = match self.focus {
            Focus::Actions => vec![(p, self.sel_action())],
            Focus::Projects => (0..self.config.projects[p].actions.len())
                .map(|a| (p, a))
                .collect(),
        };
        let stopped = keys.into_iter().filter(|k| self.runner.stop(*k)).count();
        self.flash(if stopped == 0 {
            "nothing running to stop".to_string()
        } else {
            format!("stopping {stopped} service(s)...")
        });
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Effect {
        if key.kind != KeyEventKind::Press {
            return Effect::None;
        }
        // any key other than q cancels a pending "press q again"
        let was_armed = std::mem::replace(&mut self.quit_armed, false);
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return Effect::None;
        }
        let count = self.config.projects.len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                let running = self.runner.running_count();
                if running > 0 && !was_armed {
                    self.quit_armed = true;
                    self.flash(format!(
                        "{running} service(s) still running - press q again to stop them and quit"
                    ));
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                if count > 0 {
                    self.focus = Focus::Actions;
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => self.focus = Focus::Projects,
            KeyCode::Home => match self.focus {
                Focus::Projects => self.projects.select(Some(0)),
                Focus::Actions => self.actions.select(Some(0)),
            },
            KeyCode::End => match self.focus {
                Focus::Projects => self.projects.select(Some(count.saturating_sub(1))),
                Focus::Actions => {
                    let n = self
                        .config
                        .projects
                        .get(self.sel_project())
                        .map_or(1, |p| p.actions.len());
                    self.actions.select(Some(n.saturating_sub(1)))
                }
            },
            KeyCode::Enter | KeyCode::Char(' ') => {
                if count > 0 {
                    let a = if self.focus == Focus::Actions {
                        self.sel_action()
                    } else {
                        0
                    };
                    return self.run_action((self.sel_project(), a));
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                if count > 0 {
                    let n = c as usize - '1' as usize;
                    if n < self.config.projects[self.sel_project()].actions.len() {
                        self.actions.select(Some(n));
                        return self.run_action((self.sel_project(), n));
                    }
                }
            }
            KeyCode::Char('s') => {
                if count > 0 {
                    self.stop_selected();
                }
            }
            KeyCode::Char('o') => {
                if count > 0 {
                    self.open_selected();
                }
            }
            KeyCode::Char('e') => return Effect::EditConfig,
            KeyCode::Char('r') => return Effect::ReloadConfig,
            _ => {}
        }
        Effect::None
    }

    /// Click a row to select it; click it again (or double-click) to run it.
    pub fn on_mouse(&mut self, m: MouseEvent) -> Effect {
        match m.kind {
            MouseEventKind::ScrollUp => self.move_selection(-1),
            MouseEventKind::ScrollDown => self.move_selection(1),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = |area: Rect, state: &ListState, n: usize| -> Option<usize> {
                    let inside = m.column >= area.x
                        && m.column < area.x + area.width
                        && m.row >= area.y
                        && m.row < area.y + area.height;
                    let i = state.offset() + (m.row.checked_sub(area.y)? as usize);
                    (inside && i < n).then_some(i)
                };
                let n_actions = self
                    .config
                    .projects
                    .get(self.sel_project())
                    .map_or(0, |p| p.actions.len());
                if let Some(i) = hit(
                    self.areas.projects,
                    &self.projects,
                    self.config.projects.len(),
                ) {
                    return self.click(Focus::Projects, i);
                }
                if let Some(i) = hit(self.areas.actions, &self.actions, n_actions) {
                    return self.click(Focus::Actions, i);
                }
            }
            _ => {}
        }
        Effect::None
    }

    fn click(&mut self, focus: Focus, index: usize) -> Effect {
        let again = matches!(self.last_click, Some((t, f, i)) if f == focus && i == index && t.elapsed() < Duration::from_millis(600));
        self.last_click = Some((Instant::now(), focus, index));
        self.focus = focus;
        match focus {
            Focus::Projects => {
                if self.sel_project() != index {
                    self.actions.select(Some(0));
                }
                self.projects.select(Some(index));
                if again {
                    return self.run_action((index, 0));
                }
            }
            Focus::Actions => {
                self.actions.select(Some(index));
                if again {
                    return self.run_action((self.sel_project(), index));
                }
            }
        }
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, Project};
    use ratatui::crossterm::event::{KeyEventState, MouseEvent};
    use std::sync::mpsc;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn action(name: &str, kind: Kind, run: &str) -> Action {
        Action {
            name: name.into(),
            kind,
            run: run.into(),
            cwd: None,
            url: None,
            before: vec![],
            env: Default::default(),
            wait_secs: 5,
            stop_services: false,
        }
    }

    fn app() -> (App, mpsc::Receiver<Msg>) {
        let dir = std::env::temp_dir().display().to_string();
        let cfg = Config {
            projects: vec![
                Project {
                    name: "Alpha".into(),
                    description: "first".into(),
                    dir: dir.clone(),
                    actions: vec![
                        action("Open", Kind::Open, "."),
                        action("Test", Kind::Task, "echo hi"),
                        action("Chat", Kind::Terminal, "x"),
                    ],
                },
                Project {
                    name: "Beta".into(),
                    description: "second".into(),
                    dir: String::new(),
                    actions: vec![action("Go", Kind::Task, "echo hi")],
                },
                Project {
                    name: "Gamma".into(),
                    description: "".into(),
                    dir,
                    actions: vec![],
                },
            ],
        };
        let (tx, rx) = mpsc::channel();
        (
            App::new(cfg, PathBuf::from("x.toml"), Runner::new(tx, true)),
            rx,
        )
    }

    fn click_at(app: &mut App, x: u16, y: u16) -> Effect {
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn up_and_down_move_and_wrap_around() {
        let (mut a, _rx) = app();
        assert_eq!(a.sel_project(), 0);
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.sel_project(), 1);
        a.on_key(key(KeyCode::Char('j')));
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.sel_project(), 0, "wraps from the last to the first");
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.sel_project(), 2, "and back");
        a.on_key(key(KeyCode::Home));
        assert_eq!(a.sel_project(), 0);
        a.on_key(key(KeyCode::End));
        assert_eq!(a.sel_project(), 2);
    }

    #[test]
    fn changing_project_resets_the_action_choice() {
        let (mut a, _rx) = app();
        a.on_key(key(KeyCode::Right));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        assert_eq!((a.focus, a.sel_action()), (Focus::Actions, 2));
        a.on_key(key(KeyCode::Left));
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.sel_action(), 0);
    }

    #[test]
    fn enter_on_a_project_runs_its_first_action_and_enter_on_an_action_runs_that_one() {
        let (mut a, rx) = app();
        assert_eq!(a.on_key(key(KeyCode::Enter)), Effect::None);
        let msgs: Vec<String> = std::iter::from_fn(|| rx.recv_timeout(Duration::from_secs(5)).ok())
            .take(3)
            .map(|m| format!("{m:?}"))
            .collect();
        assert!(msgs.iter().any(|m| m.contains("would open")), "{msgs:?}");

        a.on_key(key(KeyCode::Right));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down)); // "Chat" (a terminal action)
        assert_eq!(a.on_key(key(KeyCode::Enter)), Effect::Terminal((0, 2)));
    }

    #[test]
    fn number_keys_run_that_action() {
        let (mut a, _rx) = app();
        assert_eq!(a.on_key(key(KeyCode::Char('3'))), Effect::Terminal((0, 2)));
        assert_eq!(a.sel_action(), 2);
        assert_eq!(
            a.on_key(key(KeyCode::Char('9'))),
            Effect::None,
            "no 9th action: nothing happens"
        );
    }

    #[test]
    fn a_project_without_a_folder_explains_what_to_do() {
        let (mut a, _rx) = app();
        a.on_key(key(KeyCode::Down)); // Beta has dir = ""
        assert_eq!(a.on_key(key(KeyCode::Enter)), Effect::None);
        let (text, _) = a.flash.clone().unwrap();
        assert!(text.contains("Beta") && text.contains("press e"), "{text}");
    }

    #[test]
    fn a_project_with_no_actions_does_not_crash() {
        let (mut a, _rx) = app();
        a.on_key(key(KeyCode::End)); // Gamma, no actions
        for k in [
            KeyCode::Enter,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('1'),
            KeyCode::Char('s'),
            KeyCode::End,
        ] {
            a.on_key(key(k));
        }
        assert!(!a.quit);
    }

    #[test]
    fn q_quits_at_once_when_nothing_runs() {
        let (mut a, _rx) = app();
        a.on_key(key(KeyCode::Char('q')));
        assert!(a.quit);
    }

    #[test]
    fn q_asks_for_confirmation_while_a_service_runs() {
        let (mut a, rx) = app();
        a.config.projects[0]
            .actions
            .push(action("Srv", Kind::Service, "sleep 30"));
        a.on_key(key(KeyCode::Char('4')));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !a.runner.is_running((0, 3)) && Instant::now() < deadline {
            while let Ok(m) = rx.try_recv() {
                a.on_msg(m);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        a.on_key(key(KeyCode::Char('q')));
        assert!(!a.quit, "first q only warns");
        assert!(a.flash.as_ref().unwrap().0.contains("press q again"));
        a.on_key(key(KeyCode::Char('x'))); // something else cancels it
        a.on_key(key(KeyCode::Char('q')));
        assert!(!a.quit);
        a.on_key(key(KeyCode::Char('q')));
        assert!(a.quit, "second q quits");
        a.runner.stop_all();
    }

    #[test]
    fn ctrl_c_always_quits() {
        let (mut a, _rx) = app();
        a.on_key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(a.quit);
    }

    #[test]
    fn key_releases_are_ignored() {
        let (mut a, _rx) = app();
        a.on_key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        assert_eq!(a.sel_project(), 0);
    }

    #[test]
    fn clicks_select_and_a_second_click_runs() {
        let (mut a, _rx) = app();
        a.areas = Areas {
            projects: Rect::new(1, 3, 20, 3),
            actions: Rect::new(30, 6, 40, 3),
        };
        assert_eq!(click_at(&mut a, 5, 4), Effect::None); // row 1 -> project 1 (Beta)
        assert_eq!((a.sel_project(), a.focus), (1, Focus::Projects));
        click_at(&mut a, 5, 3);
        assert_eq!(a.sel_project(), 0);
        click_at(&mut a, 35, 8); // third action row of Alpha (Chat)
        assert_eq!((a.focus, a.sel_action()), (Focus::Actions, 2));
        assert_eq!(
            click_at(&mut a, 35, 8),
            Effect::Terminal((0, 2)),
            "second click on the same row runs it"
        );
        click_at(&mut a, 70, 20); // outside everything
        assert_eq!(a.sel_project(), 0);
    }

    #[test]
    fn the_mouse_wheel_moves_the_selection() {
        let (mut a, _rx) = app();
        a.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(a.sel_project(), 1);
        a.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(a.sel_project(), 0);
    }

    #[test]
    fn output_is_kept_but_capped() {
        let (mut a, _rx) = app();
        for i in 0..(LOG_LIMIT + 50) {
            a.on_msg(Msg::Log((0, 0), format!("line {i}")));
        }
        let log = &a.states[&(0, 0)].log;
        assert_eq!(
            (log.len(), log.back().unwrap().as_str()),
            (LOG_LIMIT, "line 549")
        );
        a.on_msg(Msg::Status((0, 0), Status::Running(None)));
        assert!(a.project_active(0) && !a.project_active(1));
    }
}
