# Usage

retro-term runs on Linux in a native Wayland session. It does not run under X11, macOS, or Windows.

## Requirements

- A Wayland compositor, the Wayland client library, and `libxkbcommon`.
- A Vulkan loader and driver.
- A system monospace font, plus fallback fonts for the scripts you use.
- For clipboard copy and paste: a compositor that exposes `ext-data-control` or `wlr-data-control`.

## Keyboard shortcuts

These shortcuts are not sent to the shell.

| Shortcut | Action |
| --- | --- |
| Ctrl+Shift+C | Copy the selection |
| Ctrl+Shift+V | Paste (up to 8 MiB of UTF-8 text) |
| Ctrl+Shift+R | Reload the [configuration file](configuration.md) |
| Ctrl+Shift+P | Open or close the action panel |
| Ctrl+Shift+S | Open or close [Quick Settings](#settings-panel) |
| Ctrl+Shift+T | Open a tab after the active tab |
| Ctrl+Shift+W | Close the focused pane (no confirmation) |
| Ctrl+Shift+E | Split the focused pane side by side |
| Ctrl+Shift+O | Split the focused pane stacked |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+PageDown / Ctrl+PageUp | Next / previous tab |
| Alt+PageDown / Alt+PageUp | Next / previous pane |
| Alt+Arrow | Move the focus to the adjacent pane |
| Ctrl+Shift+Arrow | Move the nearest divider by one cell |

Every other key goes to the shell. For example, Ctrl+P sends the same byte as in other terminals.

Use Up and Down to move through the action panel or context menu. Press
Enter or click to run the action. The action panel shows shortcuts beside the
actions. Choose **Keyboard shortcuts** to see the full list. Use Up and Down
to scroll. Press Escape to close.

## Mouse

- Click a pane to focus it. Click a tab to select it.
- Drag to select text in a pane.
- Right-click a pane for Copy, Paste, tab and split actions, and Quick Settings.
- The wheel scrolls the history of the pane under the pointer.

## Tabs and splits

The tab bar shows when two or more tabs are open. A shell exit closes its pane.
When the last pane closes, the window closes.

## Settings panel

Open Quick Settings from a menu or press Ctrl+Shift+S. It groups settings
under General, Terminal, Effects, and Advanced.

| Key | Action |
| --- | --- |
| Tab | Switch category |
| Space on Profile | Browse the built-in profiles |
| Up / Down | Select a row |
| Left / Right | Change the selected row |
| Enter | Save and keep Quick Settings open |
| Escape or Ctrl+Shift+S | Close and restore the last saved settings |

While the panel is open, no key, text input, or mouse click reaches the shell.
The rules for saving are in [configuration](configuration.md#settings-panel).

## Known limits

- Linux with native Wayland only. Only x86_64 is tested.
- Clipboard copy and paste need `ext-data-control` or `wlr-data-control`.
- Key bindings and the ANSI palette use built-in values.
- retro-term does not watch the configuration file. Press Ctrl+Shift+R or save in the settings panel.
- Scrollback and shell settings apply to new panes and tabs only.
- Ctrl+Shift+W closes a pane without confirmation.
- Performance against the 1.40× Alacritty phase limit is unverified for static CRT, configuration,
  tabs and splits, and the settings panel. See [performance](performance.md).
- The interactive IME check and the live smoke checks of tabs, splits, and the settings panel are still pending.
