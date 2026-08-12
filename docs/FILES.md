# Files

Garage uses Pantheon Files as its default file explorer. Its native Miller
Columns view gives each folder its own adjacent column, so hierarchy remains
visible while browsing. Thunar stays installed as a list-view fallback for
workflows that benefit from its plugins and split view.

## Launch behavior

`SUPER + E` does not name either application. It launches
`~/.local/bin/garage-open-files`, which opens the requested folder through
`xdg-open`. The `inode/directory` XDG default therefore decides which explorer
opens, including after the user changes Default Apps in System Preferences.
Garage initially assigns both local folders and SMB locations to
`io.elementary.files.desktop`.

## Pantheon defaults

On a new install Garage seeds these preferences only when Pantheon Files has no
existing view preference:

- Miller Columns as the default view
- small content zoom
- 220 px preferred Miller column width
- 252 px sidebar width

Existing preferences always win. Pantheon keeps them in dconf under
`io.elementary.files`.

## Theme integration

Pantheon Files is GTK3. `pantheon-files.css` uses only roles from Garage's
generated light/dark palette and is scoped with the runtime
`garage-pantheon-files` class. The small Garage GTK module adds that scope to
Pantheon windows, removes the artificial sidebar gutter, and tags dynamically
created Miller-column tree views so they receive consistent padding and striped
rows. The same module supplies Thunar structure that GTK CSS cannot express.

Nautilus is neither installed by Garage nor referenced by the default-app or
theme configuration.
