# Sietch

A terminal menu for opening and testing your projects. Pick a project, press Enter, and it starts
and opens in your browser (or editor, or folder).

```
cargo install --path .     # installs the `sietch` command
sietch                  # run it
```

**Windows, one step:** double-click `setup-windows.bat`. It installs Git, the C++ build tools and Rust if missing, clones your four projects to `%USERPROFILE%\projects`, builds Sietch and puts the icon on your Desktop.

## Using it

| Key | Does |
|---|---|
| ↑ ↓ | move |
| Enter | run the selected project's first action / the selected action |
| → / ← | switch between the project list and its actions |
| 1-9 | run that action |
| s | stop what's running (project list focused: everything of that project) |
| o | open the running web app again |
| e / r | edit / reload the project list |
| q | quit (asks again if something is still running) |

Mouse: click to select, click again to run.

## Your projects

They live in a plain text file (`sietch --print-config-path` shows where; created with
tbjp-oracle, logbook, rhcp-chatbot and gilbertordx.github.io on first run; `sietch --reset-config` rewrites it with the current defaults and keeps your old one as a .bak). `sietch --check` reports which folders
were found. Edit the `dir` paths to match your machine. Action kinds:
`service` (start, wait for the port, open `url`), `task` (run, show output), `terminal`
(hand over the terminal, e.g. a chat) and `open` (open a folder or file).

## Desktop icon

- Windows: double-click `shortcut\create-desktop-shortcut.bat`
- Linux/macOS: `shortcut/create-desktop-shortcut.sh`

## Tested

Unit tests (`cargo test`) and a pty end-to-end run (`tests/e2e_pty.py <binary>`, 25 checks) on
Linux. The Windows code paths (process killing, `cmd /C`, the shortcut scripts) are untested.
