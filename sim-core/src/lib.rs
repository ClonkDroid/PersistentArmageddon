//! Deterministic authoritative simulation state.
//! Authoritative values are integers; presentation layers may interpolate copies.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SNAPSHOT_VERSION: u32 = 2;
const NEEDS_INTERVAL: u64 = 60;

/// A stable handle. Reused slots receive a new generation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);

impl EntityId {
    pub fn from_parts(index: u32, generation: u32) -> Self {
        Self((u64::from(generation) << 32) | u64::from(index))
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AdvanceTo(u64),
    World(WorldCommand),
}

/// Commands which may be placed on the scheduler. Time advancement is deliberately
/// absent: scheduled work cannot recursively move the clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldCommand {
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    TransferCompleted { from: u32, to: u32, stock: Stock },
    RegionFidelityChanged { cell: u32, hot: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimError {
    InvalidEntity,
    TimeReversal,
    UnknownStockpile,
    InsufficientStock,
    InvalidTransfer,
    ArithmeticOverflow,
    InvalidSquad,
    InvalidOfficerRole,
    Snapshot(&'static str),
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SimError {}

/// Structure-of-arrays storage. Each live slot is the sole authority for its ID.
#[derive(Clone, Default)]
struct Soldiers {
    generation: Vec<u32>,
    alive: Vec<bool>,
    faction: Vec<u16>,
    position: Vec<Position>,
    squad: Vec<Option<u32>>,
    role: Vec<Role>,
    rank: Vec<u8>,
    health: Vec<u16>,
    needs_base: Vec<Needs>,
    needs_at: Vec<u64>,
    ammunition: Vec<u32>,
    inventory: Vec<Inventory>,
    free: Vec<u32>,
    live: usize,
}

impl Soldiers {
    fn valid(&self, id: EntityId) -> bool {
        id.index() < self.alive.len()
            && self.alive[id.index()]
            && self.generation[id.index()] == id.generation()
    }
    fn spawn(&mut self, spec: SoldierSpec, now: u64) -> EntityId {
        let i = if let Some(i) = self.free.pop() {
            i as usize
        } else {
            let i = self.alive.len();
            self.generation.push(0);
            self.alive.push(false);
            self.faction.push(0);
            self.position.push(Position::default());
            self.squad.push(None);
            self.role.push(Role::Rifle);
            self.rank.push(0);
            self.health.push(0);
            self.needs_base.push(Needs::default());
            self.needs_at.push(now);
            self.ammunition.push(0);
            self.inventory.push(Inventory::default());
            i
        };
        self.alive[i] = true;
        self.faction[i] = spec.faction;
        self.position[i] = spec.position;
        self.squad[i] = spec.squad;
        self.role[i] = spec.role;
        self.rank[i] = spec.rank;
        self.health[i] = spec.health;
        self.needs_base[i] = Needs::default();
        self.needs_at[i] = now;
        self.ammunition[i] = spec.ammunition;
        self.inventory[i] = spec.inventory;
        self.live += 1;
        EntityId::from_parts(i as u32, self.generation[i])
    }
    fn despawn(&mut self, id: EntityId) -> bool {
        if !self.valid(id) {
            return false;
        }
        let i = id.index();
        self.alive[i] = false;
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.free.push(i as u32);
        self.live -= 1;
        true
    }
    fn needs(&self, i: usize, now: u64) -> Needs {
        let elapsed = now - self.needs_at[i];
        let mut n = self.needs_base[i];
        n.fatigue = n
            .fatigue
            .saturating_add(u32::try_from(elapsed).unwrap_or(u32::MAX));
        n.hunger = n
            .hunger
            .saturating_add(u32::try_from(elapsed / 3).unwrap_or(u32::MAX));
        n.thirst = n
            .thirst
            .saturating_add(u32::try_from(elapsed / 2).unwrap_or(u32::MAX));
        n.sleep_debt = n
            .sleep_debt
            .saturating_add(u32::try_from(elapsed / 4).unwrap_or(u32::MAX));
        n
    }
}

/// Complete authoritative world. `advance_to` touches only due events and hot cells.
#[derive(Clone)]
pub struct World {
    clock: u64,
    seed: u64,
    rng_counter: u64,
    soldiers: Soldiers,
    squads: BTreeMap<u32, Squad>,
    stockpiles: BTreeMap<u32, Stock>,
    hot_cells: BTreeSet<u32>,
    scheduled: BTreeMap<u64, Vec<WorldCommand>>,
    next_needs_wakeup: u64,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            clock: 0,
            seed,
            rng_counter: 0,
            soldiers: Soldiers::default(),
            squads: BTreeMap::new(),
            stockpiles: BTreeMap::new(),
            hot_cells: BTreeSet::new(),
            scheduled: BTreeMap::new(),
            next_needs_wakeup: NEEDS_INTERVAL,
        }
    }
    pub fn clock(&self) -> u64 {
        self.clock
    }
    pub fn soldier_count(&self) -> usize {
        self.soldiers.live
    }
    pub fn spawn(&mut self, spec: SoldierSpec) -> Result<EntityId, SimError> {
        if let Some(s) = spec.squad {
            if !self.squads.contains_key(&s) {
                return Err(SimError::InvalidSquad);
            }
        }
        let id = self.soldiers.spawn(spec, self.clock);
        if let Some(s) = spec.squad {
            self.squads.get_mut(&s).expect("checked").members.insert(id);
        }
        Ok(id)
    }
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.soldiers.despawn(id) {
            return false;
        }
        for squad in self.squads.values_mut() {
            squad.members.remove(&id);
            if squad.officer == Some(id) {
                squad.officer = None;
            }
        }
        true
    }
    pub fn soldier(&self, id: EntityId) -> Option<Soldier> {
        if !self.soldiers.valid(id) {
            return None;
        }
        let i = id.index();
        Some(Soldier {
            id,
            faction: self.soldiers.faction[i],
            position: self.soldiers.position[i],
            squad: self.soldiers.squad[i],
            role: self.soldiers.role[i],
            rank: self.soldiers.rank[i],
            health: self.soldiers.health[i],
            needs: self.soldiers.needs(i, self.clock),
            ammunition: self.soldiers.ammunition[i],
            inventory: self.soldiers.inventory[i],
        })
    }
    pub fn create_squad(&mut self, id: u32) {
        self.squads.entry(id).or_insert(Squad {
            id,
            officer: None,
            members: BTreeSet::new(),
        });
    }
    pub fn assign_officer(&mut self, squad: u32, officer: EntityId) -> Result<(), SimError> {
        if !self.squads.contains_key(&squad) {
            return Err(SimError::InvalidSquad);
        }
        if !self.soldiers.valid(officer) {
            return Err(SimError::InvalidEntity);
        }
        if self.soldiers.role[officer.index()] != Role::Officer {
            return Err(SimError::InvalidOfficerRole);
        }
        // Membership is one-to-one. Reassignment removes every stale relationship.
        for s in self.squads.values_mut() {
            s.members.remove(&officer);
            if s.officer == Some(officer) {
                s.officer = None;
            }
        }
        let s = self.squads.get_mut(&squad).expect("checked");
        s.members.insert(officer);
        s.officer = Some(officer);
        self.soldiers.squad[officer.index()] = Some(squad);
        Ok(())
    }
    pub fn squad(&self, id: u32) -> Option<&Squad> {
        self.squads.get(&id)
    }
    pub fn set_stockpile(&mut self, id: u32, stock: Stock) {
        self.stockpiles.insert(id, stock);
    }
    pub fn stockpile(&self, id: u32) -> Option<Stock> {
        self.stockpiles.get(&id).copied()
    }
    pub fn schedule(&mut self, at: u64, command: WorldCommand) -> Result<(), SimError> {
        if at < self.clock {
            return Err(SimError::TimeReversal);
        }
        self.scheduled.entry(at).or_default().push(command);
        Ok(())
    }
    pub fn apply(&mut self, command: Command) -> Result<Vec<Event>, SimError> {
        match command {
            Command::AdvanceTo(t) => {
                self.advance_to(t)?;
                Ok(Vec::new())
            }
            Command::World(command) => self.apply_world(command),
        }
    }
    pub fn apply_world(&mut self, command: WorldCommand) -> Result<Vec<Event>, SimError> {
        match command {
            WorldCommand::SetRegionHot { cell, hot } => {
                if hot {
                    self.hot_cells.insert(cell);
                } else {
                    self.hot_cells.remove(&cell);
                }
                Ok(vec![Event::RegionFidelityChanged { cell, hot }])
            }
            WorldCommand::Transfer {
                from,
                to,
                ammunition,
                supplies,
            } => {
                if from == to {
                    return Err(SimError::InvalidTransfer);
                }
                let amount = Stock {
                    ammunition,
                    supplies,
                };
                let src = self
                    .stockpiles
                    .get(&from)
                    .copied()
                    .ok_or(SimError::UnknownStockpile)?;
                if !self.stockpiles.contains_key(&to) {
                    return Err(SimError::UnknownStockpile);
                }
                if src.ammunition < ammunition || src.supplies < supplies {
                    return Err(SimError::InsufficientStock);
                }
                let dst = self.stockpiles[&to];
                let dst_ammunition = dst
                    .ammunition
                    .checked_add(ammunition)
                    .ok_or(SimError::ArithmeticOverflow)?;
                let dst_supplies = dst
                    .supplies
                    .checked_add(supplies)
                    .ok_or(SimError::ArithmeticOverflow)?;
                self.stockpiles.insert(
                    from,
                    Stock {
                        ammunition: src.ammunition - ammunition,
                        supplies: src.supplies - supplies,
                    },
                );
                self.stockpiles.insert(
                    to,
                    Stock {
                        ammunition: dst_ammunition,
                        supplies: dst_supplies,
                    },
                );
                Ok(vec![Event::TransferCompleted {
                    from,
                    to,
                    stock: amount,
                }])
            }
        }
    }
    pub fn advance_to(&mut self, target: u64) -> Result<(), SimError> {
        if target < self.clock {
            return Err(SimError::TimeReversal);
        }
        let times: Vec<u64> = self.scheduled.range(..=target).map(|(t, _)| *t).collect();
        for t in times {
            self.clock = t;
            if let Some(mut commands) = self.scheduled.remove(&t) {
                while !commands.is_empty() {
                    let c = commands[0];
                    if let Err(error) = self.apply_world(c) {
                        // The failing command and every unattempted command remain pending.
                        self.scheduled.insert(t, commands);
                        return Err(error);
                    }
                    commands.remove(0);
                }
            }
        }
        self.clock = target;
        if self.next_needs_wakeup <= target {
            let intervals = (target - self.next_needs_wakeup) / NEEDS_INTERVAL;
            self.next_needs_wakeup = self
                .next_needs_wakeup
                .saturating_add(intervals.saturating_add(1).saturating_mul(NEEDS_INTERVAL));
        }
        Ok(())
    }
    pub fn hot_cell_count(&self) -> usize {
        self.hot_cells.len()
    }
    pub fn next_random_u64(&mut self) -> u64 {
        let mut z = self
            .seed
            .wrapping_add(self.rng_counter.wrapping_mul(0x9e3779b97f4a7c15));
        self.rng_counter += 1;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn state_digest(&self) -> u64 {
        let bytes = self.snapshot();
        fnv1a(&bytes)
    }
    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(SNAPSHOT_VERSION);
        w.u64(self.clock);
        w.u64(self.seed);
        w.u64(self.rng_counter);
        w.u64(self.next_needs_wakeup);
        w.u32(self.soldiers.alive.len() as u32);
        for i in 0..self.soldiers.alive.len() {
            w.u32(self.soldiers.generation[i]);
            w.u8(self.soldiers.alive[i] as u8);
            if self.soldiers.alive[i] {
                w.u16(self.soldiers.faction[i]);
                let p = self.soldiers.position[i];
                w.i32(p.x_mm);
                w.i32(p.y_mm);
                w.u32(p.cell);
                w.opt_u32(self.soldiers.squad[i]);
                w.u8(self.soldiers.role[i] as u8);
                w.u8(self.soldiers.rank[i]);
                w.u16(self.soldiers.health[i]);
                let n = self.soldiers.needs_base[i];
                w.u32(n.fatigue);
                w.u32(n.hunger);
                w.u32(n.thirst);
                w.u32(n.sleep_debt);
                w.u64(self.soldiers.needs_at[i]);
                w.u32(self.soldiers.ammunition[i]);
                let v = self.soldiers.inventory[i];
                w.u32(v.food);
                w.u32(v.water);
                w.u32(v.medical);
            }
        }
        w.u32(self.soldiers.free.len() as u32);
        for index in &self.soldiers.free {
            w.u32(*index);
        }
        w.u32(self.squads.len() as u32);
        for s in self.squads.values() {
            w.u32(s.id);
            w.opt_id(s.officer);
            w.u32(s.members.len() as u32);
            for id in &s.members {
                w.u64(id.raw());
            }
        }
        w.u32(self.stockpiles.len() as u32);
        for (id, s) in &self.stockpiles {
            w.u32(*id);
            w.u64(s.ammunition);
            w.u64(s.supplies);
        }
        w.u32(self.hot_cells.len() as u32);
        for c in &self.hot_cells {
            w.u32(*c);
        }
        w.u32(self.scheduled.len() as u32);
        for (at, commands) in &self.scheduled {
            w.u64(*at);
            w.u32(commands.len() as u32);
            for command in commands {
                w.world_command(*command);
            }
        }
        w.0
    }
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, SimError> {
        let mut r = Reader { b: bytes, p: 0 };
        if r.u32()? != SNAPSHOT_VERSION {
            return Err(SimError::Snapshot("unsupported version"));
        }
        let clock = r.u64()?;
        let seed = r.u64()?;
        let rng_counter = r.u64()?;
        let next_needs_wakeup = r.u64()?;
        let slots = r.u32()? as usize;
        let mut soldiers = Soldiers::default();
        for i in 0..slots {
            soldiers.generation.push(r.u32()?);
            let alive = r.u8()? != 0;
            soldiers.alive.push(alive);
            soldiers.faction.push(0);
            soldiers.position.push(Position::default());
            soldiers.squad.push(None);
            soldiers.role.push(Role::Rifle);
            soldiers.rank.push(0);
            soldiers.health.push(0);
            soldiers.needs_base.push(Needs::default());
            soldiers.needs_at.push(clock);
            soldiers.ammunition.push(0);
            soldiers.inventory.push(Inventory::default());
            if alive {
                soldiers.live += 1;
                soldiers.faction[i] = r.u16()?;
                soldiers.position[i] = Position {
                    x_mm: r.i32()?,
                    y_mm: r.i32()?,
                    cell: r.u32()?,
                };
                soldiers.squad[i] = r.opt_u32()?;
                soldiers.role[i] = match r.u8()? {
                    0 => Role::Rifle,
                    1 => Role::Medic,
                    2 => Role::Officer,
                    3 => Role::Logistics,
                    _ => return Err(SimError::Snapshot("role")),
                };
                soldiers.rank[i] = r.u8()?;
                soldiers.health[i] = r.u16()?;
                soldiers.needs_base[i] = Needs {
                    fatigue: r.u32()?,
                    hunger: r.u32()?,
                    thirst: r.u32()?,
                    sleep_debt: r.u32()?,
                };
                soldiers.needs_at[i] = r.u64()?;
                soldiers.ammunition[i] = r.u32()?;
                soldiers.inventory[i] = Inventory {
                    food: r.u32()?,
                    water: r.u32()?,
                    medical: r.u32()?,
                };
            }
        }
        let free_len = r.u32()? as usize;
        let mut seen_free = BTreeSet::new();
        for _ in 0..free_len {
            let index = r.u32()?;
            if index as usize >= slots || soldiers.alive[index as usize] || !seen_free.insert(index)
            {
                return Err(SimError::Snapshot("invalid free slot"));
            }
            soldiers.free.push(index);
        }
        if free_len != slots - soldiers.live {
            return Err(SimError::Snapshot("incomplete free list"));
        }
        let mut squads = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            let officer = r.opt_id()?;
            let mut members = BTreeSet::new();
            for _ in 0..r.u32()? {
                members.insert(EntityId(r.u64()?));
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
            if stockpiles
                .insert(
                    r.u32()?,
                    Stock {
                        ammunition: r.u64()?,
                        supplies: r.u64()?,
                    },
                )
                .is_some()
            {
                return Err(SimError::Snapshot("duplicate stockpile"));
            }
        }
        let mut hot_cells = BTreeSet::new();
        for _ in 0..r.u32()? {
            if !hot_cells.insert(r.u32()?) {
                return Err(SimError::Snapshot("duplicate hot cell"));
            }
        }
        let mut scheduled = BTreeMap::new();
        for _ in 0..r.u32()? {
            let at = r.u64()?;
            let mut commands = Vec::new();
            for _ in 0..r.u32()? {
                commands.push(r.world_command()?);
            }
            if at < clock || scheduled.insert(at, commands).is_some() {
                return Err(SimError::Snapshot("invalid scheduled time"));
            }
        }
        if r.p != bytes.len() {
            return Err(SimError::Snapshot("trailing bytes"));
        }
        let world = Self {
            clock,
            seed,
            rng_counter,
            soldiers,
            squads,
            stockpiles,
            hot_cells,
            scheduled,
            next_needs_wakeup,
        };
        world.validate()?;
        Ok(world)
    }

    fn validate(&self) -> Result<(), SimError> {
        let n = self.soldiers.alive.len();
        let lengths = [
            self.soldiers.generation.len(),
            self.soldiers.faction.len(),
            self.soldiers.position.len(),
            self.soldiers.squad.len(),
            self.soldiers.role.len(),
            self.soldiers.rank.len(),
            self.soldiers.health.len(),
            self.soldiers.needs_base.len(),
            self.soldiers.needs_at.len(),
            self.soldiers.ammunition.len(),
            self.soldiers.inventory.len(),
        ];
        if lengths.iter().any(|length| *length != n)
            || self.soldiers.live != self.soldiers.alive.iter().filter(|v| **v).count()
        {
            return Err(SimError::Snapshot("soldier vectors"));
        }
        let mut memberships = BTreeSet::new();
        for (squad_id, squad) in &self.squads {
            if squad.id != *squad_id {
                return Err(SimError::Snapshot("squad id"));
            }
            for member in &squad.members {
                if !self.soldiers.valid(*member)
                    || !memberships.insert(*member)
                    || self.soldiers.squad[member.index()] != Some(*squad_id)
                {
                    return Err(SimError::Snapshot("squad member"));
                }
            }
            if let Some(officer) = squad.officer {
                if !squad.members.contains(&officer)
                    || self.soldiers.role[officer.index()] != Role::Officer
                {
                    return Err(SimError::Snapshot("officer"));
                }
            }
        }
        for i in 0..n {
            if self.soldiers.alive[i] {
                if let Some(squad) = self.soldiers.squad[i] {
                    let id = EntityId::from_parts(i as u32, self.soldiers.generation[i]);
                    if !self
                        .squads
                        .get(&squad)
                        .is_some_and(|s| s.members.contains(&id))
                    {
                        return Err(SimError::Snapshot("soldier squad"));
                    }
                }
                if self.soldiers.needs_at[i] > self.clock {
                    return Err(SimError::Snapshot("future needs"));
                }
            }
        }
        Ok(())
    }

    /// Materializes every live soldier's derived needs and returns a deterministic checksum.
    pub fn needs_checksum(&self) -> u64 {
        let mut checksum = 0u64;
        for i in 0..self.soldiers.alive.len() {
            if self.soldiers.alive[i] {
                let n = self.soldiers.needs(i, self.clock);
                checksum = checksum.wrapping_mul(0x100000001b3).wrapping_add(
                    u64::from(n.fatigue)
                        ^ (u64::from(n.hunger) << 32)
                        ^ u64::from(n.thirst)
                        ^ (u64::from(n.sleep_debt) << 32),
                );
            }
        }
        checksum
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v)
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes())
    }
    fn i32(&mut self, v: i32) {
        self.0.extend(v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) {
        self.0.extend(v.to_le_bytes())
    }
    fn opt_u32(&mut self, v: Option<u32>) {
        self.u32(v.unwrap_or(u32::MAX))
    }
    fn opt_id(&mut self, v: Option<EntityId>) {
        self.u64(v.map_or(u64::MAX, EntityId::raw))
    }
    fn world_command(&mut self, command: WorldCommand) {
        match command {
            WorldCommand::Transfer {
                from,
                to,
                ammunition,
                supplies,
            } => {
                self.u8(0);
                self.u32(from);
                self.u32(to);
                self.u64(ammunition);
                self.u64(supplies);
            }
            WorldCommand::SetRegionHot { cell, hot } => {
                self.u8(1);
                self.u32(cell);
                self.u8(hot as u8);
            }
        }
    }
}
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], SimError> {
        let end = self
            .p
            .checked_add(N)
            .ok_or(SimError::Snapshot("overflow"))?;
        let s = self
            .b
            .get(self.p..end)
            .ok_or(SimError::Snapshot("truncated"))?;
        self.p = end;
        Ok(s.try_into().expect("length"))
    }
    fn u8(&mut self) -> Result<u8, SimError> {
        Ok(self.take::<1>()?[0])
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
    fn opt_u32(&mut self) -> Result<Option<u32>, SimError> {
        let v = self.u32()?;
        Ok((v != u32::MAX).then_some(v))
    }
    fn opt_id(&mut self) -> Result<Option<EntityId>, SimError> {
        let v = self.u64()?;
        Ok((v != u64::MAX).then_some(EntityId(v)))
    }
    fn world_command(&mut self) -> Result<WorldCommand, SimError> {
        match self.u8()? {
            0 => Ok(WorldCommand::Transfer {
                from: self.u32()?,
                to: self.u32()?,
                ammunition: self.u64()?,
                supplies: self.u64()?,
            }),
            1 => Ok(WorldCommand::SetRegionHot {
                cell: self.u32()?,
                hot: self.u8()? != 0,
            }),
            _ => Err(SimError::Snapshot("command")),
        }
    }
}

#[cfg(test)]
mod snapshot_validation_tests {
    use super::*;

    #[test]
    fn validation_rejects_broken_relationships_and_allocator_state() {
        let mut world = World::new(1);
        world.create_squad(7);
        let id = world
            .spawn(SoldierSpec {
                squad: Some(7),
                ..SoldierSpec::default()
            })
            .unwrap();
        world.squads.get_mut(&7).unwrap().members.clear();
        assert_eq!(world.validate(), Err(SimError::Snapshot("soldier squad")));
        world.squads.get_mut(&7).unwrap().members.insert(id);
        world.soldiers.free.push(id.index() as u32);
        assert_eq!(world.validate(), Ok(())); // Encoding carries the corruption to strict restore validation.
        assert!(matches!(
            World::from_snapshot(&world.snapshot()),
            Err(SimError::Snapshot("invalid free slot"))
        ));
    }

    #[test]
    fn validation_rejects_invalid_officer_and_duplicate_membership() {
        let mut world = World::new(1);
        world.create_squad(1);
        world.create_squad(2);
        let id = world
            .spawn(SoldierSpec {
                squad: Some(1),
                ..SoldierSpec::default()
            })
            .unwrap();
        world.squads.get_mut(&1).unwrap().officer = Some(id);
        assert_eq!(world.validate(), Err(SimError::Snapshot("officer")));
        world.squads.get_mut(&1).unwrap().officer = None;
        world.squads.get_mut(&2).unwrap().members.insert(id);
        assert_eq!(world.validate(), Err(SimError::Snapshot("squad member")));
    }
}
