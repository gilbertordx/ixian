"""End-to-end test: run the real `launchpad` binary inside a pseudo-terminal, press keys and click
the mouse like a person, and check the rendered screen plus the real processes.

usage: python tests/e2e_pty.py <path-to-launchpad-binary>        (needs: pip install pyte)
"""
import fcntl, os, pty, re, select, signal, socket, struct, subprocess, sys, tempfile, termios, time, urllib.request
import pyte

BIN = os.path.abspath(sys.argv[1])
ROWS, COLS = 38, 130
work = tempfile.mkdtemp(prefix="launchpad-e2e-")
with socket.socket() as s:
    s.bind(("127.0.0.1", 0)); PORT = s.getsockname()[1]
URL = f"http://127.0.0.1:{PORT}"
os.makedirs(f"{work}/demo"); os.makedirs(f"{work}/second")
open(f"{work}/demo/hello.txt", "w").write("hi")

config = f"""
[[projects]]
name = "Demo Web"
description = "a demo web app"
dir = '{work}/demo'
  [[projects.actions]]
  name = "Serve"
  kind = "service"
  run = "python3 -m http.server {PORT} --bind 127.0.0.1"
  url = "{URL}"
  [[projects.actions]]
  name = "Say hi"
  run = "echo hello-from-task; echo second-line"
  [[projects.actions]]
  name = "Fail"
  run = "echo about-to-fail; exit 3"
  [[projects.actions]]
  name = "Chat"
  kind = "terminal"
  run = "sh -c 'echo interactive-start; read x; echo got:$x'"
  [[projects.actions]]
  name = "Open folder"
  kind = "open"
  run = "."

[[projects]]
name = "Second"
description = "another project"
dir = '{work}/second'
  [[projects.actions]]
  name = "Task two"
  run = "echo second-task-output"

[[projects]]
name = "NoFolder"
description = "has no folder"
dir = ''
  [[projects.actions]]
  name = "Nope"
  run = "echo never"
"""
open(f"{work}/projects.toml", "w").write(config)

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
env = {**os.environ, "TERM": "xterm-256color", "LINES": str(ROWS), "COLUMNS": str(COLS)}
proc = subprocess.Popen([BIN, "--config", f"{work}/projects.toml", "--no-browser"], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=os.setsid, cwd=work)
os.close(slave)

screen = pyte.Screen(COLS, ROWS); stream = pyte.ByteStream(screen)
ALT = re.compile(rb"(\x1b\[\?1049[hl])")
def pump(timeout=0.1):
    r, _, _ = select.select([master], [], [], timeout)
    if r:
        try: data = os.read(master, 65536)
        except OSError: return False
        for piece in ALT.split(data):          # pyte has no alternate screen: clear when it switches
            if ALT.fullmatch(piece): screen.reset()
            else: stream.feed(piece)
    return True
def text(): return "\n".join(screen.display)
def wait_for(needle, timeout=30):
    end = time.time() + timeout
    while time.time() < end:
        pump(0.1)
        if needle in text(): return True
    return False
def send(b): os.write(master, b)
KEY = {"down": b"\x1b[B", "up": b"\x1b[A", "right": b"\x1b[C", "left": b"\x1b[D", "enter": b"\r"}
def find(needle):
    for y, line in enumerate(screen.display):
        x = line.find(needle)
        if x >= 0: return x, y
def click(x, y):
    send(f"\x1b[<0;{x+1};{y+1}M".encode()); time.sleep(0.05); send(f"\x1b[<0;{x+1};{y+1}m".encode()); time.sleep(0.1)
def port_open():
    try: socket.create_connection(("127.0.0.1", PORT), timeout=0.5).close(); return True
    except OSError: return False
def pids(): return subprocess.run(["pgrep", "-f", f"http.server {PORT}"], capture_output=True, text=True).stdout.split()

results = []
def check(name, ok, detail=""):
    results.append(ok); print(("PASS  " if ok else "FAIL  ") + name + (f"\n        {detail}" if not ok and detail else ""))
    if not ok: print("\n".join("        | " + l.rstrip() for l in screen.display if l.strip()))

try:
    check("starts and shows the title, projects and hints", wait_for("LAUNCHPAD") and all(t in text() for t in ["Demo Web", "Second", "NoFolder", "↑↓ move", "Enter run"]))
    check("the first project's actions are listed with numbers", all(t in text() for t in ["1 Serve", "2 Say hi", "3 Fail", "4 Chat", "5 Open folder", "a demo web app"]))
    check("a missing folder is flagged in the list", "NoFolder ⚠" in text())

    send(KEY["down"]); time.sleep(0.3); pump()
    check("↓ selects the next project and the right panel follows", wait_for("1 Task two") and "another project" in text())
    send(KEY["up"]); pump(0.3)
    check("↑ goes back", wait_for("1 Serve"))

    # ---- the main flow: Enter on a project starts its first action (the web app)
    send(KEY["enter"])
    check("Enter starts the web app and shows it as running with its address", wait_for(f"● running  {URL}", 40), "")
    check("the server really answers", urllib.request.urlopen(URL, timeout=5).status == 200)
    check("the browser step ran (test mode says what it would open)", f"would open {URL}" in text())
    check("the project is marked as having something running", "● Demo Web" in text())
    old = pids()
    send(KEY["enter"]); pump(0.5)
    check("Enter again on a running web app opens it again instead of starting a second copy", wait_for("is running - opened") and pids() == old)

    # ---- actions panel: numbers and arrows
    send(KEY["right"]); send(b"2")
    check("→ then 2 runs the task; its output streams into the output pane", wait_for("hello-from-task") and wait_for("✔ done"))
    send(b"3")
    check("a failing task shows the exit code and its output", wait_for("✖ exit code 3") and wait_for("about-to-fail"))
    send(KEY["down"]); send(KEY["down"]); send(KEY["enter"])   # action 5: open
    check("an 'open' action opens the folder", wait_for("would open " + work + "/demo"))

    # ---- mouse: click a project, click again to run
    pos = find("Second")
    send(KEY["left"]); pump(0.2)
    click(pos[0], pos[1]); pump(0.3)
    check("clicking a project selects it", wait_for("1 Task two"))
    click(pos[0], pos[1])
    check("clicking the selected project again runs its first action", wait_for("second-task-output") and wait_for("✔ done"))

    # ---- a project with no folder
    send(KEY["down"]); pump(0.3); send(KEY["enter"])
    check("a project with no folder explains what to do", wait_for("folder not set or not found"))
    send(KEY["up"]); send(KEY["up"]); pump(0.3)

    # ---- terminal action: the menu steps aside and hands over the real terminal
    send(KEY["right"]); send(b"4")
    check("a terminal action hands over the terminal", wait_for("interactive-start", 30))
    send(b"hello\r")
    check("you can type into it", wait_for("got:hello"))
    check("and it waits for Enter before returning", wait_for("Press Enter to go back"))
    send(b"\r")
    check("the menu comes back afterwards", wait_for("LAUNCHPAD") and "Demo Web" in text())

    # ---- stopping
    send(KEY["left"]); pump(0.5)      # project list focused: s stops everything of the project
    send(b"s")
    check("s stops the selected web app", wait_for("■ stopped", 20))
    time.sleep(0.5)
    check("the server process tree is gone", not port_open() and not pids(), f"pids={pids()}")

    # ---- quit with a service still running
    send(KEY["left"]); send(KEY["enter"])
    wait_for(f"● running  {URL}", 40)
    send(b"q")
    check("q warns when something is still running", wait_for("press q again"))
    time.sleep(0.2)
    send(b"q")
    end = time.time() + 15
    while proc.poll() is None and time.time() < end: pump(0.1)
    check("the second q quits (exit code 0)", proc.poll() == 0, f"exit={proc.poll()}")
    time.sleep(0.5)
    check("quitting stopped the service: nothing left running", not port_open() and not pids(), f"pids={pids()}")
finally:
    if proc.poll() is None:
        os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
    subprocess.run(["pkill", "-f", f"http.server {PORT}"])

print(f"\n{sum(results)}/{len(results)} checks passed")
sys.exit(0 if all(results) else 1)
