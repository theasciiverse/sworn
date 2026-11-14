# Roadmap

Sworn is at 0.3.0 (2026-11-14): played against the computer nobles by
Claude in a terminal and tuned from that, in the catalog, with the court
of 0.3.0 mostly in. 1.0.0 is when everything below is in. The open questions
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

## 0.3.0: the court (released with the map names still to do)

- [x] Help that teaches: a new game opens with a word on raising,
  marching and ending the season, and where the crown's needs are shown
- [x] Liege and vassal duties: a liege's letter that names a province
  ("march on Osmoor") calls a computer vassal to fight there for it; one
  grown bigger than its liege does not answer
- [x] Computer nobles that betray when it pays: a vassal whose oath is all
  that stands between it and its people's crown renounces
- [x] Map names that stay readable at 80x24 (labels nudged a cell or two
  apart, names cut keep their ending's first letter)

## 0.4.0: playing with people

- [ ] Played with people over the network; pace and the 30-year limit
  tuned from that
- [ ] Rehosting when the host is gone for good (open question in
  `CLAUDE.md`: how without breaking the fog of war)
- [ ] A terminal bell at the end of a season and when the timer runs low,
  off by default

## 1.0.0

- [ ] Everything above, played through and tuned
