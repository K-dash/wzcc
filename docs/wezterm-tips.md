# WezTerm Configuration Tips for wzcc

This guide provides recommended WezTerm configurations to enhance your wzcc experience.

## Table of Contents

- [Floating Window Launcher](#floating-window-launcher)
- [Cross-Workspace Navigation](#cross-workspace-navigation)
- [Split Pane Launcher](#split-pane-launcher)
- [Complete Example](#complete-example)

## Floating Window Launcher

Launch wzcc as a floating window with a keybinding. This creates a centered, semi-transparent window that can be toggled on/off.

### Features

- **Toggle behavior**: Press once to open, press again to close
- **Centered positioning**: Opens at 70% of screen size, centered
- **Semi-transparent**: 65% opacity for overlay effect
- **Always on top**: Stays visible above other windows (macOS only)

### Configuration

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

-- Add to your keys table
{
  key = 'm',
  mods = 'LEADER',
  action = wezterm.action_callback(function(window, pane)
    local mux = wezterm.mux
    -- Update this path to your wzcc installation
    local wzcc_cwd = wezterm.home_dir .. '/path/to/wzcc'

    -- Helper function to identify wzcc window
    local function is_wzcc_session(p)
      local title = p:get_title()
      local cwd = p:get_current_working_dir()
      if not title or not cwd then
        return false
      end
      local cwd_str = cwd.file_path or tostring(cwd)
      return title:find('wzcc') and cwd_str:find(wzcc_cwd, 1, true)
    end

    -- Close if current pane is wzcc
    if is_wzcc_session(pane) then
      window:perform_action(act.CloseCurrentTab { confirm = false }, pane)
      return
    end

    -- Find and close existing wzcc window
    for _, w in ipairs(mux.all_windows()) do
      for _, t in ipairs(w:tabs()) do
        local active_pane = t:active_pane()
        if is_wzcc_session(active_pane) then
          w:gui_window():perform_action(act.CloseCurrentTab { confirm = false }, active_pane)
          return
        end
      end
    end

    -- Get active screen info (multi-monitor support)
    local screen = wezterm.gui.screens().active
    local ratio = 0.7

    -- Calculate 70% size
    local new_width = math.floor(screen.width * ratio)
    local new_height = math.floor(screen.height * ratio)

    -- Center the window
    local x = math.floor((screen.width - new_width) / 2) + screen.x
    local y = math.floor((screen.height - new_height) / 2) + screen.y

    -- Spawn new window
    local tab, new_pane, new_window = wezterm.mux.spawn_window {
      cwd = wzcc_cwd,
      position = {
        x = x,
        y = y,
        origin = 'ActiveScreen',
      },
    }

    -- Set window size (spawn_window width/height doesn't work on macOS)
    new_window:gui_window():set_inner_size(new_width, new_height)

    -- Make semi-transparent
    new_window:gui_window():set_config_overrides({
      window_background_opacity = 0.65,
    })

    -- Launch wzcc (update path as needed)
    new_pane:send_text(wzcc_cwd .. '/target/release/wzcc tui\n')

    -- Always on top (macOS only)
    new_window:gui_window():perform_action(act.ToggleAlwaysOnTop, new_pane)
  end),
},
```

## Cross-Workspace Navigation

When you jump to a session in a different workspace, wzcc can follow you there. This requires two parts:

### Part 1: Workspace Switcher Event Handler

This handles the `wzcc_switch_workspace` user variable and moves the wzcc window to the target workspace.

```lua
local wezterm_wzcc = require 'wezterm'

wezterm_wzcc.on('user-var-changed', function(window, pane, name, value)
  if name == 'wzcc_switch_workspace' and value and value ~= '' then
    local target_workspace = value
    -- Update this path to your wzcc installation
    local wzcc_cwd = wezterm_wzcc.home_dir .. '/path/to/wzcc'

    -- Find wzcc window and move it to target workspace
    local mux = wezterm_wzcc.mux
    for _, w in ipairs(mux.all_windows()) do
      for _, t in ipairs(w:tabs()) do
        local active_pane = t:active_pane()
        local title = active_pane:get_title()
        local cwd = active_pane:get_current_working_dir()
        if title and cwd then
          local cwd_str = cwd.file_path or tostring(cwd)
          if title:find('wzcc') and cwd_str:find(wzcc_cwd, 1, true) then
            -- Move wzcc window to target workspace
            w:set_workspace(target_workspace)
            break
          end
        end
      end
    end

    -- Switch to target workspace
    window:perform_action(
      wezterm_wzcc.action.SwitchToWorkspace { name = target_workspace },
      pane
    )
  end
end)
```

### Part 2: Install the Workspace Switcher (Alternative)

If you prefer automatic setup, run:

```bash
wzcc install-workspace-switcher
# Or install all components:
wzcc install
```

This adds the basic workspace switcher to your `wezterm.lua`. However, for the wzcc window to follow you across workspaces, you'll need to manually add the enhanced version above.

## Split Pane Launcher

Launch wzcc in a split pane instead of a floating window. Useful when you want wzcc alongside your current work.

```lua
{
  key = 'M',
  mods = 'LEADER|SHIFT',
  action = wezterm.action_callback(function(window, pane)
    local new_pane = pane:split {
      direction = 'Right',
      size = 0.4,
      -- Update this path to your wzcc installation
      cwd = wezterm.home_dir .. '/path/to/wzcc',
    }
    new_pane:send_text(wezterm.home_dir .. '/path/to/wzcc/target/release/wzcc tui\n')
  end),
},
```

## Complete Example

Here's a complete example combining all wzcc-related configurations:

```lua
local wezterm = require 'wezterm'
local act = wezterm.action
local config = wezterm.config_builder()

-- ============================================
-- wzcc Configuration
-- ============================================

-- Update this to your wzcc installation path
local WZCC_PATH = wezterm.home_dir .. '/path/to/wzcc'
local WZCC_BINARY = WZCC_PATH .. '/target/release/wzcc'

-- Helper function to identify wzcc windows
local function is_wzcc_session(p, wzcc_cwd)
  local title = p:get_title()
  local cwd = p:get_current_working_dir()
  if not title or not cwd then
    return false
  end
  local cwd_str = cwd.file_path or tostring(cwd)
  return title:find('wzcc') and cwd_str:find(wzcc_cwd, 1, true)
end

-- Cross-workspace navigation: move wzcc window when switching workspaces
wezterm.on('user-var-changed', function(window, pane, name, value)
  if name == 'wzcc_switch_workspace' and value and value ~= '' then
    local target_workspace = value

    -- Find and move wzcc window
    for _, w in ipairs(wezterm.mux.all_windows()) do
      for _, t in ipairs(w:tabs()) do
        local active_pane = t:active_pane()
        if is_wzcc_session(active_pane, WZCC_PATH) then
          w:set_workspace(target_workspace)
          break
        end
      end
    end

    -- Switch to target workspace
    window:perform_action(
      act.SwitchToWorkspace { name = target_workspace },
      pane
    )
  end
end)

-- Keybindings
config.keys = {
  -- Leader + m: Toggle floating wzcc window
  {
    key = 'm',
    mods = 'LEADER',
    action = wezterm.action_callback(function(window, pane)
      local mux = wezterm.mux

      -- Close if current pane is wzcc
      if is_wzcc_session(pane, WZCC_PATH) then
        window:perform_action(act.CloseCurrentTab { confirm = false }, pane)
        return
      end

      -- Find and close existing wzcc window
      for _, w in ipairs(mux.all_windows()) do
        for _, t in ipairs(w:tabs()) do
          if is_wzcc_session(t:active_pane(), WZCC_PATH) then
            w:gui_window():perform_action(act.CloseCurrentTab { confirm = false }, t:active_pane())
            return
          end
        end
      end

      -- Spawn centered floating window
      local screen = wezterm.gui.screens().active
      local ratio = 0.7
      local new_width = math.floor(screen.width * ratio)
      local new_height = math.floor(screen.height * ratio)
      local x = math.floor((screen.width - new_width) / 2) + screen.x
      local y = math.floor((screen.height - new_height) / 2) + screen.y

      local tab, new_pane, new_window = mux.spawn_window {
        cwd = WZCC_PATH,
        position = { x = x, y = y, origin = 'ActiveScreen' },
      }

      new_window:gui_window():set_inner_size(new_width, new_height)
      new_window:gui_window():set_config_overrides({
        window_background_opacity = 0.65,
      })
      new_pane:send_text(WZCC_BINARY .. ' tui\n')
      new_window:gui_window():perform_action(act.ToggleAlwaysOnTop, new_pane)
    end),
  },

  -- Leader + M: Open wzcc in split pane
  {
    key = 'M',
    mods = 'LEADER|SHIFT',
    action = wezterm.action_callback(function(window, pane)
      local new_pane = pane:split {
        direction = 'Right',
        size = 0.4,
        cwd = WZCC_PATH,
      }
      new_pane:send_text(WZCC_BINARY .. ' tui\n')
    end),
  },
}

return config
```

