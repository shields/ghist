_ghist() {
    local cur=${COMP_WORDS[COMP_CWORD]} word candidate refs prefix='' ref quote=''
    local format='%(refname:short)'
    local paths=false i
    COMPREPLY=()

    case $cur in
        \"*|\'*) quote=${cur:0:1}; cur=${cur:1} ;;
    esac

    for ((i = 1; i < COMP_CWORD; i++)); do
        word=${COMP_WORDS[i]}
        if [[ $word == -- ]]; then
            paths=true
            break
        fi
    done

    if ! $paths; then
        if [[ ${COMP_WORDS[COMP_CWORD-1]} == --completions ]]; then
            for candidate in bash zsh; do
                [[ $candidate == "$cur"* ]] && COMPREPLY+=("$candidate")
            done
            return 0
        fi
        if [[ $cur == -* ]]; then
            for candidate in -p --patch --stat -h --help --version --completions --; do
                [[ $candidate == "$cur"* ]] && COMPREPLY+=("$candidate")
            done
            return 0
        fi
    fi

    case $cur in
        *...*) prefix=${cur%...*}... ;;
        *..*) prefix=${cur%..*}.. ;;
        ^*) prefix=^ ;;
    esac

    $paths && return 0

    git rev-parse --git-dir >/dev/null 2>&1 || return 0
    ref=${cur#"$prefix"}
    [[ $ref == refs/* ]] && format='%(refname)'
    refs=$(git for-each-ref --format="$format") || return
    while IFS= read -r candidate; do
        if [[ -n $candidate && $candidate == "$ref"* ]]; then
            candidate=$prefix$candidate
            case $quote in
                "'") candidate=${candidate//\'/\'\\\'\'} ;;
                '"')
                    candidate=${candidate//\\/\\\\}
                    candidate=${candidate//\"/\\\"}
                    candidate=${candidate//\$/\\\$}
                    candidate=${candidate//\`/\\\`}
                    ;;
                *) printf -v candidate '%q' "$candidate" ;;
            esac
            COMPREPLY+=("$candidate")
        fi
    done <<< "HEAD
$refs"
    return 0
}

complete -o default -F _ghist ghist
