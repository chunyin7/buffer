# Keymaps

How `buffer-modal` turns key presses into commands. The decisions behind it:
[0018](../decisions/0018-vim-helix-presets.md) (Vim and Helix presets, keymaps as data) and
[0033](../decisions/0033-strict-keymap-trees.md) (keymaps are strict trees).

Status: keys and keymaps exist. Modes, dispatch (pending keys, counts, registers), the command
registry, the TOML format, and the Vim and Helix presets land later in M1.

## Keys

A `Key` is a `KeyCode` and the `Modifiers` held with it: Ctrl, Alt, Shift, and Super (Command on
macOS).

- Keys are logical: `KeyCode::Char` holds the char the keyboard layout typed, so a binding for
  `ö` works on any layout that types it. Space is `Char(' ')`.
- The other codes are Esc, Enter, Tab, Backspace, Delete, Insert, the four arrows, Home, End,
  PageUp, and PageDown.
- Shift is part of a char: Shift+a is `A`. `Key::new` drops Shift from char keys, so a press is
  one key whether or not the platform reports Shift with it. Other keys keep it: Shift+Tab is not
  Tab.
- `buffer-ui` turns egui's key events into keys; `buffer-modal` sees no egui type.

## Keymaps

A `Keymap` binds key sequences to command names, as a tree: each key leads to a command or to a
`Prefix`, the keys that may come next. A sequence is bound to a command or starts longer
sequences, never both, so dispatch never waits to see whether another key comes.

`bind(keys, command)` replaces whatever `keys` overlap:

| Before | After `bind(keys, command)` |
|---|---|
| `keys` unbound, or bound to a command | `keys` bound to `command` |
| `keys` start longer sequences | `keys` bound to `command`; the longer sequences unbound |
| `keys` start with a sequence bound to a command | that command unbound; its sequence starts `keys` |

Presets and user config bind in turn, so the later binding wins: that is how config overrides a
preset.

- `get(keys)` returns `Binding::Command(name)`, `Binding::Prefix(prefix)`, or `None` when no
  binding starts with `keys`. A command ends its sequence, so keys past it find nothing. The
  empty sequence is the root prefix.
- `Prefix::bindings` lists the keys that may come next, each with its binding, in the order they
  were first bound; rebinding a key keeps its place. Pending-key hints list them so, in the order
  a preset defines them.
- A prefix is a list scanned in order, which keeps that order without a second index.
