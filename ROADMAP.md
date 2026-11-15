# Roadmap

Sworn is at 0.5.0 (2026-11-15): played over the network between two
players' terminals, with a length for hosted games and a returning
player who finds their seat by the name they typed. 1.0.0 is when
everything below is in; two of the last items are David's decisions. The open questions
behind some of these are in `CLAUDE.md`.

## 0.1.0: built (2026-11-07)

- [x] The rules: seasons with secret orders, letters and binding offers,
  oaths, battles and clashes on the road, titles and the crown
- [x] Computer nobles that plan from what a player could see, take deals
  and play politics
- [x] Network play over iroh: LAN discovery, joining by id, rejoining,
  rehosting from the save, a season timer
- [x] The terminal interface, the end screen's standings and charts

## 0.2.0: played and tuned, first release

- [x] Played against the computer nobles at 100x30 and 80x24, and two
  local processes over LAN and by id
- [x] A computer noble that kneels no longer turns on its liege that same
  season, and one that demands an oath waits a season for the answer
- [x] Computer nobles write first when their lands meet a player's, and
  the strong ask a year's tribute as a binding offer
- [x] The crown's conditions as a checklist; fitting the map at small
  sizes; a fallen player can watch to the end; clearer reports
- [x] Saves in the state folder; `--seed N`
- [x] Catalog page, preview and release workflow

## 0.3.0: the court

- [x] Help that teaches: a new game opens with a word on raising,
  marching and ending the season, and where the crown's needs are shown
- [x] Liege and vassal duties: a liege's letter that names a province
  ("march on Osmoor") calls a computer vassal to fight there for it; one
  grown bigger than its liege does not answer
- [x] Computer nobles that betray when it pays: a vassal whose oath is all
  that stands between it and its people's crown renounces
- [x] Map names that stay readable at 80x24 (labels nudged a cell or two
  apart, names cut keep their ending's first letter)

## 0.4.0: a match for one player (2026-11-15)

Claude's own first game against the computer nobles was won in five
years; they grabbed land slowly, hoarded gold and sold peace to the
leader. Playing with people moved to 0.5.0, since it needs people.

- [x] Computer nobles muster troops where their next conquest lacks them,
  spend their gold, save up for their own crown seat, keep a peace they
  paid for, sell none to the realm's leader, and kin do not kneel cheaply
  to a noble near the crown
- [x] Difficulty for games against the computer: Gentle, Fair, Hard (their
  lands pay two thirds, the same, or half again)
- [x] The march window shows both sides' strength and whether it is enough
- [x] At 80x24 the crown's checklist leads the nobles tab, the chronicle
  opens on the newest season; unread letters only count as new; a finished
  game says so on the status line

## 0.5.0: playing with people, as far as one machine goes (2026-11-15)

Two players' terminals on rover, each its own process and state folder,
one hosting and one joining, in tmux. That proves the game works between
two programs over iroh: finding the table on the LAN and joining by id,
the lobby, letters with binding offers crossing the wire and accepted,
orders waiting on everyone, the season timer running out on a player who
gave none, a player who leaves held by a steward and returning, and a
whole game from the lobby to the end screen (it ended with a coronation
in Summer 805). It does not prove what people do: whether they find the
diplomacy fun, whether seasons drag with a timer or without, how long a
game should be. Both players were driven by a script, and two processes
on one machine never meet a real network's delays or NAT.

- [x] Played over the network between two processes, LAN and by id, to
  the end of a game; with a season timer; leaving and rejoining
- [x] A length for hosted games: the host picks 10, 20 or 30 years in the
  lobby (`y`), since a table of people may not have thirty years in them;
  the crown's checklist says when the highest score will win
- [x] A player renamed "David 2" because the host was also David comes back
  as plain David and finds their seat (it used to refuse them)
- [x] Lobby panels count their wrapped lines, so the keys line under a
  host's long id is no longer cut off
- [ ] Played with people; pace and the length tuned from that (needs people)
- [ ] Rehosting when the host is gone for good (David's decision: open
  question in `CLAUDE.md`, how without breaking the fog of war)
- [ ] A terminal bell at the end of a season and when the timer runs low,
  off by default (David's decision: a taste call)

## 1.0.0

- [ ] Everything above, played through and tuned
