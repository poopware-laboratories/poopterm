# >>> poopterm shell integration >>>
# Source this file from ~/.zshrc to enable OSC 133 and OSC 7 integration.
if (( $+functions[_poopterm_precmd] )); then
	return
fi

autoload -Uz add-zsh-hook add-zle-hook-widget

# Percent-encodes every byte of $1 outside the URI-unreserved set into
# REPLY, so control characters cannot end an OSC sequence early. Bytes are
# read as numbers, not through the locale: musl's C locale maps 0x80-0xFF
# to U+DF80-U+DFFF, so printf "'c" turned é into %DFC3%DFA9 there.
_poopterm_url_encode() {
	setopt local_options no_multibyte
	local LC_ALL=C char hex
	REPLY=
	for char in ${(s::)1}; do
		case $char in
			([a-zA-Z0-9/._~-]) REPLY+=$char ;;
			(*) printf -v hex '%%%02X' $(( #char & 255 )); REPLY+=$hex ;;
		esac
	done
}

_poopterm_precmd() {
	# `status` is read-only and `path` is tied to $PATH in zsh; use other names.
	local exit_code=$? REPLY
	print -rn -- $'\e]133;D;'"$exit_code"$'\a'
	# Clicks in the input line may move the cursor with arrow keys.
	print -rn -- $'\e]133;A;cl=line\a'
	_poopterm_url_encode "$PWD"
	# Naming the host keeps ssh sessions' directories apart from local ones.
	print -rn -- $'\e]7;file://'"$HOST$REPLY"$'\a'
}

_poopterm_preexec() {
	local REPLY
	_poopterm_url_encode "$1"
	print -rn -- $'\e]133;C;cmdline_url='"$REPLY"$'\a'
}

_poopterm_line_init() {
	print -rn -- $'\e]133;B\a'
}

# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if [[ $TERM == xterm-poopterm && -z ${POOPTERM_SSH_KEEP_TERM-} ]] && (( ! $+functions[ssh] )); then
	ssh() { TERM=xterm-256color command ssh "$@"; }
fi

# Continuation prompts are prompts too; zle-line-init ends them with B.
PS2=$'%{\e]133;A;k=s\a%}'"$PS2"
add-zsh-hook precmd _poopterm_precmd
add-zsh-hook preexec _poopterm_preexec
add-zle-hook-widget zle-line-init _poopterm_line_init
# <<< poopterm shell integration <<<
