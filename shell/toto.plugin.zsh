#!/usr/bin/env zsh
# toto shell integration for Zsh
# Usage: eval "$(toto widget zsh)"

# Ctrl-T: fuzzy trick picker
_toto_tricks() {
    local output
    output="$(toto --print 2>/dev/tty)"
    if [[ -n "$output" ]]; then
        LBUFFER="$output"
        RBUFFER=""
    fi
    zle reset-prompt
}
zle -N _toto_tricks
bindkey '^T' _toto_tricks
