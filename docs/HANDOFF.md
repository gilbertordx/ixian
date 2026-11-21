# Handoff notes and the briefing

Sessions (Claude Code runs, each a "worker") leave a short note when they finish.
Ixian reads the notes and opens with a **briefing**: one row per project, with what
was done, what is left and the next step. Press Enter on a row to jump to that
project. Press `b` in the menu to see it again.

## The file

One append-only Markdown file, `HANDOFF.md`, in a (preferably private) git repo that
is cloned next to your other projects (for example `~/projects/handoff`).

```markdown
## 2026-11-21 14:05 · rust-remaster · Rust remaster
Did: restyled the launcher in the portfolio palette
Left:
- confirm the clone fix on Windows
Next: run ixian --reset-config and open the menu
```

Header: `## <date time> · <project> · <session title>`. The project name should match a
project in Ixian's list (case does not matter); unknown names show a ⚠.
`Did:`, `Left:` and `Next:` take text on the same line or `- bullet` lines.

## Turning it on

The project list has a `[handoff]` section. `ixian --reset-config` fills in
`file` automatically when it finds a `handoff` folder in your projects folder; or set
it by hand:

```toml
[handoff]
file = 'C:\Users\you\projects\handoff\HANDOFF.md'
```

At start Ixian runs `git pull --ff-only` in that folder (6 second limit, offline is
fine) and reads the file. `ixian --no-briefing` skips both. `ixian --check` shows
whether the file was found.

## The line for every session

> Before you finish, or when I say "handoff", append a short entry to `HANDOFF.md` in
> the `gilbertordx/handoff` repo (attach it with add_repo if it is not attached),
> then commit and push it. Format: `## <date time> · <project> · <session title>`,
> then `Did:`, `Left:` and `Next:` lines.
