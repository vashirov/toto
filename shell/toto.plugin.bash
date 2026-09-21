#!/usr/bin/env bash
# toto shell integration for Bash
# Usage: eval "$(toto widget bash)"

# Ctrl-T: fuzzy trick picker
_toto_tricks() {
    local output
    output="$(toto --print 2>/dev/tty)"
    if [ -n "$output" ]; then
        READLINE_LINE="$output"
        READLINE_POINT=${#output}
    fi
}

bind -x '"\C-t": _toto_tricks'
