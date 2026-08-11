# Launcher

Garage's Quickshell launcher searches installed applications, evaluates
expressions, opens web destinations, and exposes a small command language for
desktop actions and utilities. Type a query, choose a result, and press Enter.

Command arguments use ordinary whitespace. No colon is required: write
`emoji love`, `kill quickshell`, `power reboot`, or `audio play`. Older forms
with a colon remain accepted for compatibility, but are not the canonical
syntax.

## Search and utilities

| Capability | Example | Activation |
| --- | --- | --- |
| Applications | `firefox` | Launch the selected desktop application. |
| Calculator | `(12 + 8) * 3` | Copy the result. |
| Web address | `github.com` | Open the address in the default browser. |
| Web search | `hyprland keybinds` | Search with the configured search engine. |
| Unit conversion | `1 m in inch` | Copy the conversion. |
| Currency conversion | `$1 to IDR` or `10 euro in rupiah` | Fetch a Frankfurter rate and copy the conversion. |
| Emoji search | `emoji love` | Copy the selected emoji. |
| UUID | `uuid` | Generate and copy a UUID v4. |
| Random digits | `rand(16)` | Generate and copy 16 random digits; the limit is 128. |
| Indexed files | `file launch notes` or a plain filename | Open the selected file or directory. |
| Timer | `timer 10m Tea` | Start a persistent timer and notify when it finishes. |
| Stopwatch | `stopwatch start` | Start, pause, resume, lap, or reset the persistent stopwatch. |

Application matching prioritizes graphical desktop apps. Terminal-only desktop
entries remain searchable, but rank after every matching graphical app.

Unit conversion accepts `in` or `to` and covers length, mass, temperature,
volume, time, area, and decimal or binary data sizes. Currency conversion is an
online feature backed by [Frankfurter](https://frankfurter.dev/); a failed
request can be retried from its result row.

File results come from a background filename index, not a filesystem walk at
query time. Plain launcher queries mix matching files with graphical desktop
applications; `file QUERY` shows only indexed files and directories. The index
stores paths, names, kinds, and modification times, never file contents. Hidden
directories, symlinks, caches, and common generated dependency trees are
skipped. A transactional refresh leaves the previous complete snapshot
searchable until its replacement is ready, so a scan cannot expose a partial
result set.

Configure the index in **System Preferences → General → File Indexing**. The
pane shows current activity, the last successful index time, its item count,
and a manual **Refresh Now** action. It also controls whether indexing runs,
the refresh frequency, maximum traversal depth, and the directories included.
Indexed roots are confined to the user's home directory.

Timers accept duration components from one second through seven days, for
example `timer 45s`, `timer 1h 30m`, or `timer 25m Pomodoro`. Type `timer` to
list active timers and `timer cancel` to select one to cancel. Stopwatch
queries are `stopwatch`, `stopwatch start`, `stopwatch pause`, `stopwatch
resume`, `stopwatch lap`, and `stopwatch reset`. Timer and stopwatch state is
owned by the shell rather than the launcher window, so closing and reopening
the launcher does not reset it.

## Desktop commands

| Query | Behavior |
| --- | --- |
| `power` or `system` | List shutdown, restart, sleep, logout, and lock actions. |
| `power reboot` | Open the existing session confirmation UI with Restart selected. |
| `audio` or `media` | List play, pause, stop, next-track, and mute actions. |
| `audio play` | Send the selected command through `playerctl`; mute uses `wpctl`. |
| `kill quickshell` | Fuzzy-search the current user's processes and send `SIGTERM` to the selected PID. |
| `ssh user@example.com` | Open the configured default terminal and connect with SSH. |
| `settings`, `preferences`, or `system preferences` | Open Garage System Preferences. |
| `dnd` | Toggle Do Not Disturb. |
| `night` | Toggle Night Shift. |
| `light` or `dark` | Toggle between the light and dark appearances. |
| `caffeine` | Toggle display sleep inhibition. The `caffein` spelling is also accepted. |

A bare `power` or `audio` prefix lists every action in that group. Adding an
action filters the list, so partial queries such as `power re` and `audio pa`
work. SSH accepts one host, optionally prefixed by a user; options and shell
syntax are deliberately rejected. Process termination is intentionally
graceful (`SIGTERM`), not a forced kill.
