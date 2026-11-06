# Fealty (working title)

Asciiverse multiplayer strategy game. Rust + Ratatui, networking over iroh. Original IP.

## Origin

Inspired by **Warwick**, a "multiplayer, terminal-based strategy game for linux" announced by a
small studio called Lurebound in 2017 (site playwarwick.com, GitHub org `lurebound`). It never
shipped: the closed-beta signup opened, the docs page only ever said "Coming soon!", and the
repos had disappeared by 2026. What existed was a 5-commit Python command-line client (MIT) for a
server that was never published, apparently written in Elixir. Researched 2026-10-12; the
Wayback Machine holds the site (July to November 2018) and the repo front pages.

Nothing is ported or reused from Warwick; there is nothing left to port. Fealty takes only the
premise and does it as a proper full-screen TUI. Warwick's own client was one-shot shell commands
(`warwick list players`, `warwick message Bjorn re:friends`).

## Premise

Each player starts as a lesser noble in an early-medieval realm and rises by military, economic
and political means: allying, conquering and bribing other players. The first to become king or
queen of their own faction wins.

Warwick's four factions were Byzantine, Norse, Frank and Longobard, each with its own bonuses and
unique units. Using real historical peoples is fine, but the faction set is ours to decide.

## Design notes

- The politics between players is the game: alliances, betrayal, bribes, vassalage, oaths of
  fealty that can be broken at a cost. Combat and economy matter, but they serve the diplomacy.
- Holds and fortresses on a map are what you conquer; titles (lord, count, duke, king) are what
  you climb.
- Messaging between players is a first-class part of the UI (letters, envoys, secret offers),
  not a separate chat window.
- Multiplayer over iroh, direct between players, no central server, the same as Panzer, Tetrix
  and Sleighride. Needs a design for who hosts the authoritative game state and what happens when
  that player drops.
- Rankings and statistics after a game, charts drawn in the terminal (Warwick promised "colorful
  charts").
- Session logging per the Asciiverse convention (`log` + `env_logger` or `tracing`).

## Open questions / next steps

- [ ] Title: Fealty is a working name; check for collisions before it sticks
- [ ] Real-time, turn-based, or asynchronous turns over hours or days (a long game played in short
      sessions suits a diplomacy game and a terminal well, but iroh with no server makes offline
      players hard)
- [ ] Number of players per game, and whether AI nobles fill empty seats
- [ ] Faction set and what makes each one play differently
- [ ] Map: hex or province graph; how much is visible to whom (fog of war matters for intrigue)
- [ ] Bribery and treachery mechanics that are fun to be on the losing end of
- [ ] Persistent hosting for long games without a central server, or accept that the host must
      stay online

## Status

Ideation stage only, no code. Concept saved 2026-10-12 after researching what happened to
Warwick (`lurebound/warwick`).
