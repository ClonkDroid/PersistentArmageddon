//! Deterministic authoritative M0 simulation kernel.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

pub const SNAPSHOT_VERSION: u32 = 6;
pub const NEED_MAX: u32 = 1_000;
pub const RATION_THRESHOLD: u32 = 100;
pub const FOOD_RATION: u32 = 1;
pub const WATER_RATION: u32 = 1;
pub const SEVERE_HUNGER: u32 = 800;
pub const SEVERE_THIRST: u32 = 800;
pub const FORCED_IDLE_FATIGUE: u32 = 900;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Activity {
    Rest,
    #[default]
    Idle,
    March,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeathCause {
    Dehydration,
    Starvation,
    Exhaustion,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LifeState {
    #[default]
    Alive,
    Dead {
        at: u64,
        cause: DeathCause,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LivingState {
    pub hunger: u32,
    pub thirst: u32,
    pub fatigue: u32,
    pub sleep_debt: u32,
    pub morale: u16,
    pub health: u16,
    pub activity: Activity,
    pub life: LifeState,
    pub materialized_at: u64,
}
impl Default for LivingState {
    fn default() -> Self {
        Self {
            hunger: 0,
            thirst: 0,
            fatigue: 0,
            sleep_debt: 0,
            morale: 1000,
            health: 1000,
            activity: Activity::Idle,
            life: LifeState::Alive,
            materialized_at: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct EntityId(u64);
impl EntityId {
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
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
    pub living: LivingState,
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
    SetActivity {
        id: EntityId,
        activity: Activity,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    SoldierSpawned {
        id: EntityId,
        loadout: Loadout,
    },
    SoldierRemoved {
        id: EntityId,
        loadout: Loadout,
    },
    SquadCreated {
        id: u32,
    },
    OfficerAssigned {
        squad: u32,
        officer: EntityId,
    },
    StockpileCreated {
        id: u32,
        initial: Stock,
    },
    TransferCompleted {
        from: u32,
        to: u32,
        stock: Stock,
    },
    RegionFidelityChanged {
        cell: u32,
        hot: bool,
        fixed_steps: u64,
    },
    TimeAdvanced {
        from: u64,
        to: u64,
        hot_cells_stepped: u64,
        fixed_steps_per_hot_cell: u64,
    },
    Scheduled {
        id: u64,
        at: u64,
    },
    ScheduleCancelled {
        id: u64,
        at: u64,
    },
    RandomGenerated {
        value: u64,
    },
    ActivityChanged {
        id: EntityId,
        before: Activity,
        after: Activity,
        forced: bool,
    },
    RationConsumed {
        id: EntityId,
        food: u32,
        water: u32,
        hunger_before: u32,
        hunger_after: u32,
        thirst_before: u32,
        thirst_after: u32,
    },
    LivingDeteriorated {
        id: EntityId,
        morale_before: u16,
        morale_after: u16,
        health_before: u16,
        health_after: u16,
    },
    SoldierDied {
        id: EntityId,
        cause: DeathCause,
        health_before: u16,
    },
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
    pub blocked: Option<BlockedCommand>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockedCommand {
    pub id: u64,
    pub at: u64,
    pub command: ScheduledCommand,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceTotals {
    pub ammunition: u128,
    pub stockpile_supplies: u128,
    pub carried_food: u128,
    pub carried_water: u128,
    pub carried_medical: u128,
    pub sourced_food: u128,
    pub sourced_water: u128,
    pub consumed_food: u128,
    pub consumed_water: u128,
    pub lost_food: u128,
    pub lost_water: u128,
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
    DeadEntity,
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
    living: Vec<LivingState>,
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
            self.living.push(LivingState::default());
            self.alive.len() - 1
        };
        self.alive[i] = true;
        self.data[i] = s;
        self.living[i] = LivingState {
            health: s.health,
            materialized_at: now,
            ..LivingState::default()
        };
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
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HotCellState {
    pub activated_at: u64,
    pub last_stepped_at: u64,
    pub fixed_steps: u64,
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
    scheduled: BTreeMap<u64, BTreeMap<u64, ScheduledCommand>>,
    // Derived authoritative index: expected O(1) ID lookup; bucket removal is O(log N).
    schedule_index: HashMap<u64, u64>,
    reserved_stockpiles: BTreeSet<u32>,
    cell_members: BTreeMap<u32, BTreeSet<EntityId>>,
    living_due: BTreeMap<u64, BTreeSet<EntityId>>,
    due_by_entity: HashMap<EntityId, u64>,
    sourced_food: u128,
    sourced_water: u128,
    consumed_food: u128,
    consumed_water: u128,
    lost_food: u128,
    lost_water: u128,
    cold_boundaries: u64,
    hot_member_steps: u64,
}
impl World {
    fn rates(a: Activity) -> (i32, u32, u32, i32) {
        match a {
            Activity::Rest => (-2, 1, 1, -2),
            Activity::Idle => (1, 1, 2, 1),
            Activity::March => (3, 2, 3, 2),
        }
    }
    fn unschedule_due(&mut self, id: EntityId) {
        if let Some(at) = self.due_by_entity.remove(&id) {
            if let Some(s) = self.living_due.get_mut(&at) {
                s.remove(&id);
                if s.is_empty() {
                    self.living_due.remove(&at);
                }
            }
        }
    }
    fn schedule_due(&mut self, id: EntityId) -> Result<(), SimError> {
        self.unschedule_due(id);
        if !self.soldiers.valid(id)
            || self
                .hot_cells
                .contains_key(&self.soldiers.data[id.index()].position.cell)
            || self.soldiers.living[id.index()].life != LifeState::Alive
        {
            return Ok(());
        }
        let l = self.soldiers.living[id.index()];
        let inv = self.soldiers.data[id.index()].inventory;
        let (_, hr, tr, _) = Self::rates(l.activity);
        let ceil = |d: u32, r: u32| u64::from(d.saturating_add(r - 1) / r).max(1);
        let mut d = u64::MAX;
        if inv.food > 0 {
            d = d.min(ceil(RATION_THRESHOLD.saturating_sub(l.hunger), hr));
        } else if l.hunger < SEVERE_HUNGER {
            d = d.min(ceil(SEVERE_HUNGER - l.hunger, hr));
        } else {
            d = 1;
        }
        if inv.water > 0 {
            d = d.min(ceil(RATION_THRESHOLD.saturating_sub(l.thirst), tr));
        } else if l.thirst < SEVERE_THIRST {
            d = d.min(ceil(SEVERE_THIRST - l.thirst, tr));
        } else {
            d = 1;
        }
        if l.activity == Activity::March && l.fatigue < FORCED_IDLE_FATIGUE {
            d = d.min(ceil(FORCED_IDLE_FATIGUE - l.fatigue, 3));
        }
        let at = l
            .materialized_at
            .checked_add(d)
            .ok_or(SimError::ArithmeticOverflow)?;
        self.living_due.entry(at).or_default().insert(id);
        self.due_by_entity.insert(id, at);
        Ok(())
    }
    fn analytical(&mut self, id: EntityId, to: u64) -> Result<(), SimError> {
        let i = id.index();
        let l = &mut self.soldiers.living[i];
        let n = to
            .checked_sub(l.materialized_at)
            .ok_or(SimError::TimeReversal)?;
        if n == 0 || l.life != LifeState::Alive {
            return Ok(());
        }
        let (fr, hr, tr, sr) = Self::rates(l.activity);
        let adj = |v: u32, r: i32| -> u32 {
            if r >= 0 {
                v.saturating_add(
                    u32::try_from(n)
                        .unwrap_or(u32::MAX)
                        .saturating_mul(r as u32),
                )
                .min(NEED_MAX)
            } else {
                v.saturating_sub(
                    u32::try_from(n)
                        .unwrap_or(u32::MAX)
                        .saturating_mul((-r) as u32),
                )
            }
        };
        l.fatigue = adj(l.fatigue, fr);
        l.sleep_debt = adj(l.sleep_debt, sr);
        l.hunger = l
            .hunger
            .saturating_add(u32::try_from(n).unwrap_or(u32::MAX).saturating_mul(hr))
            .min(NEED_MAX);
        l.thirst = l
            .thirst
            .saturating_add(u32::try_from(n).unwrap_or(u32::MAX).saturating_mul(tr))
            .min(NEED_MAX);
        l.materialized_at = to;
        Ok(())
    }
    fn reference_second(
        &mut self,
        id: EntityId,
        at: u64,
        out: &mut Vec<TimedEvent>,
    ) -> Result<(), SimError> {
        self.analytical(id, at)?;
        let i = id.index();
        if self.soldiers.living[i].life != LifeState::Alive {
            return Ok(());
        }
        let mut l = self.soldiers.living[i];
        let inv = &mut self.soldiers.data[i].inventory;
        let hb = l.hunger;
        let tb = l.thirst;
        let mut food = 0;
        let mut water = 0;
        if l.hunger >= RATION_THRESHOLD && inv.food > 0 {
            inv.food -= FOOD_RATION;
            food = FOOD_RATION;
            l.hunger -= RATION_THRESHOLD;
            self.consumed_food += 1;
        }
        if l.thirst >= RATION_THRESHOLD && inv.water > 0 {
            inv.water -= WATER_RATION;
            water = WATER_RATION;
            l.thirst -= RATION_THRESHOLD;
            self.consumed_water += 1;
        }
        if food != 0 || water != 0 {
            out.push(TimedEvent {
                at,
                event: Event::RationConsumed {
                    id,
                    food,
                    water,
                    hunger_before: hb,
                    hunger_after: l.hunger,
                    thirst_before: tb,
                    thirst_after: l.thirst,
                },
            });
        }
        if l.activity == Activity::March && l.fatigue >= FORCED_IDLE_FATIGUE {
            let before = l.activity;
            l.activity = Activity::Idle;
            out.push(TimedEvent {
                at,
                event: Event::ActivityChanged {
                    id,
                    before,
                    after: l.activity,
                    forced: true,
                },
            });
        }
        if (inv.food == 0 && l.hunger >= SEVERE_HUNGER)
            || (inv.water == 0 && l.thirst >= SEVERE_THIRST)
        {
            let mb = l.morale;
            let hb = l.health;
            l.morale = l.morale.saturating_sub(1);
            let damage = if inv.water == 0 && l.thirst >= SEVERE_THIRST {
                10
            } else {
                4
            };
            l.health = l.health.saturating_sub(damage);
            out.push(TimedEvent {
                at,
                event: Event::LivingDeteriorated {
                    id,
                    morale_before: mb,
                    morale_after: l.morale,
                    health_before: hb,
                    health_after: l.health,
                },
            });
            if l.health == 0 {
                let cause = if inv.water == 0 && l.thirst >= SEVERE_THIRST {
                    DeathCause::Dehydration
                } else {
                    DeathCause::Starvation
                };
                l.life = LifeState::Dead { at, cause };
                out.push(TimedEvent {
                    at,
                    event: Event::SoldierDied {
                        id,
                        cause,
                        health_before: hb,
                    },
                });
            }
        }
        self.soldiers.living[i] = l;
        Ok(())
    }
    fn advance_cold_to(&mut self, t: u64, out: &mut Vec<TimedEvent>) -> Result<(), SimError> {
        while let Some(at) = self.living_due.keys().next().copied().filter(|x| *x <= t) {
            let ids = self.living_due.remove(&at).unwrap();
            for id in ids {
                self.due_by_entity.remove(&id);
                if self.soldiers.valid(id)
                    && !self
                        .hot_cells
                        .contains_key(&self.soldiers.data[id.index()].position.cell)
                {
                    self.reference_second(id, at, out)?;
                    self.cold_boundaries = self
                        .cold_boundaries
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    self.schedule_due(id)?;
                }
            }
        }
        let ids: Vec<_> = self.due_by_entity.keys().copied().collect();
        for id in ids {
            if self.soldiers.valid(id) {
                self.analytical(id, t)?;
            }
        }
        Ok(())
    }
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
            schedule_index: HashMap::new(),
            reserved_stockpiles: BTreeSet::new(),
            cell_members: BTreeMap::new(),
            living_due: BTreeMap::new(),
            due_by_entity: HashMap::new(),
            sourced_food: 0,
            sourced_water: 0,
            consumed_food: 0,
            consumed_water: 0,
            lost_food: 0,
            lost_water: 0,
            cold_boundaries: 0,
            hot_member_steps: 0,
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
            needs: Needs {
                fatigue: self.soldiers.living[i].fatigue,
                hunger: self.soldiers.living[i].hunger,
                thirst: self.soldiers.living[i].thirst,
                sleep_debt: self.soldiers.living[i].sleep_debt,
            },
            ammunition: s.ammunition,
            inventory: s.inventory,
            living: self.soldiers.living[i],
        })
    }
    pub fn apply(&mut self, c: Command) -> ApplyOutcome {
        let mut events = Vec::new();
        let mut blocked = None;
        let error = match c {
            Command::AdvanceTo { target } => self.advance(target, &mut events, &mut blocked),
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
            blocked,
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
                self.sourced_food += u128::from(spec.inventory.food);
                self.sourced_water += u128::from(spec.inventory.water);
                self.cell_members
                    .entry(spec.position.cell)
                    .or_default()
                    .insert(id);
                if let Some(s) = spec.squad {
                    self.squads.get_mut(&s).expect("checked").members.insert(id);
                }
                self.schedule_due(id)?;
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
                self.unschedule_due(id);
                if let Some(members) = self.cell_members.get_mut(&spec.position.cell) {
                    members.remove(&id);
                }
                self.lost_food += u128::from(spec.inventory.food);
                self.lost_water += u128::from(spec.inventory.water);
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
                if self.reserved_stockpiles.contains(&id) {
                    return Err(SimError::StockpileAlreadyExists);
                }
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
                self.scheduled.entry(at).or_default().insert(id, command);
                self.schedule_index.insert(id, at);
                if let ScheduledCommand::CreateStockpile { id, .. } = command {
                    self.reserved_stockpiles.insert(id);
                }
                Ok(Event::Scheduled { id, at })
            }
            Command::CancelScheduled { id } => {
                let at = self
                    .schedule_index
                    .remove(&id)
                    .ok_or(SimError::UnknownScheduledCommand)?;
                let queue = self.scheduled.get_mut(&at).expect("found");
                let command = queue.remove(&id).expect("indexed");
                if let ScheduledCommand::CreateStockpile { id, .. } = command {
                    self.reserved_stockpiles.remove(&id);
                }
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
            Command::SetActivity { id, activity } => {
                if !self.soldiers.valid(id) {
                    return Err(SimError::InvalidEntity);
                }
                if self.soldiers.living[id.index()].life != LifeState::Alive {
                    return Err(SimError::DeadEntity);
                }
                let before = self.soldiers.living[id.index()].activity;
                self.soldiers.living[id.index()].activity = activity;
                self.schedule_due(id)?;
                Ok(Event::ActivityChanged {
                    id,
                    before,
                    after: activity,
                    forced: false,
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
            ScheduledCommand::CreateStockpile { id, .. }
                if self.stockpiles.contains_key(&id) || self.reserved_stockpiles.contains(&id) =>
            {
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
                let members: Vec<_> = self
                    .cell_members
                    .get(&cell)
                    .into_iter()
                    .flat_map(|x| x.iter())
                    .copied()
                    .collect();
                for id in &members {
                    self.analytical(*id, self.clock)?;
                    self.unschedule_due(*id);
                }
                let fixed_steps = if hot {
                    self.hot_cells
                        .entry(cell)
                        .or_insert(HotCellState {
                            activated_at: self.clock,
                            last_stepped_at: self.clock,
                            fixed_steps: 0,
                        })
                        .fixed_steps
                } else {
                    self.hot_cells.remove(&cell).map_or(0, |h| h.fixed_steps)
                };
                if !hot {
                    for id in members {
                        self.schedule_due(id)?;
                    }
                }
                Ok(Event::RegionFidelityChanged {
                    cell,
                    hot,
                    fixed_steps,
                })
            }
        }
    }
    fn step_hot_to(&mut self, t: u64, out: &mut Vec<TimedEvent>) -> Result<(u64, u64), SimError> {
        for h in self.hot_cells.values() {
            h.fixed_steps
                .checked_add(t - h.last_stepped_at)
                .ok_or(SimError::ArithmeticOverflow)?;
        }
        let cells: Vec<_> = self.hot_cells.keys().copied().collect();
        for cell in cells {
            let start = self.hot_cells[&cell].last_stepped_at;
            let members: Vec<_> = self
                .cell_members
                .get(&cell)
                .into_iter()
                .flat_map(|x| x.iter())
                .copied()
                .collect();
            if !members.is_empty() {
                for at in start + 1..=t {
                    for id in &members {
                        if self.soldiers.valid(*id) {
                            self.reference_second(*id, at, out)?;
                            self.hot_member_steps = self
                                .hot_member_steps
                                .checked_add(1)
                                .ok_or(SimError::ArithmeticOverflow)?;
                        }
                    }
                }
            }
            let h = self.hot_cells.get_mut(&cell).unwrap();
            let d = t - start;
            h.fixed_steps += d;
            h.last_stepped_at = t;
        }
        let cells = self.hot_cells.len() as u64;
        Ok(if cells == 0 {
            (0, 0)
        } else {
            (cells, t - self.clock)
        })
    }
    fn advance(
        &mut self,
        target: u64,
        out: &mut Vec<TimedEvent>,
        blocked: &mut Option<BlockedCommand>,
    ) -> Result<(), SimError> {
        if target < self.clock {
            return Err(SimError::TimeReversal);
        }
        let times: Vec<_> = self.scheduled.range(..=target).map(|(t, _)| *t).collect();
        for t in times {
            let from = self.clock;
            self.advance_cold_to(t, out)?;
            let (hot_cells_stepped, fixed_steps_per_hot_cell) = self.step_hot_to(t, out)?;
            self.clock = t;
            if t != from {
                out.push(TimedEvent {
                    at: t,
                    event: Event::TimeAdvanced {
                        from,
                        to: t,
                        hot_cells_stepped,
                        fixed_steps_per_hot_cell,
                    },
                });
            }
            if let Some(v) = self.scheduled.remove(&t) {
                let mut pending = v.into_iter();
                while let Some((id, command)) = pending.next() {
                    match self.apply_scheduled(command) {
                        Ok(event) => out.push(TimedEvent { at: t, event }),
                        Err(e) => {
                            let queue = self.scheduled.entry(t).or_default();
                            queue.insert(id, command);
                            queue.extend(pending);
                            *blocked = Some(BlockedCommand { id, at: t, command });
                            return Err(e);
                        }
                    }
                    self.schedule_index.remove(&id);
                    if let ScheduledCommand::CreateStockpile { id, .. } = command {
                        self.reserved_stockpiles.remove(&id);
                    }
                }
            }
        }
        let from = self.clock;
        self.advance_cold_to(target, out)?;
        let (hot_cells_stepped, fixed_steps_per_hot_cell) = self.step_hot_to(target, out)?;
        self.clock = target;
        if target != from {
            out.push(TimedEvent {
                at: target,
                event: Event::TimeAdvanced {
                    from,
                    to: target,
                    hot_cells_stepped,
                    fixed_steps_per_hot_cell,
                },
            });
        }
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
                let n = self.soldiers.living[i];
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
    pub fn resource_totals(&self) -> ResourceTotals {
        let mut a = self
            .stockpiles
            .values()
            .map(|s| u128::from(s.ammunition))
            .sum();
        let supplies = self
            .stockpiles
            .values()
            .map(|s| u128::from(s.supplies))
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
        ResourceTotals {
            ammunition: a,
            stockpile_supplies: supplies,
            carried_food: f,
            carried_water: w,
            carried_medical: m,
            sourced_food: self.sourced_food,
            sourced_water: self.sourced_water,
            consumed_food: self.consumed_food,
            consumed_water: self.consumed_water,
            lost_food: self.lost_food,
            lost_water: self.lost_water,
        }
    }
    pub fn living_work_counters(&self) -> (u64, u64) {
        (self.cold_boundaries, self.hot_member_steps)
    }
    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u32(SNAPSHOT_VERSION);
        w.u64(self.clock);
        w.u64(self.seed);
        w.u64(self.rng_counter);
        w.u64(self.next_schedule_id);
        w.u128(self.sourced_food);
        w.u128(self.sourced_water);
        w.u128(self.consumed_food);
        w.u128(self.consumed_water);
        w.u128(self.lost_food);
        w.u128(self.lost_water);
        w.u64(self.cold_boundaries);
        w.u64(self.hot_member_steps);
        w.u32(self.soldiers.alive.len() as u32);
        for i in 0..self.soldiers.alive.len() {
            w.u32(self.soldiers.generation[i]);
            w.bool(self.soldiers.alive[i]);
            if self.soldiers.alive[i] {
                w.spec(self.soldiers.data[i]);
                w.living(self.soldiers.living[i]);
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
            for (id, command) in v {
                w.u64(*id);
                w.sc(*command)
            }
        }
        w.u32(self.living_due.len() as u32);
        for (at, ids) in &self.living_due {
            w.u64(*at);
            w.u32(ids.len() as u32);
            for id in ids {
                w.u64(id.raw())
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
        let sourced_food = r.u128()?;
        let sourced_water = r.u128()?;
        let consumed_food = r.u128()?;
        let consumed_water = r.u128()?;
        let lost_food = r.u128()?;
        let lost_water = r.u128()?;
        let cold_boundaries = r.u64()?;
        let hot_member_steps = r.u64()?;
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
            soldiers.living.push(if alive {
                r.living()?
            } else {
                LivingState::default()
            })
        }
        let mut seen = BTreeSet::new();
        for _ in 0..r.u32()? {
            let x = r.u32()?;
            if x as usize >= soldiers.alive.len()
                || soldiers.alive[x as usize]
                || soldiers.generation[x as usize] == 0
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
        let mut previous_squad = None;
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            if previous_squad.is_some_and(|previous| id <= previous) {
                return Err(SimError::Snapshot("noncanonical squads"));
            }
            previous_squad = Some(id);
            let officer = r.opt_id()?;
            let mut members = BTreeSet::new();
            let mut previous_member = None;
            for _ in 0..r.u32()? {
                let member = EntityId(r.u64()?);
                if previous_member.is_some_and(|previous| member <= previous)
                    || !members.insert(member)
                {
                    return Err(SimError::Snapshot("duplicate squad member"));
                }
                previous_member = Some(member);
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
        let mut previous_stockpile = None;
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            if previous_stockpile.is_some_and(|previous| id <= previous) {
                return Err(SimError::Snapshot("noncanonical stockpiles"));
            }
            previous_stockpile = Some(id);
            if stockpiles.insert(id, r.stock()?).is_some() {
                return Err(SimError::Snapshot("duplicate stockpile"));
            }
        }
        let mut hot_cells = BTreeMap::new();
        let mut previous_cell = None;
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            if previous_cell.is_some_and(|previous| id <= previous) {
                return Err(SimError::Snapshot("noncanonical hot cells"));
            }
            previous_cell = Some(id);
            let h = HotCellState {
                activated_at: r.u64()?,
                last_stepped_at: r.u64()?,
                fixed_steps: r.u64()?,
            };
            if h.activated_at > h.last_stepped_at
                || h.last_stepped_at != clock
                || h.fixed_steps != h.last_stepped_at - h.activated_at
                || hot_cells.insert(id, h).is_some()
            {
                return Err(SimError::Snapshot("hot cell"));
            }
        }
        let mut scheduled = BTreeMap::new();
        let mut schedule_index = HashMap::new();
        let mut reserved_stockpiles = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut previous_at = None;
        for _ in 0..r.u32()? {
            let at = r.u64()?;
            if previous_at.is_some_and(|previous| at <= previous) {
                return Err(SimError::Snapshot("noncanonical scheduled times"));
            }
            previous_at = Some(at);
            let mut v = BTreeMap::new();
            let mut previous_id = None;
            for _ in 0..r.u32()? {
                let id = r.u64()?;
                if id >= next_schedule_id || !ids.insert(id) || previous_id.is_some_and(|p| id <= p)
                {
                    return Err(SimError::Snapshot("schedule id"));
                }
                previous_id = Some(id);
                let command = r.sc()?;
                match command {
                    ScheduledCommand::Transfer { from, to, .. } if from == to => {
                        return Err(SimError::Snapshot("invalid scheduled command"))
                    }
                    ScheduledCommand::CreateStockpile { id, .. }
                        if stockpiles.contains_key(&id) || !reserved_stockpiles.insert(id) =>
                    {
                        return Err(SimError::Snapshot("conflicting reserved stockpile"))
                    }
                    _ => {}
                }
                v.insert(id, command);
                schedule_index.insert(id, at);
            }
            if at < clock || v.is_empty() || scheduled.insert(at, v).is_some() {
                return Err(SimError::Snapshot("scheduled time"));
            }
        }
        let mut living_due: BTreeMap<u64, BTreeSet<EntityId>> = BTreeMap::new();
        let mut due_by_entity = HashMap::new();
        let mut prev_due = None;
        for _ in 0..r.u32()? {
            let at = r.u64()?;
            if at <= clock || prev_due.is_some_and(|p| at <= p) {
                return Err(SimError::Snapshot("living due time"));
            }
            prev_due = Some(at);
            let mut ids = BTreeSet::new();
            let mut prev = None;
            for _ in 0..r.u32()? {
                let id = EntityId(r.u64()?);
                if prev.is_some_and(|p| id <= p)
                    || !soldiers.valid(id)
                    || soldiers.living[id.index()].life != LifeState::Alive
                    || !ids.insert(id)
                    || due_by_entity.insert(id, at).is_some()
                {
                    return Err(SimError::Snapshot("living due entity"));
                }
                prev = Some(id);
            }
            if ids.is_empty() {
                return Err(SimError::Snapshot("empty living due"));
            }
            living_due.insert(at, ids);
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
            schedule_index,
            reserved_stockpiles,
            cell_members: BTreeMap::new(),
            living_due,
            due_by_entity,
            sourced_food,
            sourced_water,
            consumed_food,
            consumed_water,
            lost_food,
            lost_water,
            cold_boundaries,
            hot_member_steps,
        };
        let mut w = w;
        for i in 0..w.soldiers.alive.len() {
            if w.soldiers.alive[i] {
                let id = EntityId::from_parts(i as u32, w.soldiers.generation[i]);
                w.cell_members
                    .entry(w.soldiers.data[i].position.cell)
                    .or_default()
                    .insert(id);
            }
        }
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
                if self.soldiers.living[i].materialized_at > self.clock
                    || (self.soldiers.living[i].life == LifeState::Alive
                        && self.soldiers.living[i].materialized_at != self.clock)
                {
                    return Err(SimError::Snapshot("living timestamp"));
                }
                if let Some(q) = self.soldiers.data[i].squad {
                    let id = EntityId::from_parts(i as u32, self.soldiers.generation[i]);
                    if !self.squads.get(&q).is_some_and(|q| q.members.contains(&id)) {
                        return Err(SimError::Snapshot("soldier squad"));
                    }
                }
            }
        }
        let carried = self.resource_totals();
        if self.sourced_food != carried.carried_food + self.consumed_food + self.lost_food
            || self.sourced_water != carried.carried_water + self.consumed_water + self.lost_water
        {
            return Err(SimError::Snapshot("resource ledger"));
        }
        for (cell, ids) in &self.cell_members {
            for id in ids {
                if !self.soldiers.valid(*id)
                    || self.soldiers.data[id.index()].position.cell != *cell
                {
                    return Err(SimError::Snapshot("cell membership"));
                }
            }
        }
        for i in 0..self.soldiers.alive.len() {
            if self.soldiers.alive[i] {
                let id = EntityId::from_parts(i as u32, self.soldiers.generation[i]);
                let hot = self
                    .hot_cells
                    .contains_key(&self.soldiers.data[i].position.cell);
                let alive = self.soldiers.living[i].life == LifeState::Alive;
                if self.due_by_entity.contains_key(&id) != (alive && !hot) {
                    return Err(SimError::Snapshot("living due coverage"));
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
    fn u128(&mut self, x: u128) {
        self.0.extend(x.to_le_bytes())
    }
    fn living(&mut self, l: LivingState) {
        self.u32(l.hunger);
        self.u32(l.thirst);
        self.u32(l.fatigue);
        self.u32(l.sleep_debt);
        self.u16(l.morale);
        self.u16(l.health);
        self.u8(l.activity as u8);
        match l.life {
            LifeState::Alive => self.u8(0),
            LifeState::Dead { at, cause } => {
                self.u8(1);
                self.u64(at);
                self.u8(cause as u8)
            }
        }
        self.u64(l.materialized_at)
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
    fn u128(&mut self) -> Result<u128, SimError> {
        Ok(u128::from_le_bytes(self.take()?))
    }
    fn living(&mut self) -> Result<LivingState, SimError> {
        let hunger = self.u32()?;
        let thirst = self.u32()?;
        let fatigue = self.u32()?;
        let sleep_debt = self.u32()?;
        let morale = self.u16()?;
        let health = self.u16()?;
        let activity = match self.u8()? {
            0 => Activity::Rest,
            1 => Activity::Idle,
            2 => Activity::March,
            _ => return Err(SimError::Snapshot("activity")),
        };
        let life = match self.u8()? {
            0 => LifeState::Alive,
            1 => {
                let at = self.u64()?;
                let cause = match self.u8()? {
                    0 => DeathCause::Dehydration,
                    1 => DeathCause::Starvation,
                    2 => DeathCause::Exhaustion,
                    _ => return Err(SimError::Snapshot("death cause")),
                };
                LifeState::Dead { at, cause }
            }
            _ => return Err(SimError::Snapshot("life state")),
        };
        let materialized_at = self.u64()?;
        if hunger > NEED_MAX
            || thirst > NEED_MAX
            || fatigue > NEED_MAX
            || sleep_debt > NEED_MAX
            || morale > 1000
            || health > 1000
        {
            return Err(SimError::Snapshot("living range"));
        }
        if let LifeState::Dead { at, .. } = life {
            if at != materialized_at || health != 0 {
                return Err(SimError::Snapshot("death state"));
            }
        }
        Ok(LivingState {
            hunger,
            thirst,
            fatigue,
            sleep_debt,
            morale,
            health,
            activity,
            life,
            materialized_at,
        })
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

#[cfg(test)]
mod private_invariants {
    use super::*;

    #[test]
    fn generation_exhaustion_retires_slot_permanently() {
        let mut soldiers = Soldiers::default();
        soldiers.generation.push(u32::MAX - 1);
        soldiers.alive.push(true);
        soldiers.data.push(SoldierSpec::default());
        soldiers.living.push(LivingState::default());
        soldiers.live = 1;
        let old = EntityId::from_parts(0, u32::MAX - 1);
        assert!(soldiers.remove(old).is_some());
        let last = soldiers.spawn(SoldierSpec::default(), 0);
        assert_eq!(last, EntityId::from_parts(0, u32::MAX));
        assert!(soldiers.remove(last).is_some());
        assert!(soldiers.free.is_empty());
        let fresh = soldiers.spawn(SoldierSpec::default(), 0);
        assert_eq!(fresh.index(), 1);
        assert!(!soldiers.valid(old));
    }

    #[test]
    fn rng_wrap_is_explicit_and_snapshot_restore_rejects_invalid_internal_state() {
        let mut world = World::new(9);
        world.rng_counter = u64::MAX;
        assert!(world.apply(Command::NextRandom).error.is_none());
        assert_eq!(world.rng_counter, 0);
        let mut invalid = World::new(0);
        invalid.scheduled.entry(1).or_default().insert(
            0,
            ScheduledCommand::Transfer {
                from: 2,
                to: 2,
                ammunition: 0,
                supplies: 0,
            },
        );
        invalid.schedule_index.insert(0, 1);
        invalid.next_schedule_id = 1;
        assert!(matches!(
            World::from_snapshot(&invalid.snapshot()),
            Err(SimError::Snapshot("invalid scheduled command"))
        ));
        let mut hot = World::new(0);
        hot.hot_cells.insert(
            3,
            HotCellState {
                activated_at: 0,
                last_stepped_at: 0,
                fixed_steps: 1,
            },
        );
        assert!(matches!(
            World::from_snapshot(&hot.snapshot()),
            Err(SimError::Snapshot("hot cell"))
        ));
    }

    #[test]
    fn restore_rejects_unreachable_zero_generation_free_slot() {
        let mut world = World::new(0);
        world.soldiers.generation.push(0);
        world.soldiers.alive.push(false);
        world.soldiers.data.push(SoldierSpec::default());
        world.soldiers.living.push(LivingState::default());
        world.soldiers.free.push(0);
        assert!(matches!(
            World::from_snapshot(&world.snapshot()),
            Err(SimError::Snapshot("invalid free slot"))
        ));
    }

    #[test]
    fn snapshot_preserves_lifo_allocator_with_retirement_and_stale_ids() {
        let mut world = World::new(17);
        world.soldiers.generation.push(u32::MAX - 1);
        world.soldiers.alive.push(true);
        world.soldiers.data.push(SoldierSpec::default());
        world.soldiers.living.push(LivingState::default());
        world.soldiers.live = 1;
        let near_retirement = EntityId::from_parts(0, u32::MAX - 1);
        assert!(world
            .apply(Command::DespawnSoldier {
                id: near_retirement
            })
            .error
            .is_none());
        let final_generation = match world
            .apply(Command::SpawnSoldier {
                spec: SoldierSpec::default(),
            })
            .events[0]
            .event
        {
            Event::SoldierSpawned { id, .. } => id,
            _ => unreachable!(),
        };
        assert_eq!(final_generation, EntityId::from_parts(0, u32::MAX));
        assert!(world
            .apply(Command::DespawnSoldier {
                id: final_generation
            })
            .error
            .is_none());

        let live: Vec<_> = (0..6)
            .map(|rank| {
                let outcome = world.apply(Command::SpawnSoldier {
                    spec: SoldierSpec {
                        rank,
                        ..SoldierSpec::default()
                    },
                });
                match outcome.events[0].event {
                    Event::SoldierSpawned { id, .. } => id,
                    _ => unreachable!(),
                }
            })
            .collect();
        let mut stale = vec![near_retirement, final_generation];
        for index in [1usize, 4, 2] {
            stale.push(live[index]);
            assert!(world
                .apply(Command::DespawnSoldier { id: live[index] })
                .error
                .is_none());
        }
        let mut restored = World::from_snapshot(&world.snapshot()).unwrap();
        assert!(stale
            .iter()
            .all(|id| world.soldier(*id).is_none() && restored.soldier(*id).is_none()));
        for rank in 20..24 {
            let command = Command::SpawnSoldier {
                spec: SoldierSpec {
                    rank,
                    ..SoldierSpec::default()
                },
            };
            let a = world.apply(command);
            let b = restored.apply(command);
            assert_eq!(a, b);
            assert!(stale
                .iter()
                .all(|id| world.soldier(*id).is_none() && restored.soldier(*id).is_none()));
        }
        assert_eq!(world.state_digest(), restored.state_digest());
    }
}
