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

; Events: a tiny channel bot.
on *:TEXT:!seen &:#:{
  msg $chan $nick $+ : I do not keep logs of $2 yet.
}

on *:JOIN:#:{
  notice $nick Welcome to $chan $+ , $nick $+ !
}

; Mine: `on me:` handlers run for my own actions.
on me:*:JOIN:#:echo -a Joined $chan on $network

; Typed text: `halt` keeps it from being sent.
on *:INPUT:#:{
  if ($1 == brb) {
    msg $chan be right back
    halt
  }
}

; Hash tables and timers: count messages per person, report every hour.
on *:TEXT:*:#:{
  hinc -m stats $nick
}

alias top {
  echo -a $hget(stats, 0).item people talked
  var %i = 1
  while (%i <= $hget(stats, 0).item) {
    echo -a $hget(stats, %i).item : $hget(stats, %i).data
    inc %i
  }
}

on *:CONNECT:timerstats 0 3600 top
