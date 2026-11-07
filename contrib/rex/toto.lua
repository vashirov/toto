-- toto integration for Rex.
--
-- Install toto-rex-pick beside this file and add `require("toto")` to
-- init.lua in the same directory. Then reload the configuration:
--
--   rex config check && rex config reload
--
-- Press Ctrl-T in a terminal to open toto in a floating pane. The selected
-- trick is typed into the focused terminal with bracketed paste, ready to be
-- reviewed or edited, but not run.

local function config_dir()
  local base = os.getenv("XDG_CONFIG_HOME")
  if base == nil or base == "" then
    base = (os.getenv("HOME") or "") .. "/.config"
  end
  return base .. "/rex"
end

local picker = os.getenv("TOTO_REX_PICK") or (config_dir() .. "/toto-rex-pick")

rex.action{
  name = "toto",
  title = "Toto: Pick a Trick",
  description = "Open toto in a floating pane and insert the selected trick into the focused terminal.",
  category = "Toto",
  keywords = "tricks snippets fuzzy finder",
  args = {
    query = "string?",
    path = "string?",
  },
  run = function(ctx, args)
    if ctx.session_id == nil or ctx.block_id == nil then
      error("toto: no terminal is focused", 0)
    end

    -- Place the popup in the target terminal's window. If toto is already
    -- open, do nothing rather than stacking another picker.
    local described = rex.session.describe_block{ block_id = ctx.block_id }
    local window_id = nil
    if described ~= nil and described.block ~= nil then
      if described.block.label == "toto" then
        return
      end
      window_id = described.block.window_id
    end

    -- The wrapper runs toto, then sends the selection to the target block.
    local command = { picker, ctx.session_id, ctx.block_id }
    if args.query ~= nil then
      table.insert(command, "--query")
      table.insert(command, args.query)
    end
    if args.path ~= nil then
      table.insert(command, "--path")
      table.insert(command, args.path)
    end

    local layer, err = rex.session.new_layer{
      session_id = ctx.session_id,
      window_id = window_id,
      bounds = { 0.08, 0.08, 0.84, 0.84 },
      focus = true,
      layout = rex.layout.block{
        flavor = "com.superlogical.terminal.shell",
        label = "toto",
        options = {
          command = command,
          -- Close after a successful pick or a cancel. Leave the pane open on
          -- failure long enough to read the error.
          exit = { on_completion = true, quick_exit_threshold_ms = 600000 },
        },
      },
    }
    if err ~= nil then
      error(err, 0)
    end

    return { layer_id = layer.layer_id, block_id = layer.block_ids[1] }
  end,
}

rex.bind("ctrl+t", "toto")
