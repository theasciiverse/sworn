//! The game screen's state and what each key does. Drawing is in `draw.rs`.

use super::map::Flash;
use crate::chronicle::{EventKind, Outcome};
use crate::court::{Court, Notice};
use crate::diplomacy::Term;
use crate::orders::Order;
use crate::view::View;
use crate::{season_name, NobleId, ProvinceId};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Province,
    Orders,
    Letters,
    Nobles,
    Chronicle,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Province, Tab::Orders, Tab::Letters, Tab::Nobles, Tab::Chronicle];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Province => "Province",
            Tab::Orders => "Orders",
            Tab::Letters => "Letters",
            Tab::Nobles => "Nobles",
            Tab::Chronicle => "Chronicle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    To,
    Body,
    Give,
    Want,
    Send,
}

#[derive(Clone, Debug, Default)]
pub struct Compose {
    pub to: Vec<NobleId>,
    pub cursor: usize,
    pub body: String,
    pub gold_give: u32,
    pub gold_want: u32,
    pub prov_give: Vec<ProvinceId>,
    pub prov_want: Vec<ProvinceId>,
    pub fealty_give: bool,
    pub fealty_want: bool,
    pub give_cursor: usize,
    pub want_cursor: usize,
    pub field: Option<Field>,
    pub error: Option<String>,
}

impl Compose {
    pub fn field(&self) -> Field {
        self.field.unwrap_or(Field::To)
    }

    pub fn has_offer(&self) -> bool {
        self.gold_give > 0
            || self.gold_want > 0
            || !self.prov_give.is_empty()
            || !self.prov_want.is_empty()
            || self.fealty_give
            || self.fealty_want
    }

    pub fn terms(&self) -> (Vec<Term>, Vec<Term>) {
        let side = |gold: u32, provs: &[ProvinceId], fealty: bool| {
            let mut t = vec![];
            if gold > 0 {
                t.push(Term::Gold(gold));
            }
            t.extend(provs.iter().map(|&p| Term::Province(p)));
            if fealty {
                t.push(Term::Fealty);
            }
            t
        };
        (side(self.gold_give, &self.prov_give, self.fealty_give), side(self.gold_want, &self.prov_want, self.fealty_want))
    }
}

#[derive(Clone, Debug)]
pub enum Popup {
    Raise { at: ProvinceId, count: u32 },
    March { from: ProvinceId, dests: Vec<ProvinceId>, di: usize, count: u32, ally: Option<NobleId> },
    Compose(Box<Compose>),
    Read { letter: u32, scroll: u16 },
    Report { season: u32, scroll: u16 },
    ConfirmEnd,
    ConfirmQuit,
    Help,
    /// What to do in the first season of a new game.
    FirstSeason,
    Over { metric: usize },
}

pub struct Play {
    pub court: Box<dyn Court>,
    pub sel: ProvinceId,
    pub tab: Tab,
    pub popup: Option<Popup>,
    pub draft: Vec<Order>,
    pub list_sel: [usize; 5],
    pub status: String,
    pub hits: Vec<(Rect, ProvinceId)>,
    pub sidebar: Rect,
    /// Back to the title screen.
    pub leave: bool,
    /// Last season's fighting, to flash on the map once the report is closed.
    pub flashes: Vec<Flash>,
    /// When the flashing began (it waits for the map to be in view).
    pub flash_start: Option<Instant>,
    /// Letters opened this session, so the status line counts only unread ones.
    pub read: std::collections::HashSet<u32>,
}

/// How long last season's battles flash on the map, and how fast.
pub const FLASH_FOR: Duration = Duration::from_millis(3500);
const FLASH_BLINK: u128 = 300;

impl Play {
    /// A new game's first screen, with a word on what to do.
    pub fn first(court: Box<dyn Court>) -> Play {
        let mut p = Play::new(court);
        if p.view().season == 0 && p.popup.is_none() {
            p.popup = Some(Popup::FirstSeason);
        }
        p
    }

    pub fn new(court: Box<dyn Court>) -> Play {
        let v = court.view();
        let sel = v.mine().next().map_or(0, |p| p.id);
        let status = format!("{}: {}. Press ? for help.", season_name(v.season), court.describe());
        let mut p = Play {
            court,
            sel,
            tab: Tab::Province,
            popup: None,
            draft: vec![],
            list_sel: [0; 5],
            status,
            hits: vec![],
            sidebar: Rect::default(),
            leave: false,
            flashes: vec![],
            flash_start: None,
            read: Default::default(),
        };
        if p.view().winner.is_some() {
            p.popup = Some(Popup::Over { metric: 0 });
            p.status = "The game is over. Enter shows the standings again; q leaves.".into();
        }
        p
    }

    pub fn view(&self) -> &View {
        self.court.view()
    }

    // ---- the draft and what it commits -------------------------------

    pub fn raise_cost(&self) -> u32 {
        let v = self.view();
        v.noble(v.me).faction.raise_cost(v.rules.raise_cost).max(1)
    }

    pub fn raised_at(&self, p: ProvinceId) -> u32 {
        self.draft.iter().map(|o| if let Order::Raise { at, count } = o { if *at == p { *count } else { 0 } } else { 0 }).sum()
    }

    pub fn marching_from(&self, p: ProvinceId) -> u32 {
        self.draft
            .iter()
            .map(|o| if let Order::March { from, count, .. } = o { if *from == p { *count } else { 0 } } else { 0 })
            .sum()
    }

    pub fn planned_gold(&self) -> u32 {
        let raised: u32 = self.draft.iter().map(|o| if let Order::Raise { count, .. } = o { *count } else { 0 }).sum();
        let v = self.view();
        let promised: u32 = self
            .draft
            .iter()
            .filter_map(|o| if let Order::Accept(id) = o { v.offers.iter().find(|x| x.id == *id) } else { None })
            .flat_map(|o| o.want.iter())
            .map(|t| if let Term::Gold(g) = t { *g } else { 0 })
            .sum();
        raised * self.raise_cost() + promised
    }

    pub fn free_troops(&self, p: ProvinceId) -> u32 {
        let have = self.view().province(p).troops.unwrap_or(0) + self.raised_at(p);
        have.saturating_sub(self.marching_from(p))
    }

    pub fn raise_room(&self, p: ProvinceId) -> u32 {
        let v = self.view();
        let cap = v.rules.raise_cap * v.province(p).kind.raise_multiplier();
        let gold_left = v.gold.saturating_sub(self.planned_gold());
        cap.saturating_sub(self.raised_at(p)).min(gold_left / self.raise_cost())
    }

    pub fn marches(&self) -> Vec<(ProvinceId, ProvinceId)> {
        self.draft.iter().filter_map(|o| if let Order::March { from, to, .. } = o { Some((*from, *to)) } else { None }).collect()
    }

    pub fn accepted(&self, offer: u32) -> bool {
        self.draft.contains(&Order::Accept(offer))
    }

    /// Letters newest first.
    pub fn letters(&self) -> Vec<&crate::diplomacy::Letter> {
        self.view().letters.iter().rev().collect()
    }

    pub fn others(&self) -> Vec<NobleId> {
        let v = self.view();
        v.nobles.iter().filter(|n| n.alive && n.id != v.me).map(|n| n.id).collect()
    }

    // ---- input -----------------------------------------------------------

    /// Whether this frame shows last season's battles bright; None once they are done.
    pub fn flash_on(&self) -> bool {
        self.flash_start.is_some_and(|s| (s.elapsed().as_millis() / FLASH_BLINK).is_multiple_of(2))
    }

    pub fn tick(&mut self) {
        if !self.flashes.is_empty() && !matches!(self.popup, Some(Popup::Report { .. })) {
            match self.flash_start {
                None => self.flash_start = Some(Instant::now()),
                Some(s) if s.elapsed() >= FLASH_FOR => {
                    self.flashes.clear();
                    self.flash_start = None;
                }
                Some(_) => {}
            }
        }
        for n in self.court.poll() {
            match n {
                Notice::Resolved { season } => {
                    self.draft.clear();
                    let v = self.view();
                    let keep = v.provinces.get(self.sel as usize).is_some_and(|p| p.owner == Some(v.me));
                    let first = v.mine().next().map(|p| p.id).filter(|_| !keep);
                    if let Some(p) = first {
                        self.sel = p;
                    }
                    self.flashes = flashes(self.view(), season);
                    self.flash_start = None;
                    self.status = if self.view().winner.is_some() {
                        "The game is over. Enter shows the standings again; q leaves.".into()
                    } else {
                        format!("{}: your orders for the new season.", season_name(self.view().season))
                    };
                    self.popup = Some(if self.view().winner.is_some() { Popup::Over { metric: 0 } } else { Popup::Report { season, scroll: 0 } });
                }
                Notice::Info(s) => self.status = s,
                Notice::Lost(s) => {
                    self.status = format!("Connection lost: {s}");
                }
            }
        }
    }

    pub fn mouse(&mut self, m: MouseEvent) {
        let at = Position::new(m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(&(_, id)) = self.hits.iter().find(|(r, _)| r.contains(at)) else { return };
                match &mut self.popup {
                    Some(Popup::March { dests, di, .. }) => {
                        if let Some(i) = dests.iter().position(|&d| d == id) {
                            *di = i;
                        }
                    }
                    None => {
                        self.sel = id;
                        self.tab = Tab::Province;
                    }
                    _ => {}
                }
            }
            MouseEventKind::ScrollDown if self.sidebar.contains(at) && self.popup.is_none() => self.list_move(1),
            MouseEventKind::ScrollUp if self.sidebar.contains(at) && self.popup.is_none() => self.list_move(-1),
            _ => {}
        }
    }

    pub fn key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.popup = Some(Popup::ConfirmQuit);
            return;
        }
        if let Some(p) = self.popup.take() {
            self.popup = self.popup_key(p, k);
            return;
        }
        let v = self.view();
        let me = v.me;
        let mine = v.province(self.sel).owner == Some(me);
        let over = v.winner.is_some();
        match k.code {
            KeyCode::Left => self.move_sel(-1, 0),
            KeyCode::Right => self.move_sel(1, 0),
            KeyCode::Up => self.move_sel(0, -1),
            KeyCode::Down => self.move_sel(0, 1),
            KeyCode::Char('n') => self.cycle_mine(1),
            KeyCode::Char('N') => self.cycle_mine(-1),
            KeyCode::Tab => self.tab = Tab::ALL[(self.tab as usize + 1) % 5],
            KeyCode::BackTab => self.tab = Tab::ALL[(self.tab as usize + 4) % 5],
            KeyCode::Char(c @ '1'..='5') => self.tab = Tab::ALL[c as usize - '1' as usize],
            KeyCode::Char('j') | KeyCode::PageDown => self.list_move(1),
            KeyCode::Char('k') | KeyCode::PageUp => self.list_move(-1),
            KeyCode::Char('?') | KeyCode::F(1) => self.popup = Some(Popup::Help),
            KeyCode::Char('q') | KeyCode::Esc => self.popup = Some(Popup::ConfirmQuit),
            _ if over => {
                if k.code == KeyCode::Enter {
                    self.popup = Some(Popup::Over { metric: 0 });
                }
            }
            KeyCode::Char('r') if self.tab == Tab::Letters => self.reply(false),
            KeyCode::Char('R') if self.tab == Tab::Letters => self.reply(true),
            KeyCode::Char('a') if self.tab == Tab::Letters => self.toggle_accept(),
            KeyCode::Enter if self.tab == Tab::Letters => {
                if let Some(id) = self.letters().get(self.list_sel[Tab::Letters as usize]).map(|l| l.id) {
                    self.read.insert(id);
                    self.popup = Some(Popup::Read { letter: id, scroll: 0 });
                }
            }
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace if self.tab == Tab::Orders => {
                let i = self.list_sel[Tab::Orders as usize];
                if i < self.draft.len() {
                    self.draft.remove(i);
                    self.list_sel[Tab::Orders as usize] = i.min(self.draft.len().saturating_sub(1));
                }
            }
            KeyCode::Char('w') => {
                let to = if self.tab == Tab::Nobles {
                    self.view().nobles.get(self.list_sel[Tab::Nobles as usize]).filter(|n| n.alive && n.id != me).map(|n| n.id)
                } else {
                    self.view().province(self.sel).owner.filter(|&o| o != me)
                };
                self.compose(to.into_iter().collect());
            }
            KeyCode::Char('r') if mine => {
                if self.view().submitted {
                    self.status = "Your orders are in; press u to take them back first.".into();
                } else {
                    self.popup = Some(Popup::Raise { at: self.sel, count: self.raise_room(self.sel).min(1) });
                }
            }
            KeyCode::Char('m') if mine => self.start_march(),
            KeyCode::Char('r') | KeyCode::Char('m') => self.status = "That province is not yours.".into(),
            KeyCode::Char('e') => {
                if self.view().submitted {
                    self.status = "Your orders are in; waiting for the others.".into();
                } else {
                    self.popup = Some(Popup::ConfirmEnd);
                }
            }
            KeyCode::Char('u') => match self.court.unsubmit() {
                Ok(()) => self.status = "Orders taken back.".into(),
                Err(e) => self.status = e,
            },
            KeyCode::Char('s') => {
                self.status = match self.court.save() {
                    Ok(p) => format!("Saved to {p}."),
                    Err(e) => format!("Not saved: {e}."),
                }
            }
            _ => {}
        }
    }

    fn move_sel(&mut self, dx: i32, dy: i32) {
        if let Some(p) = super::map::step(self.view(), self.sel, dx, dy) {
            self.sel = p;
            if self.tab != Tab::Province && self.tab != Tab::Orders {
                self.tab = Tab::Province;
            }
        }
    }

    fn cycle_mine(&mut self, d: i32) {
        let mine: Vec<ProvinceId> = self.view().mine().map(|p| p.id).collect();
        if mine.is_empty() {
            return;
        }
        let i = mine.iter().position(|&p| p == self.sel).map_or(0, |i| (i as i32 + d).rem_euclid(mine.len() as i32) as usize);
        self.sel = mine[i];
        self.tab = Tab::Province;
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Province => 0,
            Tab::Orders => self.draft.len(),
            Tab::Letters => self.view().letters.len(),
            Tab::Nobles => self.view().nobles.len(),
            Tab::Chronicle => self.view().events.len(),
        }
    }

    fn list_move(&mut self, d: i32) {
        let len = self.list_len();
        let s = &mut self.list_sel[self.tab as usize];
        *s = if len == 0 { 0 } else { (*s as i32 + d).clamp(0, len as i32 - 1) as usize };
    }

    fn start_march(&mut self) {
        if self.view().submitted {
            self.status = "Your orders are in; press u to take them back first.".into();
            return;
        }
        let from = self.sel;
        let free = self.free_troops(from);
        if free == 0 {
            self.status = "No troops free to march from here.".into();
            return;
        }
        let dests = self.view().province(from).neighbours.clone();
        self.popup = Some(Popup::March { from, dests, di: 0, count: free, ally: None });
    }

    pub fn compose(&mut self, to: Vec<NobleId>) {
        if self.others().is_empty() {
            self.status = "There is no one left to write to.".into();
            return;
        }
        let c = Compose { to, field: Some(Field::To), ..Default::default() };
        self.popup = Some(Popup::Compose(Box::new(c)));
    }

    fn reply(&mut self, all: bool) {
        let me = self.view().me;
        let Some(l) = self.letters().get(self.list_sel[Tab::Letters as usize]).map(|l| (*l).clone()) else { return };
        let mut to: Vec<NobleId> = if all { l.to.iter().copied().chain([l.from]).collect() } else { vec![l.from] };
        if !all && l.from == me {
            to = l.to.clone();
        }
        to.retain(|&n| n != me && self.view().noble(n).alive);
        to.sort_unstable();
        to.dedup();
        self.compose(to);
        if let Some(Popup::Compose(c)) = &mut self.popup {
            c.field = Some(Field::Body);
        }
    }

    fn toggle_accept(&mut self) {
        let me = self.view().me;
        let Some(l) = self.letters().get(self.list_sel[Tab::Letters as usize]).map(|l| (*l).clone()) else { return };
        let Some(id) = l.offer else {
            self.status = "That letter carries no offer.".into();
            return;
        };
        let Some(o) = self.view().offers.iter().find(|o| o.id == id).cloned() else {
            self.status = "That offer is no longer open.".into();
            return;
        };
        if o.to != me {
            self.status = "That is your own offer; only they can accept it.".into();
            return;
        }
        if self.view().submitted {
            self.status = "Your orders are in; press u to take them back first.".into();
            return;
        }
        if let Some(i) = self.draft.iter().position(|x| *x == Order::Accept(id)) {
            self.draft.remove(i);
            self.status = "You will not accept it after all.".into();
        } else {
            self.draft.push(Order::Accept(id));
            self.status = "Accepted: it takes effect when the season ends, if both sides can still honour it.".into();
        }
    }

    fn popup_key(&mut self, p: Popup, k: KeyEvent) -> Option<Popup> {
        let esc = k.code == KeyCode::Esc;
        match p {
            Popup::Raise { at, count } => {
                let room = self.raise_room(at);
                match k.code {
                    _ if esc => None,
                    KeyCode::Left | KeyCode::Char('-') => Some(Popup::Raise { at, count: count.saturating_sub(1) }),
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => Some(Popup::Raise { at, count: (count + 1).min(room) }),
                    KeyCode::Home => Some(Popup::Raise { at, count: 0 }),
                    KeyCode::End => Some(Popup::Raise { at, count: room }),
                    KeyCode::Enter => {
                        if count > 0 {
                            self.draft.push(Order::Raise { at, count });
                            self.status = format!("Raise {count} at {}.", self.view().province(at).name);
                        }
                        None
                    }
                    _ => Some(Popup::Raise { at, count }),
                }
            }
            Popup::March { from, dests, mut di, mut count, mut ally } => {
                let free = self.free_troops(from);
                match k.code {
                    _ if esc => return None,
                    KeyCode::Up | KeyCode::Char('k') => di = (di + dests.len() - 1) % dests.len(),
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => di = (di + 1) % dests.len(),
                    KeyCode::Left | KeyCode::Char('-') => count = count.saturating_sub(1).max(1),
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => count = (count + 1).min(free),
                    KeyCode::Home => count = 1,
                    KeyCode::End => count = free,
                    KeyCode::Char('b') => {
                        let others = self.others();
                        ally = match ally {
                            None => others.first().copied(),
                            Some(a) => others.iter().position(|&o| o == a).and_then(|i| others.get(i + 1).copied()),
                        };
                    }
                    KeyCode::Enter => {
                        let to = dests[di];
                        self.draft.push(Order::March { from, to, count, on_behalf: ally });
                        let v = self.view();
                        self.status = format!("March {count} from {} to {}.", v.province(from).name, v.province(to).name);
                        return None;
                    }
                    _ => {}
                }
                Some(Popup::March { from, dests, di, count, ally })
            }
            Popup::Compose(mut c) => {
                if esc {
                    return None;
                }
                if self.compose_key(&mut c, k) {
                    None
                } else {
                    Some(Popup::Compose(c))
                }
            }
            Popup::Read { letter, scroll } => match k.code {
                KeyCode::Down | KeyCode::Char('j') => Some(Popup::Read { letter, scroll: scroll.saturating_add(1) }),
                KeyCode::Up | KeyCode::Char('k') => Some(Popup::Read { letter, scroll: scroll.saturating_sub(1) }),
                KeyCode::Char('a') => {
                    self.toggle_accept();
                    Some(Popup::Read { letter, scroll })
                }
                KeyCode::Char('r') => {
                    self.reply(false);
                    self.popup.take()
                }
                KeyCode::Char('R') => {
                    self.reply(true);
                    self.popup.take()
                }
                _ => None,
            },
            Popup::Report { season, scroll } => match k.code {
                KeyCode::Down | KeyCode::Char('j') => Some(Popup::Report { season, scroll: scroll.saturating_add(1) }),
                KeyCode::Up | KeyCode::Char('k') => Some(Popup::Report { season, scroll: scroll.saturating_sub(1) }),
                _ => None,
            },
            Popup::ConfirmEnd => {
                if matches!(k.code, KeyCode::Char('y') | KeyCode::Enter | KeyCode::Char('e')) {
                    let orders = self.draft.clone();
                    match self.court.submit(orders) {
                        Ok(()) => self.status = "Orders sent.".into(),
                        Err(e) => self.status = format!("Not sent: {e}"),
                    }
                }
                None
            }
            Popup::ConfirmQuit => {
                if matches!(k.code, KeyCode::Char('y') | KeyCode::Enter | KeyCode::Char('q')) {
                    let _ = self.court.save();
                    self.leave = true;
                }
                None
            }
            Popup::Help => None,
            Popup::FirstSeason => (k.code == KeyCode::Char('?')).then_some(Popup::Help),
            Popup::Over { metric } => match k.code {
                KeyCode::Char('q') | KeyCode::Enter => {
                    self.leave = true;
                    None
                }
                KeyCode::Tab | KeyCode::Right => Some(Popup::Over { metric: (metric + 1) % super::over::METRICS.len() }),
                KeyCode::BackTab | KeyCode::Left => {
                    Some(Popup::Over { metric: (metric + super::over::METRICS.len() - 1) % super::over::METRICS.len() })
                }
                _ => None,
            },
        }
    }

    /// Returns true when the letter is sent.
    fn compose_key(&mut self, c: &mut Compose, k: KeyEvent) -> bool {
        let others = self.others();
        let order = [Field::To, Field::Body, Field::Give, Field::Want, Field::Send];
        let fi = order.iter().position(|&f| f == c.field()).unwrap_or(0);
        match k.code {
            KeyCode::Tab => {
                c.field = Some(order[(fi + 1) % order.len()]);
                return false;
            }
            KeyCode::BackTab => {
                c.field = Some(order[(fi + order.len() - 1) % order.len()]);
                return false;
            }
            _ => {}
        }
        c.error = None;
        let v = self.view();
        let mine: Vec<ProvinceId> = v.mine().map(|p| p.id).collect();
        let theirs: Vec<ProvinceId> = match c.to.as_slice() {
            [one] => v.provinces.iter().filter(|p| p.owner == Some(*one)).map(|p| p.id).collect(),
            _ => vec![],
        };
        match c.field() {
            Field::To => match k.code {
                KeyCode::Left | KeyCode::Up => c.cursor = (c.cursor + others.len() - 1) % others.len(),
                KeyCode::Right | KeyCode::Down => c.cursor = (c.cursor + 1) % others.len(),
                KeyCode::Char(' ') | KeyCode::Enter => {
                    let n = others[c.cursor % others.len()];
                    if let Some(i) = c.to.iter().position(|&t| t == n) {
                        c.to.remove(i);
                    } else {
                        c.to.push(n);
                    }
                    c.prov_want.clear();
                }
                KeyCode::Char('a') => {
                    c.to = if c.to.len() == others.len() { vec![] } else { others.clone() };
                    c.prov_want.clear();
                }
                _ => {}
            },
            Field::Body => match k.code {
                KeyCode::Char(ch) => {
                    if c.body.len() < crate::diplomacy::MAX_LETTER - 4 {
                        c.body.push(ch);
                    }
                }
                KeyCode::Enter => c.body.push('\n'),
                KeyCode::Backspace => {
                    c.body.pop();
                }
                _ => {}
            },
            Field::Give | Field::Want => {
                let give = c.field() == Field::Give;
                let (pool, gold, provs, fealty, cursor) = if give {
                    (&mine, &mut c.gold_give, &mut c.prov_give, &mut c.fealty_give, &mut c.give_cursor)
                } else {
                    (&theirs, &mut c.gold_want, &mut c.prov_want, &mut c.fealty_want, &mut c.want_cursor)
                };
                match k.code {
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => *gold += 1,
                    KeyCode::Left | KeyCode::Char('-') => *gold = gold.saturating_sub(1),
                    KeyCode::PageUp => *gold += 10,
                    KeyCode::PageDown => *gold = gold.saturating_sub(10),
                    KeyCode::Down if !pool.is_empty() => *cursor = (*cursor + 1) % pool.len(),
                    KeyCode::Up if !pool.is_empty() => *cursor = (*cursor + pool.len() - 1) % pool.len(),
                    KeyCode::Char(' ') | KeyCode::Enter if !pool.is_empty() => {
                        let p = pool[*cursor % pool.len()];
                        if let Some(i) = provs.iter().position(|&x| x == p) {
                            provs.remove(i);
                        } else {
                            provs.push(p);
                        }
                    }
                    KeyCode::Char('f') => *fealty = !*fealty,
                    _ => {}
                }
            }
            Field::Send => {
                if matches!(k.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    if c.to.is_empty() {
                        c.error = Some("Choose who it goes to.".into());
                        return false;
                    }
                    if c.body.trim().is_empty() && !c.has_offer() {
                        c.error = Some("Write something, or make an offer.".into());
                        return false;
                    }
                    let offer = c.has_offer().then(|| c.terms());
                    match self.court.send_letter(&c.to, c.body.trim_end(), offer) {
                        Ok(()) => {
                            let v = self.view();
                            let names: Vec<&str> = c.to.iter().map(|&n| v.noble(n).name.as_str()).collect();
                            self.status = format!("Letter sent to {}.", names.join(", "));
                            self.tab = Tab::Letters;
                            self.list_sel[Tab::Letters as usize] = 0;
                            return true;
                        }
                        Err(e) => c.error = Some(e),
                    }
                }
            }
        }
        false
    }
}

/// The battles and road clashes of `season` that this noble knows of.
pub fn flashes(v: &View, season: u32) -> Vec<Flash> {
    v.events
        .iter()
        .filter(|e| e.season == season)
        .filter_map(|e| match &e.kind {
            EventKind::Battle { at, outcome, .. } => Some(Flash::Battle { at: *at, taken: matches!(outcome, Outcome::Taken(_)) }),
            EventKind::Clash { between: (a, b), .. } => Some(Flash::Clash(*a, *b)),
            _ => None,
        })
        .collect()
}
