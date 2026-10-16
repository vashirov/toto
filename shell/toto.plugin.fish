#!/usr/bin/env fish
# toto shell integration for Fish
# Usage: eval "$(toto init fish)"

# Ctrl-T: fuzzy trick picker
function _toto_tricks
    set -l output (toto --print 2>/dev/tty)
    if test -n "$output"
        commandline -r $output
    end
    commandline -f repaint
end

bind \ct _toto_tricks
