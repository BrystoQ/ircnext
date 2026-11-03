; Example mIRC script for ircnext. Copy it into the scripts folder (next
; to config.toml), then type /reload.

; /hello [nick]: greets someone in the current channel.
alias hello {
  if ($1 == $null) {
    echo -a Usage: /hello <nick>
    halt
  }
  msg $chan Hello $1 $+ , welcome to $chan $+ !
}

; /dice [sides]: rolls a die.
alias dice {
  var %sides = $iif($1 isnum 2-1000, $1, 6)
  msg $chan $me rolls a $rand(1, %sides) (d $+ %sides $+ )
}

; /countdown: shows a loop and local variables.
alias countdown {
  var %i = 3
  while (%i > 0) {
    echo -a %i $+ ...
    dec %i
  }
  echo -a Go!
}

; $initials(text): an identifier written as an alias.
alias initials {
  var %out, %i = 1
  while (%i <= $numtok($1, 32)) {
    var %out = %out $+ $upper($left($gettok($1, %i, 32), 1))
    inc %i
  }
  return %out
}
