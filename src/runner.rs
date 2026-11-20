//! Starting, watching and stopping the commands behind each action.
//!
//! Every action runs on its own background thread and reports back through a channel
//! (`Msg`), so the menu never freezes. Commands run through the system shell (`sh -c`, or
//! `cmd /C` on Windows) in the project folder. Services get their own process group (Unix) so
//! stopping one also stops everything it started (cargo -> your server).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{Action, Kind, Step, expand_home};

/// `(project index, action index)`
pub type Key = (usize, usize);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Running a `before` step (shows which).
    Preparing(String),
    /// A service was started and we are waiting for its address to answer.
    Starting,
    /// A service is up (with the address that was opened, if any).
    Running(Option<String>),
    Done,
    Failed(String),
    Stopped,
}

#[derive(Debug)]
pub enum Msg {
    Log(Key, String),
    Status(Key, Status),
}

#[derive(Clone)]
pub struct Runner {
    tx: Sender<Msg>,
    pids: Arc<Mutex<HashMap<Key, u32>>>,
    stopping: Arc<Mutex<HashSet<Key>>>,
    /// In test mode the browser is never opened; the log says what would have been opened.
    pub no_browser: bool,
}

/// Build a command for the system shell. `raw` means "exactly as written" (Windows needs this).
pub fn shell(cmdline: &str, cwd: &Path, env: &BTreeMap<String, String>) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").raw_arg(cmdline);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmdline);
        c
    };
    cmd.current_dir(cwd).envs(env);
    cmd
}

/// The folder an action runs in: the project folder, plus the action's `cwd` if it has one.
pub fn action_dir(project_dir: &Path, action: &Action) -> PathBuf {
    match &action.cwd {
        Some(sub) => project_dir.join(sub),
        None => project_dir.to_path_buf(),
    }
}

/// Should this `before` step run? (`unless` steps are skipped when the path already exists.)
pub fn step_needed(step: &Step, project_dir: &Path) -> Option<String> {
    match step {
        Step::Cmd(c) => Some(c.clone()),
        Step::When { unless, run } => {
            let p = expand_home(unless);
            let p = if p.is_absolute() {
                p
            } else {
                project_dir.join(p)
            };
            (!p.exists()).then(|| run.clone())
        }
    }
}

/// `http://127.0.0.1:8501/x` -> `("127.0.0.1", 8501)`
pub fn host_port(url: &str) -> Option<(String, u16)> {
    let (default_port, rest) = if let Some(r) = url.strip_prefix("http://") {
        (80, r)
    } else {
        let r = url.strip_prefix("https://")?;
        (443, r)
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    match authority.rsplit_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok()?)),
        None => Some((authority.to_string(), default_port)),
    }
}

/// Wait until something accepts connections on the address, or give up. Returns false on
/// timeout or if `cancel` is set.
pub fn wait_for_port(host: &str, port: u16, timeout: Duration, cancel: &AtomicBool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
        let addrs = (host, port)
            .to_socket_addrs()
            .map(|a| a.collect::<Vec<_>>())
            .unwrap_or_default();
        if addrs
            .iter()
            .any(|a| TcpStream::connect_timeout(a, Duration::from_millis(300)).is_ok())
        {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Stop a process and everything it started.
pub fn kill_tree(pid: u32) {
    #[cfg(unix)]
    unsafe {
        // negative pid = the whole process group (the service was started as its leader)
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
}

#[cfg(unix)]
fn force_kill_tree(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

fn pump(
    reader: impl std::io::Read + Send + 'static,
    key: Key,
    tx: Sender<Msg>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            // progress bars use carriage returns; show only the last piece
            let line = line.rsplit('\r').next().unwrap_or("").to_string();
            if !line.trim().is_empty() {
                let _ = tx.send(Msg::Log(key, line));
            }
        }
    })
}

impl Runner {
    pub fn new(tx: Sender<Msg>, no_browser: bool) -> Self {
        Self {
            tx,
            pids: Default::default(),
            stopping: Default::default(),
            no_browser,
        }
    }

    fn log(&self, key: Key, text: impl Into<String>) {
        let _ = self.tx.send(Msg::Log(key, text.into()));
    }

    fn status(&self, key: Key, s: Status) {
        let _ = self.tx.send(Msg::Status(key, s));
    }

    /// Open a web address or a path in the system's default app.
    pub fn open_target(&self, key: Key, target: &str, project_dir: &Path) -> Result<(), String> {
        let is_url = target.starts_with("http://") || target.starts_with("https://");
        let resolved = if is_url {
            target.to_string()
        } else {
            let p = expand_home(target);
            let p = if p.is_absolute() {
                p
            } else {
                project_dir.join(p)
            };
            p.display().to_string()
        };
        if self.no_browser {
            self.log(key, format!("(test mode) would open {resolved}"));
            return Ok(());
        }
        open::that_detached(&resolved).map_err(|e| format!("could not open {resolved}: {e}"))?;
        self.log(key, format!("opened {resolved}"));
        Ok(())
    }

    pub fn is_running(&self, key: Key) -> bool {
        self.pids.lock().expect("pids lock").contains_key(&key)
    }

    /// Run the `before` steps to completion, streaming their output. `Err` = a step failed.
    pub fn run_before(&self, key: Key, project_dir: &Path, action: &Action) -> Result<(), String> {
        let dir = action_dir(project_dir, action);
        for step in &action.before {
            let Some(cmd) = step_needed(step, project_dir) else {
                continue;
            };
            self.status(key, Status::Preparing(cmd.clone()));
            self.log(key, format!("$ {cmd}"));
            let code = self.run_to_end(key, &cmd, &dir, &action.env)?;
            if code != 0 {
                return Err(format!("step failed (exit {code}): {cmd}"));
            }
        }
        Ok(())
    }

    fn run_to_end(
        &self,
        key: Key,
        cmdline: &str,
        dir: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<i32, String> {
        let mut child = shell(cmdline, dir, env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start `{cmdline}`: {e}"))?;
        let readers = [
            pump(child.stdout.take().expect("piped"), key, self.tx.clone()),
            pump(child.stderr.take().expect("piped"), key, self.tx.clone()),
        ];
        let status = child.wait().map_err(|e| e.to_string())?;
        for r in readers {
            let _ = r.join();
        }
        Ok(status.code().unwrap_or(-1))
    }

    /// Start an action in the background (not for `Kind::Terminal`, which needs the real terminal).
    pub fn start(&self, key: Key, project_dir: PathBuf, action: Action) {
        let runner = self.clone();
        self.stopping.lock().expect("lock").remove(&key);
        thread::spawn(move || {
            if let Err(e) = runner.run_before(key, &project_dir, &action) {
                runner.status(key, Status::Failed(e));
                return;
            }
            match action.kind {
                Kind::Open => match runner.open_target(key, &action.run, &project_dir) {
                    Ok(()) => runner.status(key, Status::Done),
                    Err(e) => runner.status(key, Status::Failed(e)),
                },
                Kind::Task | Kind::Terminal => {
                    let dir = action_dir(&project_dir, &action);
                    runner.status(key, Status::Starting);
                    runner.log(key, format!("$ {}", action.run));
                    match runner.run_to_end(key, &action.run, &dir, &action.env) {
                        Ok(0) => runner.status(key, Status::Done),
                        Ok(code) => runner.status(key, Status::Failed(format!("exit code {code}"))),
                        Err(e) => runner.status(key, Status::Failed(e)),
                    }
                }
                Kind::Service => runner.run_service(key, &project_dir, &action),
            }
        });
    }

    fn run_service(&self, key: Key, project_dir: &Path, action: &Action) {
        let dir = action_dir(project_dir, action);
        self.status(key, Status::Starting);
        self.log(key, format!("$ {}", action.run));
        let mut cmd = shell(&action.run, &dir, &action.env);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0); // its own group, so one signal stops the whole tree
        }
        let mut child: Child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return self.status(key, Status::Failed(format!("could not start: {e}"))),
        };
        let pid = child.id();
        self.pids.lock().expect("lock").insert(key, pid);
        let readers = [
            pump(child.stdout.take().expect("piped"), key, self.tx.clone()),
            pump(child.stderr.take().expect("piped"), key, self.tx.clone()),
        ];

        let cancel = Arc::new(AtomicBool::new(false));
        match action.url.clone() {
            None => self.status(key, Status::Running(None)),
            Some(url) => {
                let (runner, cancel2, wait) = (
                    self.clone(),
                    cancel.clone(),
                    Duration::from_secs(action.wait_secs),
                );
                let project_dir = project_dir.to_path_buf();
                thread::spawn(move || {
                    let Some((host, port)) = host_port(&url) else {
                        return runner
                            .status(key, Status::Failed(format!("not a web address: {url}")));
                    };
                    if wait_for_port(&host, port, wait, &cancel2) {
                        thread::sleep(Duration::from_millis(300)); // let the server finish starting
                        if let Err(e) = runner.open_target(key, &url, &project_dir) {
                            runner.log(key, e);
                        }
                        runner.status(key, Status::Running(Some(url)));
                    } else if !cancel2.load(Ordering::Relaxed) {
                        runner.status(
                            key,
                            Status::Failed(format!(
                                "{url} did not answer within {} s",
                                wait.as_secs()
                            )),
                        );
                    }
                });
            }
        }

        let exit = child.wait();
        cancel.store(true, Ordering::Relaxed);
        for r in readers {
            let _ = r.join();
        }
        self.pids.lock().expect("lock").remove(&key);
        let was_stopped = self.stopping.lock().expect("lock").remove(&key);
        self.status(
            key,
            match exit {
                _ if was_stopped => Status::Stopped,
                Ok(s) if s.success() => Status::Done,
                Ok(s) => Status::Failed(format!(
                    "stopped unexpectedly (exit code {})",
                    s.code().unwrap_or(-1)
                )),
                Err(e) => Status::Failed(e.to_string()),
            },
        );
    }

    /// Stop a running service. Returns false if it was not running.
    pub fn stop(&self, key: Key) -> bool {
        let pid = self.pids.lock().expect("lock").get(&key).copied();
        let Some(pid) = pid else { return false };
        self.stopping.lock().expect("lock").insert(key);
        kill_tree(pid);
        #[cfg(unix)]
        {
            // anything that ignored the polite request is stopped for good after 3 s
            let pids = self.pids.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(3));
                if pids.lock().expect("lock").get(&key) == Some(&pid) {
                    force_kill_tree(pid);
                }
            });
        }
        true
    }

    pub fn running_count(&self) -> usize {
        self.pids.lock().expect("lock").len()
    }

    /// Stop everything and wait (briefly) for it to exit. Used when the menu closes.
    pub fn stop_all(&self) {
        let keys: Vec<Key> = self.pids.lock().expect("lock").keys().copied().collect();
        for k in keys {
            self.stop(k);
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        while self.running_count() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn collect(
        rx: &mpsc::Receiver<Msg>,
        until: impl Fn(&Status) -> bool,
        secs: u64,
    ) -> (Vec<String>, Vec<Status>) {
        let (mut logs, mut statuses) = (Vec::new(), Vec::new());
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Log(_, l)) => logs.push(l),
                Ok(Msg::Status(_, s)) => {
                    let done = until(&s);
                    statuses.push(s);
                    if done {
                        break;
                    }
                }
                Err(_) => {}
            }
        }
        (logs, statuses)
    }

    fn action(kind: Kind, run: &str) -> Action {
        Action {
            name: "t".into(),
            kind,
            run: run.into(),
            cwd: None,
            url: None,
            before: vec![],
            env: Default::default(),
            wait_secs: 20,
        }
    }

    #[test]
    fn host_and_port_are_read_from_urls() {
        assert_eq!(
            host_port("http://127.0.0.1:8501/x?y=1"),
            Some(("127.0.0.1".into(), 8501))
        );
        assert_eq!(
            host_port("http://localhost/"),
            Some(("localhost".into(), 80))
        );
        assert_eq!(
            host_port("https://example.com"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(host_port("ftp://x"), None);
        assert_eq!(host_port("http://host:notaport/"), None);
    }

    #[test]
    fn unless_steps_are_skipped_when_the_file_exists() {
        let dir = std::env::temp_dir().join(format!("launchpad-step-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let step = Step::When {
            unless: "made.txt".into(),
            run: "make it".into(),
        };
        assert_eq!(step_needed(&step, &dir), Some("make it".into()));
        std::fs::write(dir.join("made.txt"), "x").unwrap();
        assert_eq!(step_needed(&step, &dir), None);
        assert_eq!(
            step_needed(&Step::Cmd("always".into()), &dir),
            Some("always".into())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn waiting_for_a_port_succeeds_when_something_listens_and_gives_up_otherwise() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = AtomicBool::new(false);
        assert!(wait_for_port(
            "127.0.0.1",
            port,
            Duration::from_secs(2),
            &cancel
        ));
        drop(listener);
        assert!(!wait_for_port(
            "127.0.0.1",
            port,
            Duration::from_millis(600),
            &cancel
        ));
        cancel.store(true, Ordering::Relaxed);
        assert!(
            !wait_for_port("127.0.0.1", 1, Duration::from_secs(5), &cancel),
            "cancel returns at once"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_task_streams_its_output_and_reports_success_or_failure() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        r.start(
            (0, 0),
            std::env::temp_dir(),
            action(Kind::Task, "echo hello; echo oops 1>&2"),
        );
        let (logs, st) = collect(&rx, |s| matches!(s, Status::Done | Status::Failed(_)), 10);
        assert!(
            logs.contains(&"hello".to_string()) && logs.contains(&"oops".to_string()),
            "{logs:?}"
        );
        assert_eq!(st.last(), Some(&Status::Done));

        r.start(
            (0, 1),
            std::env::temp_dir(),
            action(Kind::Task, "echo bad; exit 3"),
        );
        let (_, st) = collect(&rx, |s| matches!(s, Status::Done | Status::Failed(_)), 10);
        assert_eq!(st.last(), Some(&Status::Failed("exit code 3".into())));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_before_step_stops_the_action() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        let mut a = action(Kind::Task, "echo should-not-run");
        a.before = vec![Step::Cmd("exit 7".into())];
        r.start((0, 0), std::env::temp_dir(), a);
        let (logs, st) = collect(&rx, |s| matches!(s, Status::Failed(_)), 10);
        assert!(
            matches!(st.last(), Some(Status::Failed(m)) if m.contains("exit 7")),
            "{st:?}"
        );
        assert!(!logs.iter().any(|l| l.contains("should-not-run")));
    }

    #[cfg(unix)]
    #[test]
    fn open_actions_report_what_they_would_open_in_test_mode() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        r.start(
            (0, 0),
            PathBuf::from("/some/project"),
            action(Kind::Open, "docs/guide.md"),
        );
        let (logs, st) = collect(&rx, |s| matches!(s, Status::Done | Status::Failed(_)), 10);
        assert!(
            logs.iter()
                .any(|l| l.contains("would open /some/project/docs/guide.md")),
            "{logs:?}"
        );
        assert_eq!(st.last(), Some(&Status::Done));
    }

    #[cfg(unix)]
    #[test]
    fn a_service_waits_for_its_address_then_runs_until_stopped_and_leaves_no_process_behind() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        let mut a = action(
            Kind::Service,
            &format!("sh -c 'sleep 1; exec python3 -m http.server {port} --bind 127.0.0.1'"),
        );
        a.url = Some(format!("http://127.0.0.1:{port}/"));
        r.start((0, 0), std::env::temp_dir(), a);

        let (logs, st) = collect(
            &rx,
            |s| matches!(s, Status::Running(_) | Status::Failed(_)),
            30,
        );
        assert_eq!(st.first(), Some(&Status::Starting));
        assert!(
            matches!(st.last(), Some(Status::Running(Some(u))) if u.contains(&port.to_string())),
            "{st:?} {logs:?}"
        );
        assert!(
            logs.iter()
                .any(|l| l.contains("would open http://127.0.0.1")),
            "browser step ran: {logs:?}"
        );
        assert!(r.is_running((0, 0)) && r.running_count() == 1);
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_ok(),
            "server really answers"
        );

        assert!(r.stop((0, 0)));
        let (_, st) = collect(
            &rx,
            |s| matches!(s, Status::Stopped | Status::Failed(_)),
            15,
        );
        assert_eq!(st.last(), Some(&Status::Stopped), "{st:?}");
        assert_eq!(r.running_count(), 0);
        thread::sleep(Duration::from_millis(500));
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "the server process tree was stopped"
        );
        assert!(!r.stop((0, 0)), "stopping twice is harmless");
    }

    #[cfg(unix)]
    #[test]
    fn stop_all_stops_every_service() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        for i in 0..3 {
            r.start(
                (0, i),
                std::env::temp_dir(),
                action(Kind::Service, "sleep 60"),
            );
        }
        let mut running = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while running < 3 && Instant::now() < deadline {
            if let Ok(Msg::Status(_, Status::Running(_))) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                running += 1;
            }
        }
        assert_eq!(r.running_count(), 3);
        r.stop_all();
        assert_eq!(r.running_count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_service_that_never_answers_fails_with_a_clear_message() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(tx, true);
        let mut a = action(Kind::Service, "sleep 30");
        a.url = Some("http://127.0.0.1:1/".into()); // nothing listens on port 1
        a.wait_secs = 1;
        r.start((0, 0), std::env::temp_dir(), a);
        let (_, st) = collect(&rx, |s| matches!(s, Status::Failed(_)), 15);
        assert!(
            matches!(st.last(), Some(Status::Failed(m)) if m.contains("did not answer")),
            "{st:?}"
        );
        r.stop_all();
    }
}
