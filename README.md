# Sworn

Multiplayer medieval politics for the terminal. An Asciiverse game, in Rust and Ratatui.

You start as a lesser noble in the year 800. Rise by war, trade and treachery until you are
crowned king of your people. Every season, all orders are secret and take effect at once.
Letters can promise anything; only offers bind, and only if both sides can still pay when the
season turns.

## Playing

    cargo run --release            # title menu
    cargo run --release -- --host  # host a table for others
    cargo run --release -- --join  # find tables on the local network
    cargo run --release -- --join <ID>   # join a host across the internet
    cargo run --release -- --seed 7      # new games on the same realm each time

In game, press `?` for the keys. Briefly: arrows move on the map, `r` raises troops, `m`
marches (`b` to fight on someone else's behalf), `w` writes a letter with an optional offer,
`e` ends the season.

Up to eight nobles; empty seats go to computer nobles. A game against the computer has a
difficulty: on Gentle their lands pay them a third less than yours would, on Hard half again
as much; on Fair the same. Games save every season to
the state folder (`$XDG_STATE_HOME/sworn`, else `~/.local/state/sworn/`). If a player drops, the computer holds their seat until they rejoin
under the same name. If the host drops, they resume from the title menu and everyone rejoins.
The host can set a season timer in the lobby (`t`); when it runs out, anyone who has not sent
orders holds where they stand.

Enemy columns marching opposite ways along the same road meet halfway and fight in the field,
troop for troop, before either reaches its target.

## The crown

To be king of your people you must answer to no one, hold your people's crown seat (♛),
command 40% of the realm through your own provinces and your vassals', have 20 prestige, and
have every other noble of your people sworn to you. After thirty years the highest score wins.

## The peoples

- **Saxons**: troops cost a third less to keep.
- **Danes**: 10% stronger when attacking.
- **Bretons**: half again as strong defending; fortresses pay 3 more.
- **Moravians**: towns pay 3 more.

## License

MIT.
