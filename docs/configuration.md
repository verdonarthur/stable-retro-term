# Configuration

retro-term reads one TOML file. [`assets/config.example.toml`](../assets/config.example.toml)
lists every key at its default value.

## File location

1. `retro-term --config PATH` reads `PATH`. If the file cannot be read, retro-term exits with an error.
2. Otherwise `$XDG_CONFIG_HOME/retro-term/config.toml`, when `XDG_CONFIG_HOME` is set to an absolute path.
3. Otherwise `$HOME/.config/retro-term/config.toml`.

A missing default file gives the defaults.

## Precedence

Built-in defaults, then the selected preset, then the file, then the command line.
`--plain` turns off the CRT effects, also with `effects.animation = true`. A program after `--` runs in the first pane only.
New panes and tabs start `shell.program` with `shell.args`, or your login shell.
Command-line values stay active after a reload.

## Keys

| Key | Type | Default | Range |
| --- | --- | --- | --- |
| `preset` | string | `"static-crt"` | `"static-crt"`, `"plain"`, `"animated-crt"` |
| `effects.crt` | bool | `true` | `false` draws without CRT effects |
| `effects.animation` | bool | `false` | `true` adds CRT flicker; needs `effects.crt`. Experimental; performance unverified |
| `font.family` | string | `""` | An installed family; `""` selects the system monospace font |
| `font.size` | integer | `14` | 6 to 72 pixels at scale 1 |
| `font.line_height` | integer | `19` | `font.size` to twice `font.size`, pixels at scale 1 |
| `terminal.scrollback_lines` | integer | `10000` | 0 to 100000 |
| `shell.program` | string | `""` | `""` selects your login shell |
| `shell.args` | array of strings | `[]` | Requires `shell.program` |
| `shell.working_directory` | string | `""` | `""` inherits the directory of retro-term |

Unknown keys are errors.

## Presets

- `static-crt`: the default look with static CRT effects.
- `plain`: no CRT effects.
- `animated-crt`: static CRT effects plus flicker on frames that retro-term draws anyway.
  Experimental; performance unverified.

The preset files are in [`assets/presets/`](../assets/presets/). Keys in your file override the preset.

## Reload

Press Ctrl+Shift+R to read the file again. Press Ctrl+Shift+P to open the [settings panel](#settings-panel).

- Applies to the open window: `preset`, the `effects` keys, and the `font` keys. A font change resizes the grid and the shell.
- Applies to new panes and tabs only: the `terminal` and `shell` keys. retro-term shows a message when they change.

## Errors

retro-term checks the whole file before it changes anything. If the file is invalid, retro-term keeps the
last valid configuration. The error shows on the last row of the window and on stderr, with the file,
line, column, and key or allowed range. The next key press or a successful reload removes it.

If the file is invalid at startup, retro-term starts with the defaults and shows the error.

## Settings panel

Ctrl+Shift+P opens a panel that edits these keys: `preset`, `effects.crt`, `font.size`,
`font.line_height`, and `terminal.scrollback_lines` (steps of 1000). The keys are in [usage](usage.md#settings-panel).

- The rows show the result of the [precedence](#precedence) rules. For example, with `effects.crt = true`
  in the file, the `plain` preset still shows CRT effects on.
- A row stops before a value that the file cannot hold. For example, `font.size` cannot go above
  `font.line_height`; the panel tells you to change the other row first.
- With `--plain`, the CRT row shows `off (--plain; file: on)`. A change still goes to the file and applies
  when you start without `--plain`.
- Changes show only in the panel. Enter saves and then reloads the file, the same as Ctrl+Shift+R.
  Enter with no change closes the panel and writes nothing.
- Saving changes only the edited values. Comments, key order, formatting, and other keys stay.
  A missing key is added to its table; a missing table is added at the end; a missing file and its
  directory are created with only the changed keys.
- The file is written to a temporary file in the same directory and then renamed over the old file.
  The file permissions stay. If the path is a symbolic link, the target file is written and the link stays.
  On any error the old file does not change and the panel shows the error.
- If the file changed on disk after the panel opened, the panel does not save. Close the panel, press
  Ctrl+Shift+R, and open it again. A change in the short time between this check and the rename is not detected.
- If the file is invalid or cannot be read, or there is no configuration path, the panel is read-only and
  shows the reason. Fix the file, then press Ctrl+Shift+R.
- If the reload after a save fails (for example, the font family is not installed), the file stays saved and
  retro-term keeps the running configuration and shows the error, the same as Ctrl+Shift+R.
- `font.family` and the `shell` keys are not in the panel. Edit the file for them.
- The panel does not open in a window that is too small for it. If the window becomes too small, the panel
  closes without saving.
