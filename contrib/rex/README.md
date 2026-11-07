# toto for Rex

This integration opens toto in a floating Rex pane and inserts the selected
trick into the terminal you started from. The command is left on the shell
line, so you can review or edit it before running it.

## Install

Copy the two files into your Rex configuration directory and load the action:

```sh
mkdir -p ~/.config/rex
cp toto.lua toto-rex-pick ~/.config/rex/
chmod +x ~/.config/rex/toto-rex-pick
printf 'require("toto")\n' >> ~/.config/rex/init.lua
```

If `XDG_CONFIG_HOME` is set, use `$XDG_CONFIG_HOME/rex` instead of
`~/.config/rex`.

Check and reload the configuration:

```sh
rex config check
rex config reload
```

Rex must find `toto` and `rex` on the login shell's PATH. To use another
binary, set `TOTO_BIN` or `REX_BIN` in the Rex server's environment.

## Use

Press <kbd>Ctrl</kbd>+<kbd>T</kbd> at a shell prompt in a Rex terminal. Toto
opens in a floating pane. Select a trick and press Enter; the command appears
on the shell line. Pressing Esc cancels without changing the prompt.

The binding is set in `toto.lua`. Change `rex.bind("ctrl+t", "toto")` if the
key is already used, then reload the configuration. A shortcut set in the Rex
app itself takes precedence over the configuration file; check the app's
**Change Shortcut...** command if Ctrl-T does nothing.

Run the action with arguments from the CLI or another action:

```sh
rex do toto query=git
rex do toto path=$HOME/src/my-tricks
```

Both arguments are optional:

| Argument | Effect                                  |
| -------- | --------------------------------------- |
| `query`  | Starts toto with this search query.     |
| `path`   | Uses these colon-separated trick paths. |

## How it works

`toto.lua` creates a floating terminal block that runs `toto-rex-pick` with the
focused session and block IDs. The wrapper runs `toto --print`, captures the
selection from stdout, and uses `rex send` to type it into the target block.
When the target terminal has bracketed paste enabled, the wrapper pastes the
command so multi-line tricks stay in one editable buffer. Without bracketed
paste, the text is typed directly and a multi-line trick may run line by line.
The floating pane closes after a successful pick or a cancel.
