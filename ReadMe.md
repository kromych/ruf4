# ruf4

[![CI](https://github.com/kromych/ruf4/actions/workflows/ci.yml/badge.svg)](https://github.com/kromych/ruf4/actions/workflows/ci.yml)

<p align="center">
  <img src="resources/icons/ruf4.svg" width="128" alt="ruf4 icon">
</p>

> NOTE: This is **alpha-quality** software! It _may_ delete and/or corrupt
> your files at some point and cause other losses, non-intentionally though.
> Also your sense of aesthetics _may_ be hurt. Use at _your own_ risk and expense.
> I'm always happy to make it better if you tell me what should be changed, or
> you can post a PR in the spirit of OSS.

We all sometimes file, and here is a double-panel file commander. It's been created
because I couldn't use what I wanted to due to legal regulations, and what I could
use looked bad, didn't have the true rough spirit of a double-panel file commander.
So rock ur files, folks :)

This is built in Rust on the TUI framework derived from
[Microsoft Edit](https://github.com/microsoft/edit). It is an immediate mode TUI,
very small and just enough.

Runs on Linux, macOS, and Windows. You can download the latest pre-release
[0.0.2](https://github.com/kromych/ruf4/releases/tag/v0.0.2).

If you are a developer, [here](./ReleaseFlow.md) are the gory details and notes on builds/releases.
## Screenshots

### Main view
![Main view](resources/screenshots/how-it-looks.png)

### Quick view for text

![Quick view for text](resources/screenshots/qview-highlight.png)

### Quick view for binaries

![Quick view for binaries](resources/screenshots/qview-bin.png)

### Files menu

![Files menu](resources/screenshots/menu-files.png)

### Commands menu

![Commands menu](resources/screenshots/menu-commands.png)

### Select FS root dialog

![Select root](resources/screenshots/select-root.png)

### Directory history dialog

![Directory history](resources/screenshots/history-directory.png)

### Command history dialog

![Command history](resources/screenshots/history-commands.png)

## Keyboard shortcuts

### Navigation

| Key | Action |
|-----|--------|
| Up / Down | Move cursor |
| Page Up / Page Down | Scroll by page |
| Home / End | Jump to first / last entry |
| Enter | Enter directory or open file |
| Tab | Switch active panel |
| Backspace | Go to parent directory |
| Alt+letters | Quick search: jump to file by name prefix |

### File operations

| Key | Action |
|-----|--------|
| F4 | Rename |
| F5 | Copy |
| F6 | Move |
| F7 | Make directory |
| F8 / Delete | Delete |
| Ctrl+G | Change root / drive |
| Ctrl+D | Directory history |
| Ctrl+E | Command history |
| Ctrl+R | Refresh both panels |

### Selection

| Key | Action |
|-----|--------|
| Ins / Ctrl+Space | Toggle selection and move down |
| + | Select group (glob pattern) |
| - | Deselect group (glob pattern) |
| * | Invert selection |
| Ctrl+A | Select all |

### View & sorting

| Key | Action |
|-----|--------|
| F3 / Ctrl+Q | Toggle quick view panel |
| Ctrl+H | Toggle hidden files |
| Ctrl+F3 | Sort by name |
| Ctrl+F4 | Sort by extension |
| Ctrl+F5 | Sort by date |
| Ctrl+F6 | Sort by size |

### General

| Key | Action |
|-----|--------|
| F1 | Help screen |
| F2 | Save settings |
| F9 | Focus menubar |
| F10 | Quit (with confirmation) |
| Ctrl+O | Show the user screen (the output of previously run commands); Ctrl+O or Esc returns |
| Any letter | Activate command line |

### macOS alternatives

On macOS, F-keys are mapped to system functions by default (brightness,
Mission Control, media, volume). These Ctrl shortcuts work without Fn:

| Key | Action |
|-----|--------|
| Ctrl+S | Save settings (F2) |
| Ctrl+Q | Toggle quick view (F3) |
| Ctrl+P | Rename (F4) |
| Ctrl+C | Copy (F5) |
| Ctrl+K | Move (F6) |
| Ctrl+N | Make directory (F7) |
| Ctrl+X | Delete (F8) |
| Ctrl+W | Quit (F10) |

### Command line

Type any text to activate the command line at the bottom of the screen.
Commands run in the active panel's directory.

| Key | Action |
|-----|--------|
| Enter | Execute command |
| Escape | Cancel |
| Backspace | Delete character |

Commands run in the foreground with the terminal handed back to them, so
interactive programs (a shell, `python`, `vim`, `less`) work normally. The
panel display is restored when the command exits; press Enter at the prompt to
return. Press Ctrl+O at any time to peek at that output again.

## SSH remote filesystems

The change-root dialog (Ctrl+G) lists the hosts from `~/.ssh/config` as
`ssh://host` roots next to the local drives. Panels can also be pointed at any
`ssh://[user@]host[:port]/path` with the `cd` command. Remote directories
browse, sort, quick-view, copy, move, rename, and delete like local ones;
copies stream between hosts through ruf4 with byte progress. Enter on a remote
file downloads it to a temporary directory and opens it; the command line runs
commands on the remote host in the panel's directory over `ssh -t`.

Transport is the OpenSSH client: `ruf4` spawns `ssh -s <host> sftp` and speaks
SFTP over it, so keys, agents, jump hosts, and everything else in
`~/.ssh/config` behave exactly like plain `ssh`. On the first use of a host a
connection master is opened with the panels hidden so host-key and password
prompts work; subsequent channels multiplex over its socket. Set
`RUF4_SSH_CONFIG` to point `ssh` at an alternative client configuration file.
On Windows, where OpenSSH lacks multiplexing, authentication must be
non-interactive (keys or an agent).

### Dialogs

Most confirmation dialogs respond to:

| Key | Action |
|-----|--------|
| Y / Enter | Confirm |
| N / Escape | Cancel |
| A | All (in overwrite prompts) |

### Mouse

- Click a panel to make it active
- Click a file entry to select it
- Double-click to enter a directory or open a file
- Scroll wheel to navigate; over the quick view panel it scrolls the preview
- Click the function key bar at the bottom for quick access

### Clicking around

| Area | Action |
|------|--------|
| Panel path (title bar) | Open change root dialog |
| File entry | Select entry; double-click to open/enter |
| Sort indicator in footer (`Sort:Name+`) | Open sort mode dialog |
| Hidden indicator in footer (`[H]` / `[ ]`) | Toggle hidden files |
| Function key bar (bottom row) | Invoke the corresponding F-key action |
| Help dialog entry | Close help and invoke the shortcut's action |

## License

MIT
