# Loaded after the user's zshrc so prompt frameworks and their hooks stay intact.
[[ -o interactive && -z ${_HERDSTEAD_ZSH_LOADED:-} ]] || return
typeset -g _HERDSTEAD_ZSH_LOADED=1
: ${HERDSTEAD_EXECUTABLE:=${commands[herdstead]:-herdstead}}
autoload -Uz compinit add-zle-hook-widget add-zsh-hook is-at-least
(( $+functions[compdef] )) || compinit -D -i
zmodload zsh/complist 2>/dev/null

_herdstead_candidates() { local curcontext="${WIDGET}:::"; _main_complete _herdstead_files }
_herdstead_files() {
  compset -P '@' || return 1
  _files
}
zle -C _herdstead_complete complete-word _herdstead_candidates
zstyle ':completion:_herdstead_complete:*' menu select

# Preserve the user's normal completion widget outside a reference.
for _herdstead_map in main emacs viins; do
  _herdstead_binding=( ${(z)$(bindkey -M $_herdstead_map '^I')} )
  if [[ -n ${_herdstead_binding[2]} ]]; then
    zle -A "${_herdstead_binding[2]}" "_herdstead_tab_${_herdstead_map}" 2>/dev/null
  fi
  _herdstead_binding=( ${(z)$(bindkey -M $_herdstead_map '^M')} )
  if [[ -n ${_herdstead_binding[2]} ]]; then
    zle -A "${_herdstead_binding[2]}" "_herdstead_enter_${_herdstead_map}" 2>/dev/null
  fi
  _herdstead_binding=( ${(z)$(bindkey -M $_herdstead_map '^J')} )
  if [[ ${_herdstead_binding[2]} == (accept-line|.accept-line) ]]; then
    zle -A "${_herdstead_binding[2]}" "_herdstead_lf_${_herdstead_map}"
  fi
done
for _herdstead_map in main emacs viins; do
  bindkey -M $_herdstead_map '^I' _herdstead_tab
  bindkey -M $_herdstead_map '^M' _herdstead_accept
  (( $+widgets[_herdstead_lf_${_herdstead_map}] )) && bindkey -M $_herdstead_map '^J' _herdstead_accept_lf
done
unset _herdstead_map _herdstead_binding

_herdstead_tab() {
  if [[ $LBUFFER =~ '(^|[[:space:]])@("[^"]*|([^[:space:]\\]|\\.)*)$' ]]; then
    zle _herdstead_complete
  else
    zle "_herdstead_tab_${KEYMAP}" 2>/dev/null || zle .expand-or-complete
  fi
}
zle -N _herdstead_tab

_herdstead_expand_buffer() {
  if [[ $BUFFER == *'@'* ]]; then
    local expanded
    expanded=$(print -rn -- "$BUFFER" | "$HERDSTEAD_EXECUTABLE" shell-expand 2>&1)
    if (( $? != 0 )); then
      zle -M -- "$expanded"
      return 1
    fi
    BUFFER=$expanded
    CURSOR=${#BUFFER}
  fi
  return 0
}
_herdstead_accept() {
  _herdstead_expand_buffer || return 1
  zle "_herdstead_enter_${KEYMAP}" 2>/dev/null || zle .accept-line
}
# Cooked TTY input can translate an early Return to LF before ZLE starts.
_herdstead_accept_lf() {
  _herdstead_expand_buffer || return 1
  zle "_herdstead_lf_${KEYMAP}" 2>/dev/null || zle .accept-line
}
zle -N _herdstead_accept
zle -N _herdstead_accept_lf

_herdstead_references() {
  emulate -L zsh
  setopt extendedglob
  # zsh 5.9 added memo tags, which let us coexist with syntax-highlighting plugins.
  is-at-least 5.9 || return
  region_highlight=( ${region_highlight:#*memo=herdstead} )
  local rest=$BUFFER token reference_path colour
  local MATCH MBEGIN MEND
  local -a match mbegin mend
  local -i offset=0 start end
  local expression='(^|[[:space:]])(@("[^"]*"|([^[:space:];|&<>()\\]|\\.)+))'
  while [[ $rest =~ $expression ]]; do
    token=$match[2]
    start=$(( offset + MBEGIN - 1 + ${#match[1]} ))
    end=$(( start + ${#token} ))
    reference_path=${(Q)${token#@}}
    [[ -e $reference_path ]] && colour='fg=cyan,bold,underline' || colour='fg=red,underline'
    region_highlight+=( "$start $end $colour memo=herdstead" )
    offset=$(( offset + MEND ))
    rest=${rest[$(( MEND + 1 )),-1]}
  done
}
zle -N _herdstead_references
add-zle-hook-widget line-pre-redraw _herdstead_references

_herdstead_suggestions() {
  emulate -L zsh
  [[ ${_herdstead_last_query:-} == "$PWD:$LBUFFER" ]] && return
  typeset -g _herdstead_last_query="$PWD:$LBUFFER"
  local query entry MATCH MBEGIN MEND
  local -a match mbegin mend
  local -a matches labels
  if [[ $LBUFFER =~ '(^|[[:space:]])@(([^[:space:]\\]|\\.)*)$' ]]; then
    query=${(Q)match[2]}
    [[ $query == '~/'* ]] && query=$HOME/${query#\~/}
    matches=( ${query}*(NY8) )
    matches=( ${(on)matches} )
    for entry in $matches; do
      [[ -d $entry ]] && entry+=/
      labels+=( "@${(q)entry}" )
    done
    if (( ${#labels} )); then
      zle -M -- "${(j:  :)labels}  · Tab complete · arrows select"
    else
      zle -M -- 'No matching paths · edit the reference before sending'
    fi
    typeset -g _herdstead_path_message=1
  elif [[ ${_herdstead_path_message:-0} == 1 ]]; then
    zle -M ''
    typeset -g _herdstead_path_message=0
  fi
}
zle -N _herdstead_suggestions
add-zle-hook-widget line-pre-redraw _herdstead_suggestions

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
    if [[ -t 1 && $# == 1 && -f $1 && $1 != -* ]]; then hcat "$1"
    else command cat "$@"
    fi
  }
fi
if (( ! $+aliases[tree] && ! $+functions[tree] )); then
  function tree() { "$HERDSTEAD_EXECUTABLE" tree "$@" }
fi
