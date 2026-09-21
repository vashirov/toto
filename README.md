# toto

Toto is a fuzzy finder for your shell tricks 🐾

## Install

Install toto with Cargo:

```sh
cargo install --path .
```

## Try toto

From the toto repository, run the example tricks:

```sh
toto --path ./examples
```

Type a word to filter the list. Press Enter to run the selected command. Press Esc to cancel.

You can also start with a search query:

```sh
toto --path ./examples --query git
```

Print a command instead of running it:

```sh
toto --path ./examples --print
```

Select the best match without opening the picker:

```sh
toto --path ./examples --best-match --query podman
```

## Add your first trick

Toto reads `.trick.md` files. The default directory is `~/.local/share/toto/tricks`.

Create the directory and a file:

```sh
mkdir -p ~/.local/share/toto/tricks
$EDITOR ~/.local/share/toto/tricks/common.trick.md
```

Add this content:

````markdown
# git

## Show recent commits

```sh
git log --oneline -10
```
````

Run toto:

```sh
toto
```

## Use toto every day

Use these commands for common tasks:

```sh
# Open the picker
toto

# Start with a query
toto --query podman

# Print the selected command
toto --print

# Select the best match
toto --best-match --query ping

# Include only selected tags and exclude other tags
toto --tag-rules "git,!checkout"

# Use extra trick directories
toto --path ~/tricks:/opt/shared-tricks
```

Toto finds trick files from these locations, in this order:

1. The `--path` value.
2. The `TOTO_PATH` environment variable.
3. The `paths.tricks` value in the configuration file.
4. `~/.local/share/toto/tricks`, when the directory exists.

## Add a shell shortcut

Toto can insert a selected command into your current shell line. Add one command to your shell startup file.

For Bash, add this line to `~/.bashrc`:

```sh
eval "$(toto init bash)"
```

For Zsh, add this line to `~/.zshrc`:

```sh
eval "$(toto init zsh)"
```

Open a new shell. Press Ctrl-T to open toto. The selected command appears on the shell line.

## Use toto with tmux

Add this binding to `~/.tmux.conf`:

```tmux
bind-key -T prefix C-f display-popup -b rounded -w 100% -h 70% -E \
  "tmux send-keys \"$(toto --print)\""
```

Reload the tmux configuration. Press the prefix key and Ctrl-F to open toto in a popup.

## Write trick files

A trick file uses Markdown. toto reads the headings, prose, and fenced code blocks.

````markdown
# network

## Ping a host

Send ICMP echo requests to test connectivity.

```sh
ping -c <count> <host>
```

$ host: echo -e "8.8.8.8\n1.1.1.1\nlocalhost"
$ count: echo -e "4\n10\n100"
````

The file uses these elements:

| Element     | Syntax                    | Result                                    |
| ----------- | ------------------------- | ----------------------------------------- |
| Tag         | `# heading`               | Sets the category.                        |
| Description | `## heading`              | Names the trick. toto searches this text. |
| Detail      | Prose before a code block | Shows an explanation in the preview.      |
| Command     | Fenced code block         | Defines the command that toto runs.       |
| Variable    | `$ name: command`         | Provides values for `<name>`.             |
| Dependency  | `@ other_tags`            | Reuses variables from another tag.        |
| Metacomment | `; text`                  | Adds text that toto ignores.              |

Add a language after the opening fence for editor and GitHub support. toto accepts values such as `sh`, `bash`, `python`, `sql`, `powershell`, `cmd`, and `vim`.

## Use variables

Write a variable as `<name>` in a command:

````markdown
## Stop a container

```sh
podman stop <container>
```

$ container: podman ps --format '{{.Names}}' --- --prevent-extra
````

When toto selects this trick, it runs the suggestion command. You select one value from its output.

If a command has no suggestion line, toto asks you to type the value.

Use `\` to continue a suggestion command on the next line:

```markdown
$ value: echo "foo" \
| tr 'f' 'b'
```

Add options after `---`:

| Option            | Result                                                           |
| ----------------- | ---------------------------------------------------------------- |
| `--column N`      | Takes column `N` from each output line. The first column is `1`. |
| `--delimiter SEP` | Uses `SEP` to split columns. The default is whitespace.          |
| `--prevent-extra` | Allows only values from the suggestion list.                     |
| `--query TEXT`    | Starts the variable picker with `TEXT`.                          |
| `--filter TEXT`   | Selects the first line that contains `TEXT`.                     |
| `--expand`        | Expands each selected line into a separate argument.             |
| `--map CMD`       | Transforms the selected value with `CMD`.                        |

## Reuse variables

Use `@` to inherit variables from another tag section:

````markdown
# common

$ host: echo -e "server1\nserver2"

# deployment

@ common

## Deploy to host

```sh
rsync -avz ./build/ <host>:/opt/app/
```
````

The `deployment` section can use `<host>` from the `common` section.

## Use front matter

Add a YAML block at the first line of a trick file:

````markdown
---
tags: [git, vcs]
depend: common
---

## Cherry-pick a commit

```sh
git cherry-pick <hash>
```
````

The supported keys are:

| Key      | Result                                                                       |
| -------- | ---------------------------------------------------------------------------- |
| `tags`   | Sets comma-separated tags. This has the same effect as an H1 heading.        |
| `depend` | Sets comma-separated tag dependencies. This has the same effect as `@ tags`. |

## Configure toto

The default configuration file is `~/.config/toto/config.toml`.

Create an annotated file:

```sh
mkdir -p ~/.config/toto
toto info sample-config > ~/.config/toto/config.toml
```

A configuration file can contain these values:

```toml
[paths]
tricks = ["~/.local/share/toto/tricks"]

[shell]
command = "bash"

[picker]
preview_height = 7

[history]
enabled = true

[keys]
tricks = "\\C-t"
```

Show configuration paths and the sample file:

```sh
toto info config-path
toto info tricks-path
toto info sample-config
```

Use these environment variables:

| Variable             | Result                                                                   |
| -------------------- | ------------------------------------------------------------------------ |
| `TOTO_PATH`          | Sets colon-separated trick paths. It overrides the configuration file.   |
| `TOTO_CONFIG`        | Sets the path to the configuration file.                                 |
| `EDITOR` or `VISUAL` | Sets the editor for Ctrl-E. toto uses `vi` when neither variable is set. |

## Picker keys

| Key                  | Result                                                          |
| -------------------- | --------------------------------------------------------------- |
| Type text            | Filters tags, descriptions, and commands.                       |
| Up or Ctrl-P         | Moves the selection up.                                         |
| Down or Ctrl-N       | Moves the selection down.                                       |
| Page Up or Page Down | Moves the selection by one page.                                |
| Enter                | Selects the item. It runs the command unless you use `--print`. |
| Ctrl-E               | Opens the selected trick in `EDITOR` and reloads it.            |
| Esc or Ctrl-C        | Cancels the picker.                                             |
| Left or Right        | Moves the query cursor.                                         |
| Home or End          | Moves the query cursor to the start or end.                     |
| Ctrl-A               | Moves the query cursor to the start.                            |
| Ctrl-U               | Clears the query.                                               |
| Ctrl-W               | Deletes the previous word.                                      |
| Ctrl-K               | Deletes the query after the cursor.                             |
| Backspace or Delete  | Deletes one character.                                          |

## Usage history

Toto puts often-used tricks first. It stores selection counts in `~/.local/share/toto/usage.toml`.

Set `enabled = false` under `[history]` to disable this feature.

## License

Apache-2.0.
