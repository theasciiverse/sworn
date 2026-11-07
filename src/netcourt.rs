//! Games over the network. A host opens a `HostLobby`, players find it on
//! the LAN (or dial its id) through a `JoinLobby`; when the host begins,
//! the lobbies become a `HostCourt` and `RemoteCourt`s.
//!
//! The host holds the only `Game` and the save file. A player who drops is
//! played by the computer (a steward) until they come back under the same
//! name; a host who drops can continue from the save and the players rejoin.

use crate::court::{Court, Notice, Offer};
use crate::faction::Faction;
use crate::game::{Game, NewNoble, Setup};
use crate::net::{ClientId, Cmd, Evt, Net};
use crate::orders::Order;
use crate::proto::{Msg, Seat};
use crate::rules::Rules;
use crate::view::View;
use crate::{save, season_name, NobleId};
use iroh::{EndpointAddr, EndpointId};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const PING_EVERY: Duration = Duration::from_secs(2);

/// Season timers the host can choose in the lobby, in seconds (0: none).
pub const TIMERS: [u32; 6] = [0, 60, 120, 180, 300, 600];

/// "2 min" or "90 s"; "no time limit" for none.
pub fn timer_name(t: Option<Duration>) -> String {
    match t.map(|t| t.as_secs()) {
        None | Some(0) => "no time limit".into(),
        Some(s) if s % 60 == 0 => format!("{} min a season", s / 60),
        Some(s) => format!("{s} s a season"),
    }
}

fn instant_in(secs: Option<u32>) -> Option<Instant> {
    secs.map(|s| Instant::now() + Duration::from_secs(s as u64))
}

/// Names for computer nobles filling empty seats.
pub const COMPUTER_NAMES: &[&str] = &["Aldric", "Brynja", "Cedric", "Dagny", "Eadric", "Frida", "Godwin", "Hild", "Ivo", "Jorunn"];

fn unique_name(name: &str, taken: &[String]) -> String {
    let base: String = name.trim().chars().take(12).collect();
    let base = if base.is_empty() { "Stranger".to_string() } else { base };
    if !taken.iter().any(|t| t.eq_ignore_ascii_case(&base)) {
        return base;
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n))).unwrap()
}

struct LobbySeat {
    name: String,
    faction: Faction,
    client: Option<ClientId>,
}

/// The host before the game begins: players come and go and pick their people.
pub struct HostLobby {
    net: Net,
    pub id: Option<EndpointId>,
    seats: Vec<LobbySeat>,
    pub size: usize,
    /// A game continued from a save: players reclaim their seats by name.
    resumed: Option<Game>,
    path: PathBuf,
    pub status: String,
    pub failed: Option<String>,
    last_ping: Instant,
    /// How long each season waits for orders before the stragglers hold.
    pub timer: Option<Duration>,
}

impl HostLobby {
    pub fn new(name: &str, faction: Faction, size: usize, path: PathBuf) -> HostLobby {
        let host = LobbySeat { name: unique_name(name, &[]), faction, client: None };
        HostLobby {
            net: Net::host(host.name.clone()),
            id: None,
            seats: vec![host],
            size: size.clamp(2, 8),
            resumed: None,
            path,
            status: "Opening the gates…".into(),
            failed: None,
            last_ping: Instant::now(),
            timer: None,
        }
    }

    /// Continue a hosted game from its save. The host sits where they sat.
    pub fn resume(game: Game, path: PathBuf) -> HostLobby {
        let me = &game.nobles[0];
        let host = LobbySeat { name: me.name.clone(), faction: me.faction, client: None };
        let size = game.nobles.len();
        HostLobby {
            net: Net::host(host.name.clone()),
            id: None,
            seats: vec![host],
            size,
            resumed: Some(game),
            path,
            status: "Opening the gates…".into(),
            failed: None,
            last_ping: Instant::now(),
            timer: None,
        }
    }

    /// Sets how long each season waits for orders (None: until everyone is ready).
    pub fn set_timer(&mut self, t: Option<Duration>) {
        self.timer = t.filter(|t| !t.is_zero());
        self.broadcast_lobby();
    }

    /// Steps through `TIMERS`.
    pub fn cycle_timer(&mut self, forward: bool) {
        let cur = self.timer.map_or(0, |t| t.as_secs() as u32);
        let i = TIMERS.iter().position(|&t| t == cur).unwrap_or(0) as i32 + if forward { 1 } else { -1 };
        let t = TIMERS[i.rem_euclid(TIMERS.len() as i32) as usize];
        self.set_timer(Some(Duration::from_secs(t as u64)));
    }

    pub fn resumed(&self) -> bool {
        self.resumed.is_some()
    }

    pub fn seats(&self) -> Vec<Seat> {
        match &self.resumed {
            None => self.seats.iter().map(|s| Seat { name: s.name.clone(), faction: s.faction, host: s.client.is_none(), empty: false }).collect(),
            Some(g) => g
                .nobles
                .iter()
                .filter(|n| n.alive)
                .map(|n| {
                    let taken = self.seats.iter().any(|s| s.name == n.name);
                    Seat { name: n.name.clone(), faction: n.faction, host: n.id == 0, empty: !taken }
                })
                .collect(),
        }
    }

    pub fn set_faction(&mut self, f: Faction) {
        if self.resumed.is_none() {
            self.seats[0].faction = f;
            self.broadcast_lobby();
        }
    }

    pub fn players(&self) -> usize {
        self.seats.len()
    }

    fn broadcast_lobby(&self) {
        let timer = self.timer.map_or(0, |t| t.as_secs() as u32);
        let msg = Msg::Lobby { seats: self.seats(), size: self.size as u8, resumed: self.resumed.is_some(), timer };
        self.net.send(Cmd::Broadcast(msg));
        self.net.send(Cmd::Beacon { players: self.seats.len() as u8, in_game: false });
    }

    fn reject(&self, client: ClientId, reason: String) {
        self.net.send(Cmd::SendTo(client, Msg::Reject { reason }));
        self.net.send(Cmd::Kick(client));
    }

    pub fn poll(&mut self) {
        while let Some(e) = self.net.poll() {
            match e {
                Evt::Hosting { id } => {
                    self.id = Some(id);
                    self.status = "Waiting for players. They can find this game on the LAN, or join by its id.".into();
                }
                Evt::Failed(s) => self.failed = Some(s),
                Evt::ClientJoined { client, name } => self.joined(client, name),
                Evt::ClientMsg { client, msg: Msg::Pick(f) } => {
                    if let Some(s) = self.seats.iter_mut().find(|s| s.client == Some(client)) {
                        s.faction = f;
                    }
                    self.broadcast_lobby();
                }
                Evt::ClientMsg { .. } => {}
                Evt::ClientLeft { client } => {
                    if let Some(i) = self.seats.iter().position(|s| s.client == Some(client)) {
                        let s = self.seats.remove(i);
                        self.status = format!("{} left.", s.name);
                    }
                    self.broadcast_lobby();
                }
                Evt::Connected | Evt::HostMsg(_) | Evt::Disconnected => {}
            }
        }
        if self.last_ping.elapsed() >= PING_EVERY {
            self.last_ping = Instant::now();
            self.net.send(Cmd::Broadcast(Msg::Ping));
        }
    }

    fn joined(&mut self, client: ClientId, name: String) {
        let taken: Vec<String> = self.seats.iter().map(|s| s.name.clone()).collect();
        match &self.resumed {
            None => {
                if self.seats.len() >= self.size {
                    return self.reject(client, format!("all {} seats are taken", self.size));
                }
                let name = unique_name(&name, &taken);
                let used: Vec<Faction> = self.seats.iter().map(|s| s.faction).collect();
                let faction = Faction::ALL.into_iter().find(|f| !used.contains(f)).unwrap_or(Faction::Saxons);
                self.status = format!("{name} joined.");
                self.seats.push(LobbySeat { name, faction, client: Some(client) });
            }
            Some(g) => {
                let Some(n) = g.nobles.iter().find(|n| n.alive && n.id != 0 && n.name.eq_ignore_ascii_case(name.trim())) else {
                    let open: Vec<&str> = g.nobles.iter().filter(|n| n.alive && n.id != 0 && !taken.contains(&n.name)).map(|n| n.name.as_str()).collect();
                    return self.reject(client, format!("no seat named {name} in this game; open seats: {}", open.join(", ")));
                };
                if taken.contains(&n.name) {
                    return self.reject(client, format!("{} is already seated", n.name));
                }
                self.status = format!("{} is back.", n.name);
                self.seats.push(LobbySeat { name: n.name.clone(), faction: n.faction, client: Some(client) });
            }
        }
        self.broadcast_lobby();
    }

    /// Begins (or resumes) the game; empty seats go to computer nobles.
    pub fn start(self, seed: u64) -> Result<HostCourt, String> {
        let mut clients: BTreeMap<ClientId, NobleId> = BTreeMap::new();
        let game = match self.resumed {
            Some(mut g) => {
                for n in g.nobles.iter_mut() {
                    n.ai = n.id != 0;
                }
                for s in &self.seats {
                    if let (Some(c), Some(n)) = (s.client, g.nobles.iter_mut().find(|n| n.name == s.name)) {
                        n.ai = false;
                        clients.insert(c, n.id);
                    }
                }
                g
            }
            None => {
                let mut nobles: Vec<NewNoble> = vec![];
                let mut taken: Vec<String> = vec![];
                for (i, s) in self.seats.iter().enumerate() {
                    if let Some(c) = s.client {
                        clients.insert(c, i as NobleId);
                    }
                    taken.push(s.name.clone());
                    nobles.push(NewNoble { name: s.name.clone(), faction: s.faction, ai: false });
                }
                let mut names = COMPUTER_NAMES.iter();
                while nobles.len() < self.size {
                    let name = loop {
                        let n = names.next().copied().unwrap_or("Nobody");
                        if !taken.iter().any(|t| t.eq_ignore_ascii_case(n)) || n == "Nobody" {
                            break n.to_string();
                        }
                    };
                    let faction = Faction::ALL[nobles.len() % 4];
                    taken.push(name.clone());
                    nobles.push(NewNoble { name, faction, ai: true });
                }
                let mut g = Game::new(Setup { seed, nobles, rules: Rules::default() })?;
                g.ai_write();
                g
            }
        };
        log::info!("hosting a game: {} nobles, {} players, season {}", game.nobles.len(), clients.len() + 1, game.season);
        let view = game.view(0);
        let mut court = HostCourt {
            net: self.net,
            id: self.id,
            game,
            clients,
            view,
            path: self.path,
            notices: vec![],
            last_ping: Instant::now(),
            timer: self.timer,
            deadline: None,
        };
        court.open_season();
        for (&c, &n) in &court.clients {
            court.net.send(Cmd::SendTo(c, Msg::Seated { me: n }));
            court.net.send(Cmd::SendTo(c, Msg::Deadline(court.secs_left())));
        }
        court.net.send(Cmd::Beacon { players: court.clients.len() as u8 + 1, in_game: true });
        court.publish();
        court.autosave();
        Ok(court)
    }
}

/// The host, in the game.
pub struct HostCourt {
    net: Net,
    pub id: Option<EndpointId>,
    game: Game,
    clients: BTreeMap<ClientId, NobleId>,
    view: View,
    path: PathBuf,
    notices: Vec<Notice>,
    last_ping: Instant,
    timer: Option<Duration>,
    deadline: Option<Instant>,
}

impl HostCourt {
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Starts the clock on a new season's orders, if the table has a timer.
    fn open_season(&mut self) {
        self.deadline = self.timer.filter(|_| self.game.winner.is_none()).map(|t| Instant::now() + t);
    }

    fn secs_left(&self) -> Option<u32> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()).as_secs().max(1) as u32)
    }

    /// Time is up: every person still deciding gives no orders, so their
    /// troops hold where they stand, and the season turns.
    fn time_up(&mut self) {
        self.deadline = None;
        let late: Vec<NobleId> = self.game.alive().filter(|n| !n.ai && self.game.orders[n.id as usize].is_none()).map(|n| n.id).collect();
        if late.is_empty() {
            return;
        }
        for &n in &late {
            let _ = self.game.submit(n, vec![]);
        }
        let names: Vec<String> = late.iter().map(|&n| self.name(n)).collect();
        log::info!("time ran out for {}", names.join(", "));
        self.tell_all(format!("Time ran out. No orders from {}; their troops hold.", names.join(", ")));
        self.maybe_resolve();
    }

    /// Sends every player their view, and refreshes the host's own.
    fn publish(&mut self) {
        for (&c, &n) in &self.clients {
            self.net.send(Cmd::SendTo(c, Msg::View(Box::new(self.game.view(n)))));
        }
        self.view = self.game.view(0);
    }

    fn autosave(&mut self) {
        if let Err(e) = save::save(&self.game, &self.path) {
            log::warn!("autosave to {} failed: {e}", self.path.display());
            self.notices.push(Notice::Info(format!("could not save: {e}")));
        }
    }

    fn name(&self, n: NobleId) -> String {
        self.game.nobles[n as usize].name.clone()
    }

    fn tell_all(&mut self, s: String) {
        self.net.send(Cmd::Broadcast(Msg::Info(s.clone())));
        self.notices.push(Notice::Info(s));
    }

    /// Resolves the season once every person at the table has given orders.
    fn maybe_resolve(&mut self) {
        if self.game.winner.is_some() || !self.game.people_ready() {
            return;
        }
        self.game.ai_orders();
        let season = self.game.season;
        self.game.resolve();
        if self.game.winner.is_none() {
            self.game.ai_write();
        }
        self.autosave();
        self.open_season();
        let secs = self.secs_left();
        self.net.send(Cmd::Broadcast(Msg::Deadline(secs)));
        self.publish();
        self.notices.push(Notice::Resolved { season });
        log::info!("resolved {}", season_name(season));
    }

    fn client_msg(&mut self, c: ClientId, msg: Msg) {
        let Some(&n) = self.clients.get(&c) else { return };
        match msg {
            Msg::Letter { to, body, offer } => match self.game.send_letter(n, &to, &body, offer) {
                Ok(_) => self.publish(),
                Err(e) => self.net.send(Cmd::SendTo(c, Msg::Info(format!("Letter not sent: {e}")))),
            },
            Msg::Submit(orders) => {
                match self.game.submit(n, orders) {
                    Ok(()) => {
                        self.publish();
                        self.maybe_resolve();
                    }
                    Err(e) => self.net.send(Cmd::SendTo(c, Msg::Info(format!("Orders refused: {e}")))),
                }
            }
            Msg::Unsubmit => {
                self.game.unsubmit(n);
                self.publish();
            }
            Msg::Ping | Msg::Pick(_) | Msg::Hello { .. } => {}
            other => log::debug!("unexpected from client {c}: {other:?}"),
        }
    }

    fn rejoin(&mut self, c: ClientId, name: String) {
        let seat = self.game.nobles.iter().find(|n| n.alive && n.id != 0 && n.name.eq_ignore_ascii_case(name.trim())).map(|n| n.id);
        let Some(n) = seat else {
            self.net.send(Cmd::SendTo(c, Msg::Reject { reason: format!("the game has begun, and no seat is named {name}") }));
            self.net.send(Cmd::Kick(c));
            return;
        };
        if self.clients.values().any(|&x| x == n) {
            self.net.send(Cmd::SendTo(c, Msg::Reject { reason: format!("{} is already seated", self.name(n)) }));
            self.net.send(Cmd::Kick(c));
            return;
        }
        self.game.nobles[n as usize].ai = false;
        // Any orders the steward had not yet given are theirs to give again.
        self.clients.insert(c, n);
        self.net.send(Cmd::SendTo(c, Msg::Seated { me: n }));
        self.net.send(Cmd::SendTo(c, Msg::Deadline(self.secs_left())));
        self.publish();
        self.tell_all(format!("{} has returned to the table.", self.name(n)));
    }
}

impl Court for HostCourt {
    fn view(&self) -> &View {
        &self.view
    }

    fn send_letter(&mut self, to: &[NobleId], body: &str, offer: Offer) -> Result<(), String> {
        self.game.send_letter(0, to, body, offer)?;
        self.publish();
        Ok(())
    }

    fn submit(&mut self, orders: Vec<Order>) -> Result<(), String> {
        self.game.submit(0, orders)?;
        self.publish();
        self.maybe_resolve();
        Ok(())
    }

    fn unsubmit(&mut self) -> Result<(), String> {
        self.game.unsubmit(0);
        self.publish();
        Ok(())
    }

    fn poll(&mut self) -> Vec<Notice> {
        while let Some(e) = self.net.poll() {
            match e {
                Evt::ClientMsg { client, msg } => self.client_msg(client, msg),
                Evt::ClientJoined { client, name } => self.rejoin(client, name),
                Evt::ClientLeft { client } => {
                    if let Some(n) = self.clients.remove(&client) {
                        self.game.nobles[n as usize].ai = true;
                        self.publish();
                        self.tell_all(format!("{} has left; a steward holds their seat until they return.", self.name(n)));
                        self.maybe_resolve();
                    }
                }
                Evt::Failed(s) => self.notices.push(Notice::Lost(s)),
                Evt::Hosting { id } => self.id = Some(id),
                Evt::Connected | Evt::HostMsg(_) | Evt::Disconnected => {}
            }
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            self.time_up();
        }
        if self.last_ping.elapsed() >= PING_EVERY {
            self.last_ping = Instant::now();
            self.net.send(Cmd::Broadcast(Msg::Ping));
        }
        std::mem::take(&mut self.notices)
    }

    fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn save(&mut self) -> Result<String, String> {
        save::save(&self.game, &self.path).map_err(|e| e.to_string())?;
        Ok(self.path.display().to_string())
    }

    fn describe(&self) -> String {
        let ai = self.game.alive().filter(|n| n.ai).count();
        format!("hosting {} player{} and {ai} computer noble{}", self.clients.len(), if self.clients.len() == 1 { "" } else { "s" }, if ai == 1 { "" } else { "s" })
    }
}

/// A player, before the game begins.
pub struct JoinLobby {
    net: Net,
    pub connected: bool,
    pub seats: Vec<Seat>,
    pub size: u8,
    pub resumed: bool,
    /// The host's season timer, in seconds (0: none).
    pub timer: u32,
    pub faction: Faction,
    seated: Option<NobleId>,
    deadline: Option<Instant>,
    view: Option<View>,
    pub failed: Option<String>,
    last_ping: Instant,
}

impl JoinLobby {
    pub fn new(addr: EndpointAddr, name: &str) -> JoinLobby {
        JoinLobby {
            net: Net::join(addr, name.trim().to_string()),
            connected: false,
            seats: vec![],
            size: 0,
            resumed: false,
            timer: 0,
            faction: Faction::Saxons,
            seated: None,
            deadline: None,
            view: None,
            failed: None,
            last_ping: Instant::now(),
        }
    }

    pub fn pick(&mut self, f: Faction) {
        self.faction = f;
        self.net.send(Cmd::Broadcast(Msg::Pick(f)));
    }

    /// Ready to play once seated and the first view has come.
    pub fn poll(&mut self) -> bool {
        while let Some(e) = self.net.poll() {
            match e {
                Evt::Connected => self.connected = true,
                Evt::Failed(s) => self.failed = Some(s),
                Evt::Disconnected => {
                    if self.failed.is_none() {
                        self.failed = Some("the host closed the connection".into());
                    }
                }
                Evt::HostMsg(Msg::Reject { reason }) => self.failed = Some(reason),
                Evt::HostMsg(Msg::Lobby { seats, size, resumed, timer }) => {
                    self.seats = seats;
                    self.size = size;
                    self.resumed = resumed;
                    self.timer = timer;
                }
                Evt::HostMsg(Msg::Deadline(secs)) => self.deadline = instant_in(secs),
                Evt::HostMsg(Msg::Seated { me }) => self.seated = Some(me),
                Evt::HostMsg(Msg::View(v)) => self.view = Some(*v),
                Evt::HostMsg(_) => {}
                _ => {}
            }
        }
        if self.last_ping.elapsed() >= PING_EVERY {
            self.last_ping = Instant::now();
            self.net.send(Cmd::Broadcast(Msg::Ping));
        }
        self.failed.is_none() && self.seated.is_some() && self.view.as_ref().is_some_and(|v| Some(v.me) == self.seated)
    }

    pub fn into_court(self) -> Option<RemoteCourt> {
        let view = self.view?;
        Some(RemoteCourt { net: self.net, view, notices: vec![], last_ping: Instant::now(), lost: false, deadline: self.deadline })
    }
}

/// A player's side of a game hosted elsewhere.
pub struct RemoteCourt {
    net: Net,
    view: View,
    notices: Vec<Notice>,
    last_ping: Instant,
    lost: bool,
    deadline: Option<Instant>,
}

impl Court for RemoteCourt {
    fn view(&self) -> &View {
        &self.view
    }

    fn send_letter(&mut self, to: &[NobleId], body: &str, offer: Offer) -> Result<(), String> {
        if self.lost {
            return Err("not connected to the host".into());
        }
        self.net.send(Cmd::Broadcast(Msg::Letter { to: to.to_vec(), body: body.into(), offer }));
        Ok(())
    }

    fn submit(&mut self, orders: Vec<Order>) -> Result<(), String> {
        if self.lost {
            return Err("not connected to the host".into());
        }
        self.net.send(Cmd::Broadcast(Msg::Submit(orders)));
        self.view.submitted = true;
        Ok(())
    }

    fn unsubmit(&mut self) -> Result<(), String> {
        self.net.send(Cmd::Broadcast(Msg::Unsubmit));
        self.view.submitted = false;
        Ok(())
    }

    fn poll(&mut self) -> Vec<Notice> {
        while let Some(e) = self.net.poll() {
            match e {
                Evt::HostMsg(Msg::View(v)) => {
                    let old = self.view.season;
                    self.view = *v;
                    if self.view.season > old {
                        self.notices.push(Notice::Resolved { season: old });
                    }
                }
                Evt::HostMsg(Msg::Info(s)) => self.notices.push(Notice::Info(s)),
                Evt::HostMsg(Msg::Deadline(secs)) => self.deadline = instant_in(secs),
                Evt::HostMsg(Msg::Reject { reason }) => {
                    self.lost = true;
                    self.notices.push(Notice::Lost(reason));
                }
                Evt::Disconnected | Evt::Failed(_) => {
                    if !self.lost {
                        self.lost = true;
                        self.notices.push(Notice::Lost("the host is gone. If they continue the game, join again under the same name.".into()));
                    }
                }
                _ => {}
            }
        }
        if self.last_ping.elapsed() >= PING_EVERY && !self.lost {
            self.last_ping = Instant::now();
            self.net.send(Cmd::Broadcast(Msg::Ping));
        }
        std::mem::take(&mut self.notices)
    }

    fn deadline(&self) -> Option<Instant> {
        self.deadline.filter(|_| !self.lost)
    }

    fn describe(&self) -> String {
        let people = self.view.nobles.iter().filter(|n| n.alive && !n.ai).count();
        format!("at a table of {people} player{}", if people == 1 { "" } else { "s" })
    }
}
