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

## Decisions (2026-11-06/07)

- **Turns:** simultaneous seasons, as in Diplomacy. Everyone's orders are secret and resolve
  together; letters and offers go any time in between. Four seasons a year, starting 800; no
  marching in winter (`Rules::winter_marches`), so winter is for letters.
- **Hosting:** the host holds the only `Game` and sends each player their fogged `View` and
  nothing more (`netcourt.rs`). The rules core is pure and deterministic (own SplitMix64 RNG,
  no clock) and the host saves every season (`~/.local/share/fealty/hosted.fealty`). A player
  who drops is played by the computer (a "steward") until they rejoin under the same name; a
  host who drops picks "Resume the game you host" and players rejoin by name.
- **Players:** 2 to 8 nobles; computer nobles (`ai.rs`) fill empty seats and plan from the same
  fogged `View` a player gets.
- **Map:** a graph of provinces (holds, towns, fortresses, one crown seat per people), generated
  from a jittered grid with some roads cut. Who holds what is public; troop counts are visible
  only in your own provinces and their neighbours, and your liege's and direct vassals'.
- **Factions (light bonuses only):** Saxons keep troops a third cheaper, Danes attack 15%
  stronger, Bretons defend 50% stronger and their fortresses pay 3 more, Moravian towns pay 3
  more. Tuned 2026-11-07 on computer games (`cargo run --release --example balance`): each wins
  20-29% of games. The first version (Saxon troops at half price) won 61%.
- **Treachery:** only offers bind (gold, provinces, fealty; executed before marching if both
  sides can still pay). Letters promise anything. Marching for an ally (`on_behalf`) is how you
  support them, and not turning up is how you betray them. Marching on your own liege or vassal
  breaks the oath publicly for 10 prestige; renouncing openly costs 4.
- **Head-on marches** (2026-11-07): columns going opposite ways along one road meet halfway if
  they serve different sides, and fight troop for troop (no walls, no people's bonus: with the
  Danes' attack bonus counted, they rose from 29% to 32% of wins). The winners march on with
  their survivors, the rest fall back with half; allies guarding each other just pass.
- **Winning:** king of your people = no liege, hold your people's crown seat, power (provinces
  held directly and through vassals) at least 40% of the realm, 20 prestige, and every other
  living noble of your people sworn to you somewhere down the chain. After 30 years the highest
  score wins instead.
- **Computer nobles' politics:** they take good deals and answer players' offers by letter;
  4+ gold paid to one within a year buys its peace; outmatched ones send gold to whoever took
  their land and plead; strong ones demand fealty of weak neighbours (own people first, whose
  oaths the crown needs); all lean on whoever holds 25%+ of the realm, and the strongest rival
  writes to the rest urging them to strike; a vassal grown twice its liege's size renounces.

## Open questions / next steps

- [ ] Title: Fealty is a working name. Might & Fealty (`lemcomm/ReMaF`, a browser medieval
      politics game still in development) is close in name and genre
- [ ] License: not set yet (Panzer is MIT, Tetrix GPL-2.0-or-later)
- [ ] Play it with people. Balance so far is computer against computer only; since the
      computer nobles learned politics, 71% of their games end in a coronation (was 94%), in
      14 years on average. Whether that pace suits people is untested
- [ ] Rehosting needs the save, which only the host has. Sending it to every player each season
      would let anyone rehost, but a save holds the whole game, so it would break the fog of war
      for anyone who looks inside. Undecided
- [ ] Turn timer for live sessions (planned, not built): the host can only wait for everyone
- [ ] Sound, and juice (battle flashes on the map)
- [ ] LAN port 31460 is Fealty's. Note `sigpipe` and `sleighride` both use 31459

## Status (2026-11-07)

Playable: single player against computer nobles, and multiplayer over iroh (LAN discovery or
an id across the internet), with saves, rejoining and rehosting.

- `fealty` title menu; `fealty --host`, `fealty --join [ID]`, `fealty --load FILE`;
  `fealty --simulate [--seed N] [--nobles N] [--all]` prints a whole computer game.
  Session logging: `RUST_LOG=debug fealty 2>fealty.log`.
- Rules: `map.rs` generation, `game.rs` state, `diplomacy.rs` letters and offers, `resolve.rs`
  the season, `view.rs` fog of war, `ai.rs` computer nobles, `save.rs` save files,
  `chronicle.rs` events with who may see them.
- `court.rs`: the interface talks only to a `Court` (`LocalCourt` here; `HostCourt`,
  `RemoteCourt` in `netcourt.rs`). `net.rs`/`lan.rs` are Panzer's, widened to 7 guests
  (Panzer's writer now waits 300 ms before closing a kicked connection, so its Reject arrives).
  `proto.rs` is Fealty's own.
- `tui/`: `map.rs` the realm as text, `play.rs` game screen state and keys, `draw.rs` drawing,
  `over.rs` standings and charts after the game, `title.rs`, `lobby.rs`.
- Tests: `tests/rules.rs` (each mechanic on a hand-built map, AI deals and bribes, 175 computer
  games checked for invariants and replay determinism), `tests/tui.rs` (a season by keyboard,
  random keys and clicks for whole games at five terminal sizes, the end screen's charts),
  `tests/net.rs` (real iroh games on localhost: discovery, fog over the wire, private letters,
  waiting on every player, dropping and rejoining, strangers refused, rehosting from the save,
  and hosting/joining through the actual screens).
- `examples/screens.rs` prints the game screen as text after a key sequence, for checking
  layout without a terminal; `examples/balance.rs` reports win rates by people.

Concept saved 2026-10-12 after researching what happened to Warwick (`lurebound/warwick`).
