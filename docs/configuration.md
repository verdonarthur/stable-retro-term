# Configuration

retro-term reads one TOML file. [`assets/config.example.toml`](../assets/config.example.toml)
shows a starting configuration.

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

## Profiles

The built-in profiles are `static-crt` (default), `plain`, `animated-crt`,
`default-amber`, `monochrome-green`, `deep-blue`, `commodore-64`,
`commodore-pet`, `apple-ii`, `atari-400`, `ibm-vga-8x16`,
`ibm-3278-reborn`, `neon-cyan`, `ghost-terminal`, `plasma`, `boring`, and `e-ink`. The retro profiles
set phosphor colors and effect levels. Choose a profile in Quick Settings
under General, or set `preset` in the file. The profile files are in
[`assets/presets/`](../assets/presets/).

## CRT controls

Adjust the levels in Quick Settings or under `[effects.levels]`. Each level
is an integer from 0 to 100. The effects include bloom, burn-in, static noise,
jitter, a glow line, screen curvature, ambient light, flicker, horizontal sync,
RGB shift, and frame shininess. The panel also sets scanlines, phosphor color,
screen brightness, contrast, margin, corner radius, and frame size.
Set an individual animation or glow effect to 0 to turn it off.

## Keys

| Key | Type | Default | Range |
| --- | --- | --- | --- |
| `preset` | string | `"static-crt"` | See [profiles](#profiles) |
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

## Reload

Press Ctrl+Shift+R to read the file again. Press Ctrl+Shift+S to open the [settings panel](#settings-panel).

- Applies to the open window: `preset`, the `effects` keys, and the `font` keys. A font change resizes the grid and the shell.
- Applies to new panes and tabs only: the `terminal` and `shell` keys. retro-term shows a message when they change.

## Errors

retro-term checks the whole file before it changes anything. If the file is invalid, retro-term keeps the
last valid configuration. The error shows on the last row of the window and on stderr, with the file,
line, column, and key or allowed range. The next key press or a successful reload removes it.

If the file is invalid at startup, retro-term starts with the defaults and shows the error.

## Settings panel

Ctrl+Shift+S opens Quick Settings. Use Tab to switch categories and the arrow
keys to select and change a value. Press Enter to save and apply changes.

- The rows show the result of the [precedence](#precedence) rules. For example, with `effects.crt = true`
  in the file, the `plain` preset still shows CRT effects on.
- A row stops before a value that the file cannot hold. For example, `font.size` cannot go above
  `font.line_height`; the panel tells you to change the other row first.
- With `--plain`, the CRT row shows `off (--plain; file: on)`. A change still goes to the file and applies
  when you start without `--plain`.
- Changes preview after a 150 ms pause. Enter applies pending changes, saves,
  and keeps Quick Settings open.
  Escape restores the last saved appearance.
- Choosing a profile replaces the font and CRT overrides. Editing its appearance
  displays Custom and keeps the chosen profile as the base for settings that
  still inherit their values. Shell and scrollback preferences remain independent.
- Profiles map the supported settings from
  [Cool Retro Term](https://github.com/Swordfish90/cool-retro-term/blob/master/app/qml/ApplicationSettings.qml).
  They use system fonts and this renderer's scanline model. Native window opacity,
  font width, and the upstream rasterization modes are outside this mapping.
- Saving checks whether the file changed since it was opened or last saved.

- If the file is invalid or cannot be read, or there is no configuration path, the panel is read-only and
  shows the reason. Fix the file, then press Ctrl+Shift+R.
- `font.family` and the `shell` keys are not in the panel. Edit the file for them.
- The panel does not open in a window that is too small for it. If the window becomes too small, the panel
  closes without saving.
