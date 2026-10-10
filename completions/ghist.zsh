#compdef ghist

_ghist_revisions() {
    local refs_output format='%(refname:short)'
    local -a refs

    git rev-parse --git-dir >/dev/null 2>&1 || return 1
    if [[ $PREFIX == *...* ]]; then
        compset -P '*...'
    elif [[ $PREFIX == *..* ]]; then
        compset -P '*..'
    elif [[ $PREFIX == \^* ]]; then
        compset -P '\^'
    fi
    [[ $PREFIX == refs/* ]] && format='%(refname)'
    refs_output=$(git for-each-ref --format="$format") || return
    refs=(HEAD "${(@f)refs_output}")
    _describe -t revisions revision refs
}

_ghist() {
    local context state state_descr line
    local -A opt_args

    _arguments -s -S \
        '*-p[Show patches]' \
        '*--patch[Show patches]' \
        '*--stat[Show diff statistics]' \
        '(- *)-h[Show help]' \
        '(- *)--help[Show help]' \
        '(- *)--version[Show version]' \
        '(- *)--completions[Print shell completions]:shell:(bash zsh)' \
        '*:revision or path:->operand'

    if [[ $state == operand ]]; then
        if (( ${words[1,CURRENT-1][(I)--]} )); then
            _files
        elif [[ $PREFIX == \^* || $PREFIX == *..* ]]; then
            _ghist_revisions
        else
            _alternative 'revisions:revision:_ghist_revisions' 'files:path:_files'
        fi
    fi
}

if [[ $funcstack[1] == _ghist ]]; then
    _ghist "$@"
else
    compdef _ghist ghist
fi
