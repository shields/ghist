zmodload zsh/zpty
zmodload zsh/zselect

case $1 in
    bash|bash-system)
        bash_command=bash
        [[ $1 == bash-system ]] && bash_command=/bin/bash
        zpty -b shell "$bash_command" --noprofile --norc
        setup="PS1='' PS2=''; source ${(q)2}; printf '\\nGHIST_%s\\n' READY"
        ;;
    zsh|zsh-autoload)
        zpty -b shell zsh -df
        if [[ $1 == zsh-autoload ]]; then
            load="fpath=(${(q)2:h} \$fpath); autoload -Uz _ghist; compdef _ghist ghist"
        else
            load="source ${(q)2}"
        fi
        setup="PS1='' PS2='' RPS1=''; autoload -Uz compinit; compinit -i -D; $load; print GHIST_'READY'"
        ;;
    *) exit 2 ;;
esac

pty_fd=$REPLY
wait_marker() {
    local output collected='' deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
        zselect -t 10 -r $pty_fd || continue
        while zpty -r shell output; do
            collected+=$output
        done
        [[ $collected == *$1* ]] && return 0
    done
    print -ru2 -- "Timed out: $collected"
    return 1
}

zpty -w shell "$setup"
if wait_marker GHIST_READY; then
    zpty -w -n shell "$3"$'\t\001'"printf '%s\\0' "$'\005'" > ${(q)4}; printf '\\nGHIST_%s\\n' DONE"$'\n'
    if wait_marker GHIST_DONE; then
        zpty -d shell
        cat -- "$4"
        exit
    fi
fi
zpty -d shell
exit 1
