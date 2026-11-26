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
`--plain` turns off the CRT effects. A program after `--` replaces `shell.program` and `shell.args`.
Command-line values stay active after a reload.

## Keys

| Key | Type | Default | Range |
| --- | --- | --- | --- |
| `preset` | string | `"static-crt"` | `"static-crt"`, `"plain"` |
| `effects.crt` | bool | `true` | `false` draws without CRT effects |
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

The preset files are in [`assets/presets/`](../assets/presets/). Keys in your file override the preset.

## Reload

Press Ctrl+Shift+R to read the file again.

- Applies to the open window: `preset`, `effects.crt`, and the `font` keys. A font change resizes the grid and the shell.
- Applies to new shells only: the `terminal` and `shell` keys. retro-term shows a message when they change.

## Errors

retro-term checks the whole file before it changes anything. If the file is invalid, retro-term keeps the
last valid configuration. The error shows on the last row of the window and on stderr, with the file,
line, column, and key or allowed range. The next key press or a successful reload removes it.

If the file is invalid at startup, retro-term starts with the defaults and shows the error.
