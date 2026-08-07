//! Deterministic authoritative M0 simulation kernel.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SNAPSHOT_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);
impl EntityId {
    pub fn from_parts(i: u32, g: u32) -> Self {
        Self((u64::from(g) << 32) | u64::from(i))
    }
    pub fn index(self) -> usize {
        self.0 as u32 as usize
    }
    pub fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }
    pub fn raw(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Position {
    pub x_mm: i32,
    pub y_mm: i32,
    pub cell: u32,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Role {
    #[default]
    Rifle,
    Medic,
    Officer,
    Logistics,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Inventory {
    pub food: u32,
    pub water: u32,
    pub medical: u32,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Needs {
    pub fatigue: u32,
    pub hunger: u32,
    pub thirst: u32,
    pub sleep_debt: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Soldier {
    pub id: EntityId,
    pub faction: u16,
    pub position: Position,
    pub squad: Option<u32>,
    pub role: Role,
    pub rank: u8,
    pub health: u16,
    pub needs: Needs,
    pub ammunition: u32,
    pub inventory: Inventory,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoldierSpec {
    pub faction: u16,
    pub position: Position,
    pub squad: Option<u32>,
    pub role: Role,
    pub rank: u8,
    pub health: u16,
    pub ammunition: u32,
    pub inventory: Inventory,
}
impl Default for SoldierSpec {
    fn default() -> Self {
        Self {
            faction: 0,
            position: Position::default(),
            squad: None,
            role: Role::Rifle,
            rank: 0,
            health: 1000,
            ammunition: 0,
            inventory: Inventory::default(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Squad {
    pub id: u32,
    pub officer: Option<EntityId>,
    pub members: BTreeSet<EntityId>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stock {
    pub ammunition: u64,
    pub supplies: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Loadout {
    pub ammunition: u32,
    pub food: u32,
    pub water: u32,
    pub medical: u32,
}
impl From<SoldierSpec> for Loadout {
    fn from(s: SoldierSpec) -> Self {
        Self {
            ammunition: s.ammunition,
            food: s.inventory.food,
            water: s.inventory.water,
            medical: s.inventory.medical,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledCommand {
    Transfer {
        from: u32,
        to: u32,
        ammunition: u64,
        supplies: u64,
    },
    SetRegionHot {
        cell: u32,
        hot: bool,
    },
    CreateStockpile {
        id: u32,
        initial: Stock,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    SpawnSoldier {
        spec: SoldierSpec,
    },
    DespawnSoldier {
        id: EntityId,
    },
    CreateSquad {
        id: u32,
    },
    AssignOfficer {
        squad: u32,
        officer: EntityId,
    },
    CreateStockpile {
        id: u32,
        initial: Stock,
    },
    Transfer {
        from: u32,
        to: u32,
        ammunition: u64,
        supplies: u64,
    },
    SetRegionHot {
        cell: u32,
        hot: bool,
    },
    Schedule {
        at: u64,
        command: ScheduledCommand,
    },
    CancelScheduled {
        id: u64,
    },
    AdvanceTo {
        target: u64,
    },
    NextRandom,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    SoldierSpawned { id: EntityId, loadout: Loadout },
    SoldierRemoved { id: EntityId, loadout: Loadout },
    SquadCreated { id: u32 },
    OfficerAssigned { squad: u32, officer: EntityId },
    StockpileCreated { id: u32, initial: Stock },
    TransferCompleted { from: u32, to: u32, stock: Stock },
    RegionFidelityChanged { cell: u32, hot: bool },
    Scheduled { id: u64, at: u64 },
    ScheduleCancelled { id: u64, at: u64 },
    RandomGenerated { value: u64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedEvent {
    pub at: u64,
    pub event: Event,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyOutcome {
    pub clock: u64,
    pub events: Vec<TimedEvent>,
    pub error: Option<SimError>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimError {
    InvalidEntity,
    TimeReversal,
    UnknownStockpile,
    InsufficientStock,
    InvalidTransfer,
    StockpileAlreadyExists,
    ArithmeticOverflow,
    InvalidSquad,
    SquadAlreadyExists,
    InvalidOfficerRole,
    InvalidScheduledCommand,
    UnknownScheduledCommand,
    Snapshot(&'static str),
}
impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SimError {}

#[derive(Clone, Default)]
struct Soldiers {
    generation: Vec<u32>,
    alive: Vec<bool>,
    data: Vec<SoldierSpec>,
    needs_at: Vec<u64>,
    free: Vec<u32>,
    live: usize,
}
impl Soldiers {
    fn valid(&self, id: EntityId) -> bool {
        id.index() < self.alive.len()
            && self.alive[id.index()]
            && self.generation[id.index()] == id.generation()
    }
    fn spawn(&mut self, s: SoldierSpec, now: u64) -> EntityId {
        let i = if let Some(i) = self.free.pop() {
            i as usize
        } else {
            self.generation.push(0);
            self.alive.push(false);
            self.data.push(SoldierSpec::default());
            self.needs_at.push(now);
            self.alive.len() - 1
        };
        self.alive[i] = true;
        self.data[i] = s;
        self.needs_at[i] = now;
        self.live += 1;
        EntityId::from_parts(i as u32, self.generation[i])
    }
    fn remove(&mut self, id: EntityId) -> Option<SoldierSpec> {
        if !self.valid(id) {
            return None;
        }
        let i = id.index();
        self.alive[i] = false;
        self.live -= 1;
        if self.generation[i] != u32::MAX {
            self.generation[i] += 1;
            self.free.push(i as u32)
        }
        Some(self.data[i])
    }
    fn needs(&self, i: usize, now: u64) -> Needs {
        let e = now - self.needs_at[i];
        Needs {
            fatigue: u32::try_from(e).unwrap_or(u32::MAX),
            hunger: u32::try_from(e / 3).unwrap_or(u32::MAX),
            thirst: u32::try_from(e / 2).unwrap_or(u32::MAX),
            sleep_debt: u32::try_from(e / 4).unwrap_or(u32::MAX),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HotCellState {
    pub activated_at: u64,
    pub last_stepped_at: u64,
    pub fixed_steps: u64,
}
#[derive(Clone, Copy)]
struct Pending {
    id: u64,
    command: ScheduledCommand,
}
#[derive(Clone)]
pub struct World {
    clock: u64,
    seed: u64,
    rng_counter: u64,
    next_schedule_id: u64,
    soldiers: Soldiers,
    squads: BTreeMap<u32, Squad>,
    stockpiles: BTreeMap<u32, Stock>,
    hot_cells: BTreeMap<u32, HotCellState>,
    scheduled: BTreeMap<u64, Vec<Pending>>,
}
impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            clock: 0,
            seed,
            rng_counter: 0,
            next_schedule_id: 0,
            soldiers: Soldiers::default(),
            squads: BTreeMap::new(),
            stockpiles: BTreeMap::new(),
            hot_cells: BTreeMap::new(),
            scheduled: BTreeMap::new(),
        }
    }
    pub fn clock(&self) -> u64 {
        self.clock
    }
    pub fn soldier_count(&self) -> usize {
        self.soldiers.live
    }
    pub fn stockpile(&self, id: u32) -> Option<Stock> {
        self.stockpiles.get(&id).copied()
    }
    pub fn squad(&self, id: u32) -> Option<&Squad> {
        self.squads.get(&id)
    }
    pub fn hot_cell(&self, id: u32) -> Option<HotCellState> {
        self.hot_cells.get(&id).copied()
    }
    pub fn hot_cell_count(&self) -> usize {
        self.hot_cells.len()
    }
    pub fn soldier(&self, id: EntityId) -> Option<Soldier> {
        if !self.soldiers.valid(id) {
            return None;
        }
        let i = id.index();
        let s = self.soldiers.data[i];
        Some(Soldier {
            id,
            faction: s.faction,
            position: s.position,
            squad: s.squad,
            role: s.role,
            rank: s.rank,
            health: s.health,
            needs: self.soldiers.needs(i, self.clock),
            ammunition: s.ammunition,
            inventory: s.inventory,
        })
    }
    pub fn apply(&mut self, c: Command) -> ApplyOutcome {
        let mut events = Vec::new();
        let error = match c {
            Command::AdvanceTo { target } => self.advance(target, &mut events),
            _ => self.apply_one(c).map(|e| {
                events.push(TimedEvent {
                    at: self.clock,
                    event: e,
                })
            }),
        }
        .err();
        ApplyOutcome {
            clock: self.clock,
            events,
            error,
        }
    }
    fn apply_one(&mut self, c: Command) -> Result<Event, SimError> {
        match c {
            Command::SpawnSoldier { spec } => {
                if let Some(s) = spec.squad {
                    if !self.squads.contains_key(&s) {
                        return Err(SimError::InvalidSquad);
                    }
                }
                let id = self.soldiers.spawn(spec, self.clock);
                if let Some(s) = spec.squad {
                    self.squads.get_mut(&s).expect("checked").members.insert(id);
                }
                Ok(Event::SoldierSpawned {
                    id,
                    loadout: spec.into(),
                })
            }
            Command::DespawnSoldier { id } => {
                let spec = *self
                    .soldiers
                    .data
                    .get(id.index())
                    .filter(|_| self.soldiers.valid(id))
                    .ok_or(SimError::InvalidEntity)?;
                if let Some(s) = spec.squad {
                    let q = self.squads.get_mut(&s).expect("valid relationship");
                    q.members.remove(&id);
                    if q.officer == Some(id) {
                        q.officer = None
                    }
                }
                self.soldiers.remove(id);
                Ok(Event::SoldierRemoved {
                    id,
                    loadout: spec.into(),
                })
            }
            Command::CreateSquad { id } => {
                if self.squads.contains_key(&id) {
                    return Err(SimError::SquadAlreadyExists);
                }
                self.squads.insert(
                    id,
                    Squad {
                        id,
                        officer: None,
                        members: BTreeSet::new(),
                    },
                );
                Ok(Event::SquadCreated { id })
            }
            Command::AssignOfficer { squad, officer } => {
                if !self.squads.contains_key(&squad) {
                    return Err(SimError::InvalidSquad);
                }
                if !self.soldiers.valid(officer) {
                    return Err(SimError::InvalidEntity);
                }
                if self.soldiers.data[officer.index()].role != Role::Officer {
                    return Err(SimError::InvalidOfficerRole);
                }
                if let Some(old) = self.soldiers.data[officer.index()].squad {
                    let q = self.squads.get_mut(&old).expect("valid relationship");
                    q.members.remove(&officer);
                    if q.officer == Some(officer) {
                        q.officer = None
                    }
                }
                let q = self.squads.get_mut(&squad).expect("checked");
                q.members.insert(officer);
                q.officer = Some(officer);
                self.soldiers.data[officer.index()].squad = Some(squad);
                Ok(Event::OfficerAssigned { squad, officer })
            }
            Command::CreateStockpile { id, initial } => {
                self.apply_scheduled(ScheduledCommand::CreateStockpile { id, initial })
            }
            Command::Transfer {
                from,
                to,
                ammunition,
                supplies,
            } => self.apply_scheduled(ScheduledCommand::Transfer {
                from,
                to,
                ammunition,
                supplies,
            }),
            Command::SetRegionHot { cell, hot } => {
                self.apply_scheduled(ScheduledCommand::SetRegionHot { cell, hot })
            }
            Command::Schedule { at, command } => {
                if at < self.clock {
                    return Err(SimError::TimeReversal);
                }
                self.validate_schedule(command)?;
                let id = self.next_schedule_id;
                self.next_schedule_id = self
                    .next_schedule_id
                    .checked_add(1)
                    .ok_or(SimError::ArithmeticOverflow)?;
                self.scheduled
                    .entry(at)
                    .or_default()
                    .push(Pending { id, command });
                Ok(Event::Scheduled { id, at })
            }
            Command::CancelScheduled { id } => {
                let found = self.scheduled.iter().find_map(|(at, v)| {
                    v.iter().position(|p| p.id == id).map(|index| (*at, index))
                });
                let (at, index) = found.ok_or(SimError::UnknownScheduledCommand)?;
                let queue = self.scheduled.get_mut(&at).expect("found");
                queue.remove(index);
                if queue.is_empty() {
                    self.scheduled.remove(&at);
                }
                Ok(Event::ScheduleCancelled { id, at })
            }
            Command::NextRandom => {
                let mut z = self
                    .seed
                    .wrapping_add(self.rng_counter.wrapping_mul(0x9e3779b97f4a7c15));
                self.rng_counter = self.rng_counter.wrapping_add(1);
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
                Ok(Event::RandomGenerated {
                    value: z ^ (z >> 31),
                })
            }
            Command::AdvanceTo { .. } => unreachable!(),
        }
    }
    fn validate_schedule(&self, c: ScheduledCommand) -> Result<(), SimError> {
        match c {
            ScheduledCommand::Transfer { from, to, .. } if from == to => {
                Err(SimError::InvalidTransfer)
            }
            ScheduledCommand::CreateStockpile { id, .. } if self.stockpiles.contains_key(&id) => {
                Err(SimError::StockpileAlreadyExists)
            }
            _ => Ok(()),
        }
    }
    fn apply_scheduled(&mut self, c: ScheduledCommand) -> Result<Event, SimError> {
        match c {
            ScheduledCommand::CreateStockpile { id, initial } => {
                if self.stockpiles.contains_key(&id) {
                    return Err(SimError::StockpileAlreadyExists);
                }
                self.stockpiles.insert(id, initial);
                Ok(Event::StockpileCreated { id, initial })
            }
            ScheduledCommand::Transfer {
                from,
                to,
                ammunition,
                supplies,
            } => {
                if from == to {
                    return Err(SimError::InvalidTransfer);
                }
                let a = *self
                    .stockpiles
                    .get(&from)
                    .ok_or(SimError::UnknownStockpile)?;
                let b = *self.stockpiles.get(&to).ok_or(SimError::UnknownStockpile)?;
                if a.ammunition < ammunition || a.supplies < supplies {
                    return Err(SimError::InsufficientStock);
                }
                let nb = Stock {
                    ammunition: b
                        .ammunition
                        .checked_add(ammunition)
                        .ok_or(SimError::ArithmeticOverflow)?,
                    supplies: b
                        .supplies
                        .checked_add(supplies)
                        .ok_or(SimError::ArithmeticOverflow)?,
                };
                self.stockpiles.insert(
                    from,
                    Stock {
                        ammunition: a.ammunition - ammunition,
                        supplies: a.supplies - supplies,
                    },
                );
                self.stockpiles.insert(to, nb);
                Ok(Event::TransferCompleted {
                    from,
                    to,
                    stock: Stock {
                        ammunition,
                        supplies,
                    },
                })
            }
            ScheduledCommand::SetRegionHot { cell, hot } => {
                if hot {
                    self.hot_cells.entry(cell).or_insert(HotCellState {
                        activated_at: self.clock,
                        last_stepped_at: self.clock,
                        fixed_steps: 0,
                    });
                } else {
                    self.hot_cells.remove(&cell);
                }
                Ok(Event::RegionFidelityChanged { cell, hot })
            }
        }
    }
    fn step_hot_to(&mut self, t: u64) -> Result<(), SimError> {
        for h in self.hot_cells.values() {
            h.fixed_steps
                .checked_add(t - h.last_stepped_at)
                .ok_or(SimError::ArithmeticOverflow)?;
        }
        for h in self.hot_cells.values_mut() {
            let d = t - h.last_stepped_at;
            h.fixed_steps += d;
            h.last_stepped_at = t;
        }
        Ok(())
    }
    fn advance(&mut self, target: u64, out: &mut Vec<TimedEvent>) -> Result<(), SimError> {
        if target < self.clock {
            return Err(SimError::TimeReversal);
        }
        let times: Vec<_> = self.scheduled.range(..=target).map(|(t, _)| *t).collect();
        for t in times {
            self.step_hot_to(t)?;
            self.clock = t;
            if let Some(mut v) = self.scheduled.remove(&t) {
                for i in 0..v.len() {
                    match self.apply_scheduled(v[i].command) {
                        Ok(event) => out.push(TimedEvent { at: t, event }),
                        Err(e) => {
                            self.scheduled.insert(t, v.split_off(i));
                            return Err(e);
                        }
                    }
                }
            }
        }
        self.step_hot_to(target)?;
        self.clock = target;
        Ok(())
    }
    pub fn state_digest(&self) -> u64 {
        fnv1a(&self.snapshot())
    }
    pub fn needs_checksum(&self) -> u64 {
        let mut h = 0xcbf29ce484222325;
        for i in 0..self.soldiers.alive.len() {
            if self.soldiers.alive[i] {
                let id = EntityId::from_parts(i as u32, self.soldiers.generation[i]);
                let n = self.soldiers.needs(i, self.clock);
                for b in id
                    .raw()
                    .to_le_bytes()
                    .into_iter()
                    .chain(n.fatigue.to_le_bytes())
                    .chain(n.hunger.to_le_bytes())
                    .chain(n.thirst.to_le_bytes())
                    .chain(n.sleep_debt.to_le_bytes())
                {
                    h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
                }
            }
        }
        h
    }
    pub fn resource_totals(&self) -> (u128, u128, u128, u128) {
        let mut a = self
            .stockpiles
            .values()
            .map(|s| u128::from(s.ammunition))
            .sum();
        let mut f = 0;
        let mut w = 0;
        let mut m = 0;
        for (i, live) in self.soldiers.alive.iter().enumerate() {
            if *live {
                let s = self.soldiers.data[i];
                a += u128::from(s.ammunition);
                f += u128::from(s.inventory.food);
                w += u128::from(s.inventory.water);
                m += u128::from(s.inventory.medical)
            }
        }
        (a, f, w, m)
    }
    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u32(SNAPSHOT_VERSION);
        w.u64(self.clock);
        w.u64(self.seed);
        w.u64(self.rng_counter);
        w.u64(self.next_schedule_id);
        w.u32(self.soldiers.alive.len() as u32);
        for i in 0..self.soldiers.alive.len() {
            w.u32(self.soldiers.generation[i]);
            w.bool(self.soldiers.alive[i]);
            if self.soldiers.alive[i] {
                w.spec(self.soldiers.data[i]);
                w.u64(self.soldiers.needs_at[i])
            }
        }
        w.u32(self.soldiers.free.len() as u32);
        for x in &self.soldiers.free {
            w.u32(*x)
        }
        w.u32(self.squads.len() as u32);
        for q in self.squads.values() {
            w.u32(q.id);
            w.opt_id(q.officer);
            w.u32(q.members.len() as u32);
            for x in &q.members {
                w.u64(x.raw())
            }
        }
        w.u32(self.stockpiles.len() as u32);
        for (id, s) in &self.stockpiles {
            w.u32(*id);
            w.stock(*s)
        }
        w.u32(self.hot_cells.len() as u32);
        for (id, h) in &self.hot_cells {
            w.u32(*id);
            w.u64(h.activated_at);
            w.u64(h.last_stepped_at);
            w.u64(h.fixed_steps)
        }
        w.u32(self.scheduled.len() as u32);
        for (at, v) in &self.scheduled {
            w.u64(*at);
            w.u32(v.len() as u32);
            for p in v {
                w.u64(p.id);
                w.sc(p.command)
            }
        }
        w.0
    }
    pub fn from_snapshot(b: &[u8]) -> Result<Self, SimError> {
        let mut r = R { b, p: 0 };
        if r.u32() != Ok(SNAPSHOT_VERSION) {
            return Err(SimError::Snapshot("unsupported version"));
        }
        let clock = r.u64()?;
        let seed = r.u64()?;
        let rng_counter = r.u64()?;
        let next_schedule_id = r.u64()?;
        let mut soldiers = Soldiers::default();
        for _ in 0..r.u32()? {
            soldiers.generation.push(r.u32()?);
            let alive = r.bool()?;
            soldiers.alive.push(alive);
            soldiers.data.push(if alive {
                soldiers.live += 1;
                r.spec()?
            } else {
                SoldierSpec::default()
            });
            soldiers.needs_at.push(if alive { r.u64()? } else { clock })
        }
        let mut seen = BTreeSet::new();
        for _ in 0..r.u32()? {
            let x = r.u32()?;
            if x as usize >= soldiers.alive.len()
                || soldiers.alive[x as usize]
                || soldiers.generation[x as usize] == u32::MAX
                || !seen.insert(x)
            {
                return Err(SimError::Snapshot("invalid free slot"));
            }
            soldiers.free.push(x)
        }
        let retired = soldiers
            .alive
            .iter()
            .zip(&soldiers.generation)
            .filter(|(a, g)| !**a && **g == u32::MAX)
            .count();
        if soldiers.free.len() + retired != soldiers.alive.len() - soldiers.live {
            return Err(SimError::Snapshot("incomplete free list"));
        }
        let mut squads = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            let officer = r.opt_id()?;
            let mut members = BTreeSet::new();
            for _ in 0..r.u32()? {
                if !members.insert(EntityId(r.u64()?)) {
                    return Err(SimError::Snapshot("duplicate squad member"));
                }
            }
            if squads
                .insert(
                    id,
                    Squad {
                        id,
                        officer,
                        members,
                    },
                )
                .is_some()
            {
                return Err(SimError::Snapshot("duplicate squad"));
            }
        }
        let mut stockpiles = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            if stockpiles.insert(id, r.stock()?).is_some() {
                return Err(SimError::Snapshot("duplicate stockpile"));
            }
        }
        let mut hot_cells = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            let h = HotCellState {
                activated_at: r.u64()?,
                last_stepped_at: r.u64()?,
                fixed_steps: r.u64()?,
            };
            if h.activated_at > h.last_stepped_at
                || h.last_stepped_at != clock
                || h.fixed_steps < h.last_stepped_at - h.activated_at
                || hot_cells.insert(id, h).is_some()
            {
                return Err(SimError::Snapshot("hot cell"));
            }
        }
        let mut scheduled = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for _ in 0..r.u32()? {
            let at = r.u64()?;
            let mut v = Vec::new();
            for _ in 0..r.u32()? {
                let id = r.u64()?;
                if id >= next_schedule_id || !ids.insert(id) {
                    return Err(SimError::Snapshot("schedule id"));
                }
                v.push(Pending {
                    id,
                    command: r.sc()?,
                })
            }
            if at < clock || v.is_empty() || scheduled.insert(at, v).is_some() {
                return Err(SimError::Snapshot("scheduled time"));
            }
        }
        if r.p != b.len() {
            return Err(SimError::Snapshot("trailing bytes"));
        }
        let w = Self {
            clock,
            seed,
            rng_counter,
            next_schedule_id,
            soldiers,
            squads,
            stockpiles,
            hot_cells,
            scheduled,
        };
        w.validate()?;
        Ok(w)
    }
    fn validate(&self) -> Result<(), SimError> {
        let mut membership = BTreeSet::new();
        for (id, q) in &self.squads {
            if *id != q.id {
                return Err(SimError::Snapshot("squad id"));
            }
            for x in &q.members {
                if !self.soldiers.valid(*x)
                    || !membership.insert(*x)
                    || self.soldiers.data[x.index()].squad != Some(*id)
                {
                    return Err(SimError::Snapshot("squad member"));
                }
            }
            if let Some(x) = q.officer {
                if !q.members.contains(&x) || self.soldiers.data[x.index()].role != Role::Officer {
                    return Err(SimError::Snapshot("officer"));
                }
            }
        }
        for i in 0..self.soldiers.alive.len() {
            if self.soldiers.alive[i] {
                if self.soldiers.needs_at[i] > self.clock {
                    return Err(SimError::Snapshot("future needs"));
                }
                if let Some(q) = self.soldiers.data[i].squad {
                    let id = EntityId::from_parts(i as u32, self.soldiers.generation[i]);
                    if !self.squads.get(&q).is_some_and(|q| q.members.contains(&id)) {
                        return Err(SimError::Snapshot("soldier squad"));
                    }
                }
            }
        }
        Ok(())
    }
}
fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325, |h, x| {
        (h ^ u64::from(*x)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Default)]
struct W(Vec<u8>);
impl W {
    fn u8(&mut self, x: u8) {
        self.0.push(x)
    }
    fn bool(&mut self, x: bool) {
        self.u8(x as u8)
    }
    fn u16(&mut self, x: u16) {
        self.0.extend(x.to_le_bytes())
    }
    fn u32(&mut self, x: u32) {
        self.0.extend(x.to_le_bytes())
    }
    fn i32(&mut self, x: i32) {
        self.0.extend(x.to_le_bytes())
    }
    fn u64(&mut self, x: u64) {
        self.0.extend(x.to_le_bytes())
    }
    fn opt_id(&mut self, x: Option<EntityId>) {
        self.bool(x.is_some());
        if let Some(x) = x {
            self.u64(x.raw())
        }
    }
    fn stock(&mut self, x: Stock) {
        self.u64(x.ammunition);
        self.u64(x.supplies)
    }
    fn spec(&mut self, s: SoldierSpec) {
        self.u16(s.faction);
        self.i32(s.position.x_mm);
        self.i32(s.position.y_mm);
        self.u32(s.position.cell);
        self.bool(s.squad.is_some());
        if let Some(x) = s.squad {
            self.u32(x)
        }
        self.u8(s.role as u8);
        self.u8(s.rank);
        self.u16(s.health);
        self.u32(s.ammunition);
        self.u32(s.inventory.food);
        self.u32(s.inventory.water);
        self.u32(s.inventory.medical)
    }
    fn sc(&mut self, c: ScheduledCommand) {
        match c {
            ScheduledCommand::Transfer {
                from,
                to,
                ammunition,
                supplies,
            } => {
                self.u8(0);
                self.u32(from);
                self.u32(to);
                self.u64(ammunition);
                self.u64(supplies)
            }
            ScheduledCommand::SetRegionHot { cell, hot } => {
                self.u8(1);
                self.u32(cell);
                self.bool(hot)
            }
            ScheduledCommand::CreateStockpile { id, initial } => {
                self.u8(2);
                self.u32(id);
                self.stock(initial)
            }
        }
    }
}
struct R<'a> {
    b: &'a [u8],
    p: usize,
}
impl R<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], SimError> {
        let e = self
            .p
            .checked_add(N)
            .ok_or(SimError::Snapshot("overflow"))?;
        let x = self
            .b
            .get(self.p..e)
            .ok_or(SimError::Snapshot("truncated"))?;
        self.p = e;
        Ok(x.try_into().expect("length"))
    }
    fn u8(&mut self) -> Result<u8, SimError> {
        Ok(self.take::<1>()?[0])
    }
    fn bool(&mut self) -> Result<bool, SimError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SimError::Snapshot("boolean")),
        }
    }
    fn u16(&mut self) -> Result<u16, SimError> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn u32(&mut self) -> Result<u32, SimError> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    fn i32(&mut self) -> Result<i32, SimError> {
        Ok(i32::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> Result<u64, SimError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn opt_id(&mut self) -> Result<Option<EntityId>, SimError> {
        if self.bool()? {
            Ok(Some(EntityId(self.u64()?)))
        } else {
            Ok(None)
        }
    }
    fn stock(&mut self) -> Result<Stock, SimError> {
        Ok(Stock {
            ammunition: self.u64()?,
            supplies: self.u64()?,
        })
    }
    fn spec(&mut self) -> Result<SoldierSpec, SimError> {
        let faction = self.u16()?;
        let position = Position {
            x_mm: self.i32()?,
            y_mm: self.i32()?,
            cell: self.u32()?,
        };
        let squad = if self.bool()? {
            Some(self.u32()?)
        } else {
            None
        };
        let role = match self.u8()? {
            0 => Role::Rifle,
            1 => Role::Medic,
            2 => Role::Officer,
            3 => Role::Logistics,
            _ => return Err(SimError::Snapshot("role")),
        };
        Ok(SoldierSpec {
            faction,
            position,
            squad,
            role,
            rank: self.u8()?,
            health: self.u16()?,
            ammunition: self.u32()?,
            inventory: Inventory {
                food: self.u32()?,
                water: self.u32()?,
                medical: self.u32()?,
            },
        })
    }
    fn sc(&mut self) -> Result<ScheduledCommand, SimError> {
        match self.u8()? {
            0 => Ok(ScheduledCommand::Transfer {
                from: self.u32()?,
                to: self.u32()?,
                ammunition: self.u64()?,
                supplies: self.u64()?,
            }),
            1 => Ok(ScheduledCommand::SetRegionHot {
                cell: self.u32()?,
                hot: self.bool()?,
            }),
            2 => Ok(ScheduledCommand::CreateStockpile {
                id: self.u32()?,
                initial: self.stock()?,
            }),
            _ => Err(SimError::Snapshot("command")),
        }
    }
}
