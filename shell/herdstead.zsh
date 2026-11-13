# Loaded after the user's zshrc so prompt frameworks and their hooks stay intact.
[[ -o interactive && -z ${_HERDSTEAD_ZSH_LOADED:-} ]] || return
typeset -g _HERDSTEAD_ZSH_LOADED=1
: ${HERDSTEAD_EXECUTABLE:=${commands[herdstead]:-herdstead}}
autoload -Uz add-zsh-hook

# OSC 7 keeps new panes and relative links in the shell's current directory.
_herdstead_cwd() {
  local url=${PWD//\%/%25}
  url=${url// /%20}
  url=${url//\#/%23}
  url=${url//$'\n'/%0A}
  url=${url//$'\r'/%0D}
  url=${url//$'\e'/%1B}
  url=${url//$'\a'/%07}
  printf '\e]7;file://localhost%s\a' "$url"
}
add-zsh-hook chpwd _herdstead_cwd
add-zsh-hook precmd _herdstead_cwd

function hcat() { "$HERDSTEAD_EXECUTABLE" preview -- "$@" }
function hmd() {
  if (( $# )); then "$HERDSTEAD_EXECUTABLE" md "$@"
  else "$HERDSTEAD_EXECUTABLE" md - --stdin
  fi
}
if (( ! $+aliases[md] && ! $+functions[md] )); then
  function md() { hmd "$@" }
fi

# Only interactive, single-file reads become rich output; pipes keep exact bytes.
if (( ! $+aliases[cat] && ! $+functions[cat] )); then
  function cat() {
    if [[ -t 1 && $# == 1 && -f $1 && $1 != -* ]]; then
      # One blank row separates the command line from the rendered document.
      printf '\n'
      hcat "$1"
    else command cat "$@"
    fi
  }
fi
if (( ! $+aliases[tree] && ! $+functions[tree] )); then
  function tree() { "$HERDSTEAD_EXECUTABLE" tree "$@" }
fi
