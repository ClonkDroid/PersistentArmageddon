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
    InvalidHealth,
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
    /// Test-only structural evidence: entities selected from a due bucket or
    /// indexed hot membership for an automatic timestamp. This is deliberately
    /// absent from snapshots, digests, and release builds.
    #[cfg(test)]
    automatic_journal_visits: u64,
    #[cfg(test)]
    automatic_execution_visits: u64,
}

#[derive(Clone)]
struct LivingTransition {
    living: LivingState,
    inventory: Inventory,
    consumed_food: u128,
    consumed_water: u128,
    events: Vec<TimedEvent>,
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
        if let Some(at) = self.canonical_due(id)? {
            self.living_due.entry(at).or_default().insert(id);
            self.due_by_entity.insert(id, at);
        }
        Ok(())
    }
    /// `None` means that the entity is alive at the terminal instant and no
    /// representable future transition exists. Time is closed at `u64::MAX`.
    fn canonical_due(&self, id: EntityId) -> Result<Option<u64>, SimError> {
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
        if l.activity == Activity::March {
            d = d.min(if l.fatigue >= FORCED_IDLE_FATIGUE {
                1
            } else {
                ceil(FORCED_IDLE_FATIGUE - l.fatigue, 3)
            });
        }
        Ok(l.materialized_at.checked_add(d))
    }
    fn project(l: &mut LivingState, to: u64) -> Result<(), SimError> {
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
    fn transition_second(
        living: LivingState,
        inventory: Inventory,
        id: EntityId,
        at: u64,
    ) -> Result<LivingTransition, SimError> {
        if living.life != LifeState::Alive {
            return Ok(LivingTransition {
                living,
                inventory,
                consumed_food: 0,
                consumed_water: 0,
                events: Vec::new(),
            });
        }
        let mut l = living;
        Self::project(&mut l, at)?;
        let mut inv = inventory;
        let mut consumed_food = 0_u128;
        let mut consumed_water = 0_u128;
        let mut events = Vec::new();
        let hb = l.hunger;
        let tb = l.thirst;
        let mut food = 0;
        let mut water = 0;
        if l.hunger >= RATION_THRESHOLD && inv.food > 0 {
            inv.food -= FOOD_RATION;
            food = FOOD_RATION;
            l.hunger -= RATION_THRESHOLD;
            consumed_food = 1;
        }
        if l.thirst >= RATION_THRESHOLD && inv.water > 0 {
            inv.water -= WATER_RATION;
            water = WATER_RATION;
            l.thirst -= RATION_THRESHOLD;
            consumed_water = 1;
        }
        if food != 0 || water != 0 {
            events.push(TimedEvent {
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
            events.push(TimedEvent {
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
            events.push(TimedEvent {
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
                events.push(TimedEvent {
                    at,
                    event: Event::SoldierDied {
                        id,
                        cause,
                        health_before: hb,
                    },
                });
            }
        }
        Ok(LivingTransition {
            living: l,
            inventory: inv,
            consumed_food,
            consumed_water,
            events,
        })
    }
    fn commit_transition(
        &mut self,
        id: EntityId,
        transition: LivingTransition,
        out: &mut Vec<TimedEvent>,
    ) {
        let i = id.index();
        self.soldiers.living[i] = transition.living;
        self.soldiers.data[i].inventory = transition.inventory;
        self.soldiers.data[i].health = transition.living.health;
        self.consumed_food += transition.consumed_food;
        self.consumed_water += transition.consumed_water;
        out.extend(transition.events);
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
            #[cfg(test)]
            automatic_journal_visits: 0,
            #[cfg(test)]
            automatic_execution_visits: 0,
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
        let mut living = self.soldiers.living[i];
        if living.life == LifeState::Alive {
            Self::project(&mut living, self.clock).ok()?;
        }
        Some(Soldier {
            id,
            faction: s.faction,
            position: s.position,
            squad: s.squad,
            role: s.role,
            rank: s.rank,
            health: living.health,
            needs: Needs {
                fatigue: living.fatigue,
                hunger: living.hunger,
                thirst: living.thirst,
                sleep_debt: living.sleep_debt,
            },
            ammunition: s.ammunition,
            inventory: s.inventory,
            living,
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
                if spec.health == 0 || spec.health > 1000 {
                    return Err(SimError::InvalidHealth);
                }
                if let Some(s) = spec.squad {
                    if !self.squads.contains_key(&s) {
                        return Err(SimError::InvalidSquad);
                    }
                }
                let new_food = self
                    .sourced_food
                    .checked_add(u128::from(spec.inventory.food))
                    .ok_or(SimError::ArithmeticOverflow)?;
                let new_water = self
                    .sourced_water
                    .checked_add(u128::from(spec.inventory.water))
                    .ok_or(SimError::ArithmeticOverflow)?;
                // A cold spawn must have either a representable first boundary
                // or be at the terminal instant, where no future instant exists.
                if !self.hot_cells.contains_key(&spec.position.cell) && self.clock != u64::MAX {
                    self.clock
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                }
                let id = self.soldiers.spawn(spec, self.clock);
                self.sourced_food = new_food;
                self.sourced_water = new_water;
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
                let lost_food = self
                    .lost_food
                    .checked_add(u128::from(spec.inventory.food))
                    .ok_or(SimError::ArithmeticOverflow)?;
                let lost_water = self
                    .lost_water
                    .checked_add(u128::from(spec.inventory.water))
                    .ok_or(SimError::ArithmeticOverflow)?;
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
                self.lost_food = lost_food;
                self.lost_water = lost_water;
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
                let old = self.soldiers.living[id.index()];
                let before = old.activity;
                let mut replacement = old;
                Self::project(&mut replacement, self.clock)?;
                replacement.activity = activity;
                let hot = self
                    .hot_cells
                    .contains_key(&self.soldiers.data[id.index()].position.cell);
                let due = if hot {
                    None
                } else {
                    self.soldiers.living[id.index()] = replacement;
                    let result = self.canonical_due(id);
                    self.soldiers.living[id.index()] = old;
                    result?
                };
                self.unschedule_due(id);
                self.soldiers.living[id.index()] = replacement;
                if !hot {
                    if let Some(at) = due {
                        self.living_due.entry(at).or_default().insert(id);
                        self.due_by_entity.insert(id, at);
                    }
                }
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
                // Stage every touched living record and replacement due time before
                // changing fidelity or either index. This is proportional to the
                // cell membership, never to the whole world.
                let mut staged = Vec::with_capacity(members.len());
                for id in &members {
                    let mut living = self.soldiers.living[id.index()];
                    Self::project(&mut living, self.clock)?;
                    let due = if !hot && living.life == LifeState::Alive {
                        let old = self.soldiers.living[id.index()];
                        self.soldiers.living[id.index()] = living;
                        let result = self.canonical_due(*id);
                        self.soldiers.living[id.index()] = old;
                        result?
                    } else {
                        None
                    };
                    staged.push((*id, living, due));
                }
                for (id, living, _) in &staged {
                    self.soldiers.living[id.index()] = *living;
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
                    for (id, _, due) in staged {
                        if let Some(at) = due {
                            self.living_due.entry(at).or_default().insert(id);
                            self.due_by_entity.insert(id, at);
                        }
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
    fn preflight_hot_cells(&self, t: u64) -> Result<(), SimError> {
        for h in self.hot_cells.values() {
            let elapsed = t
                .checked_sub(h.last_stepped_at)
                .ok_or(SimError::TimeReversal)?;
            h.fixed_steps
                .checked_add(elapsed)
                .ok_or(SimError::ArithmeticOverflow)?;
        }
        Ok(())
    }
    fn automatic_to_inner(
        &mut self,
        t: u64,
        out: &mut Vec<TimedEvent>,
    ) -> Result<(u64, u64), SimError> {
        self.preflight_hot_cells(t)?;
        loop {
            let cold_at = self.living_due.keys().next().copied().filter(|at| *at <= t);
            let hot_at = self
                .hot_cells
                .iter()
                .filter(|(cell, h)| {
                    h.last_stepped_at < t
                        && self.cell_members.get(cell).into_iter().flatten().any(|id| {
                            self.soldiers.valid(*id)
                                && self.soldiers.living[id.index()].life == LifeState::Alive
                        })
                })
                .map(|(_, h)| h)
                .map(|h| h.last_stepped_at + 1)
                .min();
            let Some(at) = cold_at.into_iter().chain(hot_at).min() else {
                break;
            };
            let mut ids = BTreeSet::new();
            if cold_at == Some(at) {
                ids.extend(self.living_due.get(&at).into_iter().flatten().copied());
            }
            for (cell, h) in &self.hot_cells {
                if h.last_stepped_at < at {
                    ids.extend(self.cell_members.get(cell).into_iter().flatten().copied());
                }
            }
            let mut staged = Vec::new();
            let mut hot_count = 0_u64;
            let mut cold_count = 0_u64;
            let mut food_count = 0_u128;
            let mut water_count = 0_u128;
            for id in ids {
                #[cfg(test)]
                {
                    self.automatic_execution_visits += 1;
                }
                if !self.soldiers.valid(id)
                    || self.soldiers.living[id.index()].life != LifeState::Alive
                {
                    staged.push((id, false, None));
                    continue;
                }
                let cell = self.soldiers.data[id.index()].position.cell;
                let is_hot = self
                    .hot_cells
                    .get(&cell)
                    .is_some_and(|h| h.last_stepped_at < at);
                if is_hot {
                    hot_count = hot_count
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    let transition = Self::transition_second(
                        self.soldiers.living[id.index()],
                        self.soldiers.data[id.index()].inventory,
                        id,
                        at,
                    )?;
                    food_count = food_count
                        .checked_add(transition.consumed_food)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    water_count = water_count
                        .checked_add(transition.consumed_water)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    staged.push((id, true, Some(transition)));
                } else if self.due_by_entity.get(&id) == Some(&at) {
                    cold_count = cold_count
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    let transition = Self::transition_second(
                        self.soldiers.living[id.index()],
                        self.soldiers.data[id.index()].inventory,
                        id,
                        at,
                    )?;
                    food_count = food_count
                        .checked_add(transition.consumed_food)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    water_count = water_count
                        .checked_add(transition.consumed_water)
                        .ok_or(SimError::ArithmeticOverflow)?;
                    staged.push((id, false, Some(transition)));
                }
            }
            let next_hot = self
                .hot_member_steps
                .checked_add(hot_count)
                .ok_or(SimError::ArithmeticOverflow)?;
            let next_cold = self
                .cold_boundaries
                .checked_add(cold_count)
                .ok_or(SimError::ArithmeticOverflow)?;
            self.consumed_food
                .checked_add(food_count)
                .ok_or(SimError::ArithmeticOverflow)?;
            self.consumed_water
                .checked_add(water_count)
                .ok_or(SimError::ArithmeticOverflow)?;
            for (id, hot, transition) in staged {
                if let Some(transition) = transition {
                    if !hot {
                        self.unschedule_due(id);
                    }
                    self.commit_transition(id, transition, out);
                    if !hot {
                        self.schedule_due(id)?;
                    }
                } else {
                    self.unschedule_due(id);
                }
            }
            self.hot_member_steps = next_hot;
            self.cold_boundaries = next_cold;
            for h in self
                .hot_cells
                .values_mut()
                .filter(|h| h.last_stepped_at < at)
            {
                h.fixed_steps += 1;
                h.last_stepped_at = at;
            }
        }
        for h in self.hot_cells.values_mut() {
            let elapsed = t - h.last_stepped_at;
            h.fixed_steps += elapsed;
            h.last_stepped_at = t;
        }
        let cells = self.hot_cells.len() as u64;
        Ok(if cells == 0 {
            (0, 0)
        } else {
            (cells, t - self.clock)
        })
    }

    /// Runs one externally committed automatic segment transactionally.  The
    /// journal contains only records which can be reached by this segment: due
    /// entities through `t`, indexed members of hot cells, and the hot cells
    /// whose clocks advance.  It deliberately does not clone `World` or either
    /// world-sized entity index.
    fn automatic_to(&mut self, t: u64, out: &mut Vec<TimedEvent>) -> Result<(u64, u64), SimError> {
        let mut touched = BTreeSet::new();
        for (_, ids) in self.living_due.range(..=t) {
            #[cfg(test)]
            {
                self.automatic_journal_visits += ids.len() as u64;
            }
            touched.extend(ids.iter().copied());
        }
        for cell in self.hot_cells.keys() {
            touched.extend(self.cell_members.get(cell).into_iter().flatten().copied());
        }
        let records: Vec<_> = touched
            .iter()
            .filter(|id| self.soldiers.valid(**id))
            .map(|id| {
                (
                    *id,
                    self.soldiers.data[id.index()],
                    self.soldiers.living[id.index()],
                )
            })
            .collect();
        let due: Vec<_> = touched
            .iter()
            .map(|id| (*id, self.due_by_entity.get(id).copied()))
            .collect();
        let hot_cells = self.hot_cells.clone();
        let counters = (
            self.consumed_food,
            self.consumed_water,
            self.cold_boundaries,
            self.hot_member_steps,
        );
        let out_len = out.len();

        match self.automatic_to_inner(t, out) {
            Ok(result) => Ok(result),
            Err(error) => {
                // Remove both original and replacement reverse/bucket entries,
                // then reconstruct exactly the pre-segment index entries.
                for id in &touched {
                    self.unschedule_due(*id);
                }
                for (id, data, living) in records {
                    self.soldiers.data[id.index()] = data;
                    self.soldiers.living[id.index()] = living;
                }
                for (id, at) in due {
                    if let Some(at) = at {
                        self.living_due.entry(at).or_default().insert(id);
                        self.due_by_entity.insert(id, at);
                    }
                }
                self.hot_cells = hot_cells;
                self.consumed_food = counters.0;
                self.consumed_water = counters.1;
                self.cold_boundaries = counters.2;
                self.hot_member_steps = counters.3;
                out.truncate(out_len);
                Err(error)
            }
        }
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
            let (hot_cells_stepped, fixed_steps_per_hot_cell) = self.automatic_to(t, out)?;
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
        let (hot_cells_stepped, fixed_steps_per_hot_cell) = self.automatic_to(target, out)?;
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
                let soldier = self.soldier(id).expect("allocated soldier");
                let mut bytes = W::default();
                bytes.u64(id.raw());
                bytes.living(soldier.living);
                bytes.u32(soldier.inventory.food);
                bytes.u32(soldier.inventory.water);
                bytes.u32(soldier.inventory.medical);
                bytes.u16(soldier.health);
                for b in bytes.0 {
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
            #[cfg(test)]
            automatic_journal_visits: 0,
            #[cfg(test)]
            automatic_execution_visits: 0,
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
                let living = self.soldiers.living[i];
                if living.materialized_at > self.clock
                    || self.soldiers.data[i].health != living.health
                    || (living.life == LifeState::Alive && living.health == 0)
                    || matches!(living.life, LifeState::Dead { at, .. } if at != living.materialized_at || at > self.clock || living.health != 0)
                {
                    return Err(SimError::Snapshot("living timestamp"));
                }
                if living.life == LifeState::Alive
                    && self
                        .hot_cells
                        .contains_key(&self.soldiers.data[i].position.cell)
                    && living.materialized_at != self.clock
                {
                    return Err(SimError::Snapshot("hot living materialization"));
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
        let accounted_food = carried
            .carried_food
            .checked_add(self.consumed_food)
            .and_then(|x| x.checked_add(self.lost_food));
        let accounted_water = carried
            .carried_water
            .checked_add(self.consumed_water)
            .and_then(|x| x.checked_add(self.lost_water));
        if Some(self.sourced_food) != accounted_food || Some(self.sourced_water) != accounted_water
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
                let expected = if alive && !hot {
                    self.canonical_due(id)?
                } else {
                    None
                };
                if self.due_by_entity.get(&id).copied() != expected {
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

    fn spawn(world: &mut World, spec: SoldierSpec) -> EntityId {
        match world.apply(Command::SpawnSoldier { spec }).events[0].event {
            Event::SoldierSpawned { id, .. } => id,
            _ => unreachable!(),
        }
    }

    #[test]
    fn already_fatigued_marchers_force_idle_next_second_on_each_path() {
        for fatigue in [FORCED_IDLE_FATIGUE, FORCED_IDLE_FATIGUE + 1] {
            for hot in [false, true] {
                let mut world = World::new(0);
                let id = spawn(
                    &mut world,
                    SoldierSpec {
                        inventory: Inventory {
                            food: 100,
                            water: 100,
                            medical: 7,
                        },
                        ammunition: 11,
                        ..SoldierSpec::default()
                    },
                );
                assert!(world
                    .apply(Command::AdvanceTo { target: 900 })
                    .error
                    .is_none());
                world.soldiers.living[id.index()].fatigue = fatigue;
                world.schedule_due(id).unwrap();
                if hot {
                    assert!(world
                        .apply(Command::SetRegionHot { cell: 0, hot: true })
                        .error
                        .is_none());
                }
                let changed = world.apply(Command::SetActivity {
                    id,
                    activity: Activity::March,
                });
                assert_eq!(
                    changed.events,
                    vec![TimedEvent {
                        at: 900,
                        event: Event::ActivityChanged {
                            id,
                            before: Activity::Idle,
                            after: Activity::March,
                            forced: false,
                        },
                    }]
                );
                if hot {
                    assert!(!world.due_by_entity.contains_key(&id));
                } else {
                    assert_eq!(world.due_by_entity.get(&id), Some(&901));
                }

                let checkpoint = world.snapshot();
                let mut restored = World::from_snapshot(&checkpoint).unwrap();
                let expected_events = vec![
                    TimedEvent {
                        at: 901,
                        event: Event::ActivityChanged {
                            id,
                            before: Activity::March,
                            after: Activity::Idle,
                            forced: true,
                        },
                    },
                    TimedEvent {
                        at: 901,
                        event: Event::TimeAdvanced {
                            from: 900,
                            to: 901,
                            hot_cells_stepped: u64::from(hot),
                            fixed_steps_per_hot_cell: u64::from(hot),
                        },
                    },
                ];
                for candidate in [&mut world, &mut restored] {
                    let outcome = candidate.apply(Command::AdvanceTo { target: 901 });
                    assert_eq!(outcome.error, None);
                    assert_eq!(outcome.events, expected_events);
                    assert_eq!(
                        candidate.soldier(id),
                        Some(Soldier {
                            id,
                            faction: 0,
                            position: Position::default(),
                            squad: None,
                            role: Role::Rifle,
                            rank: 0,
                            health: 1000,
                            needs: Needs {
                                fatigue: fatigue + 3,
                                hunger: 2,
                                thirst: 3,
                                sleep_debt: 902,
                            },
                            ammunition: 11,
                            inventory: Inventory {
                                food: 91,
                                water: 82,
                                medical: 7,
                            },
                            living: LivingState {
                                hunger: 2,
                                thirst: 3,
                                fatigue: fatigue + 3,
                                sleep_debt: 902,
                                morale: 1000,
                                health: 1000,
                                activity: Activity::Idle,
                                life: LifeState::Alive,
                                materialized_at: 901,
                            },
                        })
                    );
                    assert_eq!(
                        candidate.resource_totals(),
                        ResourceTotals {
                            ammunition: 11,
                            stockpile_supplies: 0,
                            carried_food: 91,
                            carried_water: 82,
                            carried_medical: 7,
                            sourced_food: 100,
                            sourced_water: 100,
                            consumed_food: 9,
                            consumed_water: 18,
                            lost_food: 0,
                            lost_water: 0,
                        }
                    );
                    assert_eq!(
                        candidate.living_work_counters(),
                        if hot { (18, 1) } else { (19, 0) }
                    );
                }
                assert_eq!(world.snapshot(), restored.snapshot());
            }
        }
    }

    #[test]
    fn v6_hot_living_materialization_must_equal_clock() {
        const MATERIALIZED_AT: usize = 214;
        let mut hot = World::new(0);
        assert!(hot
            .apply(Command::SetRegionHot { cell: 0, hot: true })
            .error
            .is_none());
        let _id = spawn(&mut hot, SoldierSpec::default());
        assert!(hot.apply(Command::AdvanceTo { target: 10 }).error.is_none());
        let canonical = hot.snapshot();
        let mut resumed = World::from_snapshot(&canonical).unwrap();
        let uninterrupted = hot.apply(Command::AdvanceTo { target: 11 });
        let resumed_outcome = resumed.apply(Command::AdvanceTo { target: 11 });
        assert_eq!(uninterrupted, resumed_outcome);
        assert_eq!(hot.snapshot(), resumed.snapshot());

        let mut stale = canonical.clone();
        overwrite(&mut stale, MATERIALIZED_AT, 9_u64.to_le_bytes());
        assert_eq!(
            World::from_snapshot(&stale).err(),
            Some(SimError::Snapshot("hot living materialization"))
        );

        let mut cold = World::new(0);
        let cold_id = spawn(&mut cold, SoldierSpec::default());
        assert!(cold
            .apply(Command::AdvanceTo { target: 10 })
            .error
            .is_none());
        assert_eq!(cold.soldiers.living[cold_id.index()].materialized_at, 0);
        assert!(World::from_snapshot(&cold.snapshot()).is_ok());

        let mut dead_hot = World::new(0);
        assert!(dead_hot
            .apply(Command::SetRegionHot { cell: 0, hot: true })
            .error
            .is_none());
        let dead = spawn(
            &mut dead_hot,
            SoldierSpec {
                health: 10,
                ..SoldierSpec::default()
            },
        );
        dead_hot.soldiers.living[dead.index()].thirst = SEVERE_THIRST;
        assert!(dead_hot
            .apply(Command::AdvanceTo { target: 10 })
            .error
            .is_none());
        assert_eq!(dead_hot.soldiers.living[dead.index()].materialized_at, 1);
        let steps = dead_hot.living_work_counters();
        let mut dead_restored = World::from_snapshot(&dead_hot.snapshot()).unwrap();
        let outcome = dead_restored.apply(Command::AdvanceTo { target: 11 });
        assert_eq!(outcome.error, None);
        assert_eq!(dead_restored.living_work_counters(), steps);
        assert_eq!(
            dead_restored.soldiers.living[dead.index()].materialized_at,
            1
        );
        assert!(outcome.events.iter().all(|event| !matches!(
            event.event,
            Event::RationConsumed { .. }
                | Event::ActivityChanged { forced: true, .. }
                | Event::LivingDeteriorated { .. }
                | Event::SoldierDied { .. }
        )));
    }

    #[test]
    fn health_is_one_projection_and_impossible_health_is_rejected() {
        let mut world = World::new(0);
        assert_eq!(
            world
                .apply(Command::SpawnSoldier {
                    spec: SoldierSpec {
                        health: 0,
                        ..SoldierSpec::default()
                    }
                })
                .error,
            Some(SimError::InvalidHealth)
        );
        let id = spawn(
            &mut world,
            SoldierSpec {
                health: 10,
                inventory: Inventory::default(),
                ..SoldierSpec::default()
            },
        );
        world.soldiers.living[id.index()].hunger = SEVERE_HUNGER;
        world.soldiers.living[id.index()].thirst = SEVERE_THIRST;
        world.schedule_due(id).unwrap();
        assert!(world
            .apply(Command::AdvanceTo { target: 1 })
            .error
            .is_none());
        let soldier = world.soldier(id).unwrap();
        assert_eq!(soldier.health, soldier.living.health);
        assert!(matches!(
            soldier.living.life,
            LifeState::Dead {
                at: 1,
                cause: DeathCause::Dehydration
            }
        ));
        let mut corrupt = world.clone();
        corrupt.soldiers.data[id.index()].health = 1;
        assert!(World::from_snapshot(&corrupt.snapshot()).is_err());
        corrupt = world.clone();
        corrupt.soldiers.living[id.index()].life = LifeState::Alive;
        assert!(World::from_snapshot(&corrupt.snapshot()).is_err());
    }

    #[test]
    fn canonical_due_sparse_queries_and_corruption_are_proven() {
        let mut world = World::new(0);
        let id = spawn(
            &mut world,
            SoldierSpec {
                inventory: Inventory {
                    food: 3,
                    water: 3,
                    medical: 7,
                },
                ..SoldierSpec::default()
            },
        );
        for at in 1..200 {
            world.apply(Command::Schedule {
                at,
                command: ScheduledCommand::SetRegionHot {
                    cell: at as u32 + 10,
                    hot: true,
                },
            });
        }
        let before = world.snapshot();
        assert_eq!(world.soldier(id).unwrap().living.materialized_at, 0);
        assert_eq!(world.snapshot(), before);
        world.apply(Command::AdvanceTo { target: 49 });
        assert_eq!(world.living_work_counters().0, 0);
        assert_eq!(world.soldier(id).unwrap().living.hunger, 49);
        let mut corrupt = world.clone();
        let due = corrupt.due_by_entity[&id];
        corrupt.living_due.get_mut(&due).unwrap().remove(&id);
        corrupt.living_due.entry(due + 1).or_default().insert(id);
        corrupt.due_by_entity.insert(id, due + 1);
        assert!(World::from_snapshot(&corrupt.snapshot()).is_err());
    }

    #[test]
    fn terminal_time_and_all_authoritative_overflows_are_atomic() {
        let mut world = World::new(0);
        world.clock = u64::MAX;
        world.sourced_food = u128::MAX;
        let digest = world.state_digest();
        assert_eq!(
            world
                .apply(Command::SpawnSoldier {
                    spec: SoldierSpec {
                        inventory: Inventory {
                            food: 1,
                            ..Inventory::default()
                        },
                        ..SoldierSpec::default()
                    }
                })
                .error,
            Some(SimError::ArithmeticOverflow)
        );
        assert_eq!(world.state_digest(), digest);
        world.sourced_food = 0;
        world.hot_cells.insert(
            1,
            HotCellState {
                activated_at: u64::MAX,
                last_stepped_at: u64::MAX,
                fixed_steps: 0,
            },
        );
        let id = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        assert!(world
            .apply(Command::AdvanceTo { target: u64::MAX })
            .error
            .is_none());
        assert_eq!(world.soldier(id).unwrap().living.materialized_at, u64::MAX);
        world.hot_member_steps = u64::MAX;
        let digest = world.state_digest();
        assert_eq!(
            world.apply(Command::AdvanceTo { target: u64::MAX }).error,
            None
        );
        assert_eq!(world.state_digest(), digest);
    }

    #[test]
    fn same_timestamp_cold_counter_overflow_commits_nothing() {
        let mut world = World::new(0);
        world.apply(Command::SetRegionHot { cell: 1, hot: true });
        let hot = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        let cold = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 2,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        world.soldiers.living[cold.index()].thirst = SEVERE_THIRST - 2;
        world.schedule_due(cold).unwrap();
        assert_eq!(world.due_by_entity[&cold], 1);
        world.cold_boundaries = u64::MAX;
        let before = world.snapshot();
        let outcome = world.apply(Command::AdvanceTo { target: 1 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(world.clock(), 0);
        assert_eq!(world.soldier(hot).unwrap().living.materialized_at, 0);
        assert_eq!(world.snapshot(), before);
        assert!(World::from_snapshot(&world.snapshot()).is_ok());
    }

    #[test]
    fn complete_hot_segment_rolls_back_all_internal_seconds() {
        let mut world = World::new(0);
        world.apply(Command::SetRegionHot { cell: 1, hot: true });
        let id = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        world.hot_member_steps = u64::MAX - 1;
        let before = world.snapshot();
        let digest = world.state_digest();
        let outcome = world.apply(Command::AdvanceTo { target: 2 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(world.clock, 0);
        assert_eq!(world.soldiers.living[id.index()].materialized_at, 0);
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.state_digest(), digest);
        assert!(World::from_snapshot(&before).is_ok());
    }

    #[test]
    fn complete_cold_segment_rolls_back_all_internal_boundaries() {
        let mut world = World::new(0);
        let id = spawn(&mut world, SoldierSpec::default());
        world.soldiers.living[id.index()].thirst = SEVERE_THIRST;
        world.soldiers.data[id.index()].inventory.water = 0;
        world.schedule_due(id).unwrap();
        world.cold_boundaries = u64::MAX - 1;
        let before = world.snapshot();
        let outcome = world.apply(Command::AdvanceTo { target: 2 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(world.snapshot(), before);
        assert!(World::from_snapshot(&world.snapshot()).is_ok());
    }

    #[test]
    fn later_ledger_overflow_rolls_back_earlier_ration_and_events() {
        let mut world = World::new(0);
        let id = spawn(
            &mut world,
            SoldierSpec {
                inventory: Inventory {
                    food: 0,
                    water: 2,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        world.soldiers.living[id.index()].thirst = RATION_THRESHOLD - 1;
        world.schedule_due(id).unwrap();
        world.consumed_water = u128::MAX - 1;
        let before = world.snapshot();
        let outcome = world.apply(Command::AdvanceTo { target: 51 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(world.snapshot(), before);
    }

    #[test]
    fn failed_later_segment_preserves_earlier_scheduled_prefix() {
        let mut world = World::new(0);
        let id = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 4,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        world.apply(Command::Schedule {
            at: 1,
            command: ScheduledCommand::SetRegionHot { cell: 4, hot: true },
        });
        world.hot_member_steps = u64::MAX - 1;
        let outcome = world.apply(Command::AdvanceTo { target: 3 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert_eq!(world.clock, 1);
        assert!(world.hot_cells.contains_key(&4));
        assert_eq!(world.soldiers.living[id.index()].materialized_at, 1);
        assert_eq!(world.hot_member_steps, u64::MAX - 1);
        assert!(World::from_snapshot(&world.snapshot()).is_ok());
        assert!(outcome.events.iter().any(|event| matches!(
            event.event,
            Event::RegionFidelityChanged {
                cell: 4,
                hot: true,
                ..
            }
        )));
    }

    #[test]
    fn stale_due_entry_cannot_execute_after_generation_reuse() {
        let mut world = World::new(0);
        let stale = spawn(&mut world, SoldierSpec::default());
        world.apply(Command::DespawnSoldier { id: stale });
        let current = spawn(&mut world, SoldierSpec::default());
        assert_ne!(stale, current);
        world.living_due.entry(1).or_default().insert(stale);
        world.due_by_entity.insert(stale, 1);
        let current_before = world.soldiers.living[current.index()];
        let outcome = world.apply(Command::AdvanceTo { target: 1 });
        assert_eq!(outcome.error, None);
        assert!(!world.due_by_entity.contains_key(&stale));
        assert_eq!(world.soldiers.living[current.index()], current_before);
        assert_eq!(world.living_work_counters().0, 0);
    }

    #[test]
    fn hot_activity_never_installs_a_cold_due_entry() {
        let mut world = World::new(0);
        world.apply(Command::SetRegionHot { cell: 7, hot: true });
        let id = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 7,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        assert!(world
            .apply(Command::SetActivity {
                id,
                activity: Activity::March,
            })
            .error
            .is_none());
        assert!(!world.due_by_entity.contains_key(&id));
        assert!(World::from_snapshot(&world.snapshot()).is_ok());
        world.apply(Command::SetRegionHot {
            cell: 7,
            hot: false,
        });
        assert!(world.due_by_entity.contains_key(&id));
    }

    #[test]
    fn exact_hot_counter_capacity_accounts_for_death() {
        let mut world = World::new(0);
        world.apply(Command::SetRegionHot { cell: 1, hot: true });
        let id = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                health: 10,
                ..SoldierSpec::default()
            },
        );
        world.soldiers.living[id.index()].thirst = SEVERE_THIRST;
        world.hot_member_steps = u64::MAX - 1;
        let outcome = world.apply(Command::AdvanceTo { target: 2 });
        assert_eq!(outcome.error, None);
        assert_eq!(world.hot_member_steps, u64::MAX);
        assert!(matches!(
            world.soldiers.living[id.index()].life,
            LifeState::Dead { at: 1, .. }
        ));

        let mut overflow = World::new(0);
        overflow.apply(Command::SetRegionHot { cell: 1, hot: true });
        let id = spawn(
            &mut overflow,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        overflow.hot_member_steps = u64::MAX;
        let before = overflow.soldiers.living[id.index()];
        assert_eq!(
            overflow.apply(Command::AdvanceTo { target: 1 }).error,
            Some(SimError::ArithmeticOverflow)
        );
        assert_eq!(overflow.clock, 0);
        assert_eq!(overflow.soldiers.living[id.index()], before);
    }

    #[test]
    fn automatic_events_are_globally_entity_ordered_and_dead_hot_members_are_not_work() {
        let mut world = World::new(0);
        let hot = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 1,
                    ..Position::default()
                },
                inventory: Inventory {
                    food: 1,
                    water: 1,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        let cold = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 2,
                    ..Position::default()
                },
                inventory: Inventory {
                    food: 1,
                    water: 1,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        world.apply(Command::SetRegionHot { cell: 1, hot: true });
        let events = world.apply(Command::AdvanceTo { target: 100 }).events;
        let ration_ids: Vec<_> = events
            .iter()
            .filter_map(|e| match e.event {
                Event::RationConsumed { id, .. } => Some(id),
                _ => None,
            })
            .collect();
        assert!(ration_ids.chunks(2).all(|ids| ids == [hot, cold]));
        world.soldiers.living[hot.index()].life = LifeState::Dead {
            at: 100,
            cause: DeathCause::Starvation,
        };
        world.soldiers.living[hot.index()].health = 0;
        world.soldiers.data[hot.index()].health = 0;
        let steps = world.hot_member_steps;
        world.apply(Command::AdvanceTo { target: 101 });
        assert_eq!(world.hot_member_steps, steps);
    }

    #[test]
    fn independent_transition_oracle_threshold_matrix() {
        struct Case {
            name: &'static str,
            living: LivingState,
            inventory: Inventory,
            expected: LivingState,
            expected_inventory: Inventory,
            consumed: (u128, u128),
            events: Vec<Event>,
        }
        let id = EntityId::from_parts(4, 1);
        let state = |activity, hunger, thirst, fatigue, health| LivingState {
            activity,
            hunger,
            thirst,
            fatigue,
            health,
            ..LivingState::default()
        };
        let expected =
            |activity, hunger, thirst, fatigue, sleep_debt, morale, health, life| LivingState {
                activity,
                hunger,
                thirst,
                fatigue,
                sleep_debt,
                morale,
                health,
                life,
                materialized_at: 1,
            };
        let inv = |food, water| Inventory {
            food,
            water,
            medical: 0,
        };
        let cases = vec![
            Case {
                name: "rest",
                living: state(Activity::Rest, 0, 0, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Rest, 1, 1, 0, 0, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "idle",
                living: state(Activity::Idle, 0, 0, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 1, 2, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "march",
                living: state(Activity::March, 0, 0, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::March, 2, 3, 3, 2, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "food-ration",
                living: state(Activity::Idle, 99, 0, 0, 1000),
                inventory: inv(1, 0),
                expected: expected(Activity::Idle, 0, 2, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (1, 0),
                events: vec![Event::RationConsumed {
                    id,
                    food: 1,
                    water: 0,
                    hunger_before: 100,
                    hunger_after: 0,
                    thirst_before: 2,
                    thirst_after: 2,
                }],
            },
            Case {
                name: "water-ration",
                living: state(Activity::Idle, 0, 98, 0, 1000),
                inventory: inv(0, 1),
                expected: expected(Activity::Idle, 1, 0, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 1),
                events: vec![Event::RationConsumed {
                    id,
                    food: 0,
                    water: 1,
                    hunger_before: 1,
                    hunger_after: 1,
                    thirst_before: 100,
                    thirst_after: 0,
                }],
            },
            Case {
                name: "both-rations",
                living: state(Activity::Idle, 99, 98, 0, 1000),
                inventory: inv(1, 1),
                expected: expected(Activity::Idle, 0, 0, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (1, 1),
                events: vec![Event::RationConsumed {
                    id,
                    food: 1,
                    water: 1,
                    hunger_before: 100,
                    hunger_after: 0,
                    thirst_before: 100,
                    thirst_after: 0,
                }],
            },
            Case {
                name: "hunger-before",
                living: state(Activity::Idle, 798, 0, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 799, 2, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "hunger-at",
                living: state(Activity::Idle, 799, 0, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 800, 2, 1, 1, 999, 996, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![Event::LivingDeteriorated {
                    id,
                    morale_before: 1000,
                    morale_after: 999,
                    health_before: 1000,
                    health_after: 996,
                }],
            },
            Case {
                name: "thirst-before",
                living: state(Activity::Idle, 0, 797, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 1, 799, 1, 1, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "thirst-at",
                living: state(Activity::Idle, 0, 798, 0, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 1, 800, 1, 1, 999, 990, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![Event::LivingDeteriorated {
                    id,
                    morale_before: 1000,
                    morale_after: 999,
                    health_before: 1000,
                    health_after: 990,
                }],
            },
            Case {
                name: "forced-before",
                living: state(Activity::March, 0, 0, 896, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::March, 2, 3, 899, 2, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![],
            },
            Case {
                name: "forced-at",
                living: state(Activity::March, 0, 0, 897, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 2, 3, 900, 2, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![Event::ActivityChanged {
                    id,
                    before: Activity::March,
                    after: Activity::Idle,
                    forced: true,
                }],
            },
            Case {
                name: "forced-after",
                living: state(Activity::March, 0, 0, 898, 1000),
                inventory: inv(0, 0),
                expected: expected(Activity::Idle, 2, 3, 901, 2, 1000, 1000, LifeState::Alive),
                expected_inventory: inv(0, 0),
                consumed: (0, 0),
                events: vec![Event::ActivityChanged {
                    id,
                    before: Activity::March,
                    after: Activity::Idle,
                    forced: true,
                }],
            },
            Case {
                name: "starvation-death",
                living: state(Activity::Idle, 799, 0, 0, 4),
                inventory: inv(0, 1),
                expected: expected(
                    Activity::Idle,
                    800,
                    2,
                    1,
                    1,
                    999,
                    0,
                    LifeState::Dead {
                        at: 1,
                        cause: DeathCause::Starvation,
                    },
                ),
                expected_inventory: inv(0, 1),
                consumed: (0, 0),
                events: vec![
                    Event::LivingDeteriorated {
                        id,
                        morale_before: 1000,
                        morale_after: 999,
                        health_before: 4,
                        health_after: 0,
                    },
                    Event::SoldierDied {
                        id,
                        cause: DeathCause::Starvation,
                        health_before: 4,
                    },
                ],
            },
            Case {
                name: "dehydration-death",
                living: state(Activity::Idle, 0, 798, 0, 10),
                inventory: inv(1, 0),
                expected: expected(
                    Activity::Idle,
                    1,
                    800,
                    1,
                    1,
                    999,
                    0,
                    LifeState::Dead {
                        at: 1,
                        cause: DeathCause::Dehydration,
                    },
                ),
                expected_inventory: inv(1, 0),
                consumed: (0, 0),
                events: vec![
                    Event::LivingDeteriorated {
                        id,
                        morale_before: 1000,
                        morale_after: 999,
                        health_before: 10,
                        health_after: 0,
                    },
                    Event::SoldierDied {
                        id,
                        cause: DeathCause::Dehydration,
                        health_before: 10,
                    },
                ],
            },
        ];
        assert_eq!(cases.len(), 15);
        for case in cases {
            let actual = World::transition_second(case.living, case.inventory, id, 1).unwrap();
            assert_eq!(actual.living, case.expected, "{} living", case.name);
            assert_eq!(
                actual.inventory, case.expected_inventory,
                "{} inventory",
                case.name
            );
            assert_eq!(
                (actual.consumed_food, actual.consumed_water),
                case.consumed,
                "{} ledger",
                case.name
            );
            assert_eq!(
                actual.events.iter().map(|e| e.event).collect::<Vec<_>>(),
                case.events,
                "{} events",
                case.name
            );
        }
    }

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

    fn overwrite<const N: usize>(bytes: &mut [u8], at: usize, value: [u8; N]) {
        bytes[at..at + N].copy_from_slice(&value);
    }

    #[test]
    fn v6_byte_corruption_matrix_rejects_every_living_class() {
        // Offsets are named from the canonical v6 writer, not found by matching
        // values (which would make fixtures ambiguous when fields are zero).
        const VERSION: usize = 0;
        const CLOCK: usize = 4;
        const SOURCED_FOOD: usize = 36;
        const SOURCED_WATER: usize = 52;
        const CONSUMED_FOOD: usize = 68;
        const CONSUMED_WATER: usize = 84;
        const LOST_FOOD: usize = 100;
        const LOST_WATER: usize = 116;
        const SOLDIER_ALIVE_TAG: usize = 156;
        const ROLE_TAG: usize = 172;
        const SPEC_HEALTH: usize = 174;
        const HUNGER: usize = 192;
        const THIRST: usize = 196;
        const FATIGUE: usize = 200;
        const SLEEP_DEBT: usize = 204;
        const MORALE: usize = 208;
        const LIVING_HEALTH: usize = 210;
        const ACTIVITY_TAG: usize = 212;
        const LIFE_TAG: usize = 213;
        const MATERIALIZED_ALIVE: usize = 214;

        let mut cold = World::new(7);
        let _ = spawn(
            &mut cold,
            SoldierSpec {
                inventory: Inventory {
                    food: 2,
                    water: 2,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        let base = cold.snapshot();
        assert_eq!(
            base.len(),
            266,
            "fixture layout changed; update named offsets"
        );
        fn expected(name: &str) -> SimError {
            SimError::Snapshot(match name {
                "unsupported_version" => "unsupported version",
                "noncanonical_alive_boolean" => "boolean",
                "invalid_role_tag" => "role",
                "invalid_activity_tag" => "activity",
                "invalid_life_tag" => "life state",
                "hunger_range" | "thirst_range" | "fatigue_range" | "sleep_debt_range"
                | "morale_range" | "health_range" => "living range",
                "alive_zero_health" | "health_mirror" | "future_materialization" => {
                    "living timestamp"
                }
                "food_source_equation"
                | "water_source_equation"
                | "consumed_food_equation"
                | "consumed_water_equation"
                | "lost_food_equation"
                | "lost_water_equation"
                | "food_checked_sum_hazard"
                | "water_checked_sum_hazard" => "resource ledger",
                "clock_makes_due_stale" | "noncanonical_due_order" => "living due time",
                "trailing_byte" => "trailing bytes",
                "noncanonical_due_time"
                | "missing_reverse_due_coverage"
                | "hot_entity_due_entry" => "living due coverage",
                "stale_due_id" | "dead_entity_due_entry" | "duplicate_due_entity" => {
                    "living due entity"
                }
                "hot_activated_after_last" | "hot_last_not_clock" | "hot_fixed_step_mismatch" => {
                    "hot cell"
                }
                "hot_alive_stale_materialization" => "hot living materialization",
                "invalid_death_cause" => "death cause",
                "death_time_mismatch" | "dead_materialization_mismatch" | "dead_nonzero_health" => {
                    "death state"
                }
                _ => panic!("missing expected category for {name}"),
            })
        }
        let mut cases: Vec<(&str, Vec<u8>, SimError)> = Vec::new();
        macro_rules! mutated {
            ($name:expr, $offset:expr, $value:expr) => {{
                let mut b = base.clone();
                overwrite(&mut b, $offset, $value);
                cases.push(($name, b, expected($name)));
            }};
        }
        mutated!("unsupported_version", VERSION, 5_u32.to_le_bytes());
        mutated!("noncanonical_alive_boolean", SOLDIER_ALIVE_TAG, [2]);
        mutated!("invalid_role_tag", ROLE_TAG, [9]);
        mutated!("invalid_activity_tag", ACTIVITY_TAG, [9]);
        mutated!("invalid_life_tag", LIFE_TAG, [9]);
        mutated!("hunger_range", HUNGER, 1001_u32.to_le_bytes());
        mutated!("thirst_range", THIRST, 1001_u32.to_le_bytes());
        mutated!("fatigue_range", FATIGUE, 1001_u32.to_le_bytes());
        mutated!("sleep_debt_range", SLEEP_DEBT, 1001_u32.to_le_bytes());
        mutated!("morale_range", MORALE, 1001_u16.to_le_bytes());
        mutated!("health_range", LIVING_HEALTH, 1001_u16.to_le_bytes());
        mutated!("alive_zero_health", LIVING_HEALTH, 0_u16.to_le_bytes());
        mutated!("health_mirror", SPEC_HEALTH, 999_u16.to_le_bytes());
        mutated!(
            "future_materialization",
            MATERIALIZED_ALIVE,
            1_u64.to_le_bytes()
        );
        mutated!("food_source_equation", SOURCED_FOOD, 1_u128.to_le_bytes());
        mutated!("water_source_equation", SOURCED_WATER, 1_u128.to_le_bytes());
        mutated!(
            "consumed_food_equation",
            CONSUMED_FOOD,
            1_u128.to_le_bytes()
        );
        mutated!(
            "consumed_water_equation",
            CONSUMED_WATER,
            1_u128.to_le_bytes()
        );
        mutated!("lost_food_equation", LOST_FOOD, 1_u128.to_le_bytes());
        mutated!("lost_water_equation", LOST_WATER, 1_u128.to_le_bytes());
        mutated!(
            "food_checked_sum_hazard",
            CONSUMED_FOOD,
            u128::MAX.to_le_bytes()
        );
        mutated!(
            "water_checked_sum_hazard",
            CONSUMED_WATER,
            u128::MAX.to_le_bytes()
        );
        mutated!("clock_makes_due_stale", CLOCK, 100_u64.to_le_bytes());
        let mut trailing = base.clone();
        trailing.push(0);
        cases.push(("trailing_byte", trailing, expected("trailing_byte")));

        // The final cold due tuple is [bucket time, count, entity id].
        let due_at = base.len() - 20;
        let due_id = base.len() - 8;
        mutated!("noncanonical_due_time", due_at, 99_u64.to_le_bytes());
        mutated!("stale_due_id", due_id, u64::MAX.to_le_bytes());
        let mut missing_due = base[..base.len() - 20].to_vec();
        // Replace the final due-bucket count with canonical zero coverage.
        overwrite(&mut missing_due, 242, 0_u32.to_le_bytes());
        cases.push((
            "missing_reverse_due_coverage",
            missing_due,
            expected("missing_reverse_due_coverage"),
        ));

        let mut hot = World::new(7);
        hot.apply(Command::SetRegionHot { cell: 0, hot: true });
        let _ = spawn(&mut hot, SoldierSpec::default());
        let hot_base = hot.snapshot();
        assert_eq!(hot_base.len(), 274, "hot fixture layout changed");
        for (name, offset, value) in [
            ("hot_activated_after_last", 242, 1_u64),
            ("hot_last_not_clock", 250, 1_u64),
            ("hot_fixed_step_mismatch", 258, 1_u64),
        ] {
            let mut b = hot_base.clone();
            if name == "hot_last_not_clock" {
                // Isolate last-step versus clock: fixed steps remains canonical
                // for activated=0,last=1 while only clock equality is broken.
                overwrite(&mut b, offset, 1_u64.to_le_bytes());
                overwrite(&mut b, 258, 1_u64.to_le_bytes());
            } else {
                overwrite(&mut b, offset, value.to_le_bytes());
            }
            cases.push((name, b, expected(name)));
        }
        let mut hot_due = hot_base.clone();
        overwrite(&mut hot_due, 270, 1_u32.to_le_bytes());
        hot_due.extend(1_u64.to_le_bytes());
        hot_due.extend(1_u32.to_le_bytes());
        hot_due.extend(0_u64.to_le_bytes());
        cases.push((
            "hot_entity_due_entry",
            hot_due,
            expected("hot_entity_due_entry"),
        ));
        let mut hot_at_ten = hot;
        assert!(hot_at_ten
            .apply(Command::AdvanceTo { target: 10 })
            .error
            .is_none());
        let mut stale_hot_living = hot_at_ten.snapshot();
        overwrite(
            &mut stale_hot_living,
            MATERIALIZED_ALIVE,
            9_u64.to_le_bytes(),
        );
        cases.push((
            "hot_alive_stale_materialization",
            stale_hot_living,
            expected("hot_alive_stale_materialization"),
        ));

        let mut dead = World::new(7);
        let dead_id = spawn(
            &mut dead,
            SoldierSpec {
                health: 10,
                ..SoldierSpec::default()
            },
        );
        dead.soldiers.living[dead_id.index()].thirst = SEVERE_THIRST;
        dead.schedule_due(dead_id).unwrap();
        assert!(dead.apply(Command::AdvanceTo { target: 1 }).error.is_none());
        let dead_base = dead.snapshot();
        for (name, offset, value) in [
            ("invalid_death_cause", 222, 9_u64),
            ("death_time_mismatch", 214, 2_u64),
            ("dead_materialization_mismatch", 223, 2_u64),
        ] {
            let mut b = dead_base.clone();
            if name == "invalid_death_cause" {
                b[offset] = value as u8;
            } else {
                overwrite(&mut b, offset, value.to_le_bytes());
            }
            cases.push((name, b, expected(name)));
        }
        let mut dead_health = dead_base.clone();
        overwrite(&mut dead_health, SPEC_HEALTH, 1_u16.to_le_bytes());
        overwrite(&mut dead_health, LIVING_HEALTH, 1_u16.to_le_bytes());
        cases.push((
            "dead_nonzero_health",
            dead_health,
            expected("dead_nonzero_health"),
        ));

        let mut dead_due = dead_base.clone();
        let dead_due_count = dead_due.len() - 4;
        overwrite(&mut dead_due, dead_due_count, 1_u32.to_le_bytes());
        dead_due.extend(2_u64.to_le_bytes());
        dead_due.extend(1_u32.to_le_bytes());
        dead_due.extend(dead_id.raw().to_le_bytes());
        cases.push((
            "dead_entity_due_entry",
            dead_due,
            expected("dead_entity_due_entry"),
        ));

        let mut two = World::new(7);
        let first = spawn(&mut two, SoldierSpec::default());
        let second = spawn(&mut two, SoldierSpec::default());
        let two_base = two.snapshot();
        let mut duplicate = two_base.clone();
        overwrite(
            &mut duplicate,
            two_base.len() - 8,
            first.raw().to_le_bytes(),
        );
        cases.push((
            "duplicate_due_entity",
            duplicate,
            expected("duplicate_due_entity"),
        ));
        assert_ne!(first, second);

        let mut ordered = World::new(7);
        let idle = spawn(&mut ordered, SoldierSpec::default());
        let march = spawn(&mut ordered, SoldierSpec::default());
        ordered.soldiers.living[march.index()].activity = Activity::March;
        ordered.schedule_due(march).unwrap();
        assert_ne!(ordered.due_by_entity[&idle], ordered.due_by_entity[&march]);
        let ordered_base = ordered.snapshot();
        // Two one-entity buckets occupy the final 44 bytes. Make the second
        // timestamp equal the first, violating strict canonical ordering.
        let first_at = u64::from_le_bytes(
            ordered_base[ordered_base.len() - 40..ordered_base.len() - 32]
                .try_into()
                .unwrap(),
        );
        let mut unordered = ordered_base.clone();
        overwrite(
            &mut unordered,
            ordered_base.len() - 20,
            first_at.to_le_bytes(),
        );
        cases.push((
            "noncanonical_due_order",
            unordered,
            expected("noncanonical_due_order"),
        ));

        let names: Vec<_> = cases.iter().map(|(name, _, _)| *name).collect();
        eprintln!(
            "v6 byte mutation cases ({}): {}",
            names.len(),
            names.join(", ")
        );
        for (name, bytes, category) in cases {
            assert_eq!(
                World::from_snapshot(&bytes).err(),
                Some(category),
                "mutation {name}"
            );
        }
    }

    #[test]
    fn sparse_candidate_visits_ignore_population_times_segments() {
        let mut world = World::new(1);
        // Far-future ration boundaries: none may become a candidate before 49.
        for cell in 0..2_000 {
            let _ = spawn(
                &mut world,
                SoldierSpec {
                    position: Position {
                        cell,
                        ..Position::default()
                    },
                    inventory: Inventory {
                        food: 1,
                        water: 1,
                        medical: 0,
                    },
                    ..SoldierSpec::default()
                },
            );
        }
        // Dense unrelated segments must not visit the 2,000 cold records.
        for at in 1..50 {
            world.apply(Command::Schedule {
                at,
                command: ScheduledCommand::SetRegionHot {
                    cell: 10_000 + at as u32,
                    hot: true,
                },
            });
        }
        assert_eq!(world.automatic_journal_visits, 0);
        assert!(world
            .apply(Command::AdvanceTo { target: 49 })
            .error
            .is_none());
        assert_eq!(world.automatic_journal_visits, 0);
        assert_eq!(world.automatic_execution_visits, 0);
        assert_eq!(world.cold_boundaries, 0);
        // At second 50 every record is an actual ration candidate, exactly once.
        assert!(world
            .apply(Command::AdvanceTo { target: 50 })
            .error
            .is_none());
        assert_eq!(world.automatic_journal_visits, 2_000);
        assert_eq!(world.automatic_execution_visits, 2_000);
        assert_eq!(world.cold_boundaries, 2_000);

        // An unrelated later segment adds no traversal on either phase.
        world.apply(Command::Schedule {
            at: 51,
            command: ScheduledCommand::SetRegionHot {
                cell: 99_999,
                hot: true,
            },
        });
        assert!(world
            .apply(Command::AdvanceTo { target: 51 })
            .error
            .is_none());
        assert_eq!(world.automatic_journal_visits, 2_000);
        assert_eq!(world.automatic_execution_visits, 2_000);

        // A small independently due cohort increments both phases exactly once.
        for cell in 20_000..20_007 {
            let id = spawn(
                &mut world,
                SoldierSpec {
                    position: Position {
                        cell,
                        ..Position::default()
                    },
                    ..SoldierSpec::default()
                },
            );
            world.soldiers.living[id.index()].thirst = SEVERE_THIRST - 2;
            world.schedule_due(id).unwrap();
        }
        assert!(world
            .apply(Command::AdvanceTo { target: 52 })
            .error
            .is_none());
        assert_eq!(world.automatic_journal_visits, 2_007);
        assert_eq!(world.automatic_execution_visits, 2_007);
        assert_eq!(world.cold_boundaries, 2_007);
    }

    #[test]
    fn cold_analytical_path_matches_hand_authored_duration_matrix() {
        #[derive(Clone)]
        struct Case {
            name: &'static str,
            target: u64,
            living: LivingState,
            inventory: Inventory,
            expected: LivingState,
            expected_inventory: Inventory,
            expected_events: Vec<TimedEvent>,
            expected_due: Option<u64>,
            expected_consumed: (u128, u128),
            expected_cold_work: u64,
        }

        let id = EntityId::from_parts(0, 0);
        let living = |activity, hunger, thirst, fatigue, health| LivingState {
            activity,
            hunger,
            thirst,
            fatigue,
            health,
            ..LivingState::default()
        };
        let inventory = |food, water| Inventory {
            food,
            water,
            medical: 7,
        };
        let advanced = |to| TimedEvent {
            at: to,
            event: Event::TimeAdvanced {
                from: 0,
                to,
                hot_cells_stepped: 0,
                fixed_steps_per_hot_cell: 0,
            },
        };
        let deterioration = |at, morale_before, health_before, health_after| TimedEvent {
            at,
            event: Event::LivingDeteriorated {
                id,
                morale_before,
                morale_after: morale_before - 1,
                health_before,
                health_after,
            },
        };
        let cases = vec![
            Case {
                name: "duration-zero-rest-both-present",
                target: 0,
                living: living(Activity::Rest, 0, 0, 6, 1000),
                inventory: inventory(2, 2),
                expected: living(Activity::Rest, 0, 0, 6, 1000),
                expected_inventory: inventory(2, 2),
                expected_events: vec![],
                expected_due: Some(100),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "ration-before-idle-both-present",
                target: 1,
                living: living(Activity::Idle, 98, 97, 0, 1000),
                inventory: inventory(1, 1),
                expected: LivingState {
                    hunger: 99,
                    thirst: 99,
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 1),
                expected_events: vec![advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "ration-at-idle-both-present",
                target: 1,
                living: living(Activity::Idle, 99, 98, 0, 1000),
                inventory: inventory(1, 1),
                expected: LivingState {
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 1,
                        event: Event::RationConsumed {
                            id,
                            food: 1,
                            water: 1,
                            hunger_before: 100,
                            hunger_after: 0,
                            thirst_before: 100,
                            thirst_after: 0,
                        },
                    },
                    advanced(1),
                ],
                expected_due: Some(401),
                expected_consumed: (1, 1),
                expected_cold_work: 1,
            },
            Case {
                name: "ration-after-idle-food-only",
                target: 1,
                living: living(Activity::Idle, 100, 0, 0, 1000),
                inventory: inventory(1, 0),
                expected: LivingState {
                    hunger: 1,
                    thirst: 2,
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 1,
                        event: Event::RationConsumed {
                            id,
                            food: 1,
                            water: 0,
                            hunger_before: 101,
                            hunger_after: 1,
                            thirst_before: 2,
                            thirst_after: 2,
                        },
                    },
                    advanced(1),
                ],
                expected_due: Some(400),
                expected_consumed: (1, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "ration-at-idle-water-only",
                target: 1,
                living: living(Activity::Idle, 0, 98, 0, 1000),
                inventory: inventory(0, 1),
                expected: LivingState {
                    hunger: 1,
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 1,
                        event: Event::RationConsumed {
                            id,
                            food: 0,
                            water: 1,
                            hunger_before: 1,
                            hunger_after: 1,
                            thirst_before: 100,
                            thirst_after: 0,
                        },
                    },
                    advanced(1),
                ],
                expected_due: Some(401),
                expected_consumed: (0, 1),
                expected_cold_work: 1,
            },
            Case {
                name: "ration-duration-boundary-minus-one",
                target: 49,
                living: living(Activity::Idle, 49, 0, 0, 1000),
                inventory: inventory(1, 1),
                expected: LivingState {
                    hunger: 98,
                    thirst: 98,
                    fatigue: 49,
                    sleep_debt: 49,
                    materialized_at: 49,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 1),
                expected_events: vec![advanced(49)],
                expected_due: Some(50),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "ration-duration-boundary",
                target: 50,
                living: living(Activity::Idle, 49, 0, 0, 1000),
                inventory: inventory(1, 1),
                expected: LivingState {
                    hunger: 99,
                    fatigue: 50,
                    sleep_debt: 50,
                    materialized_at: 50,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 50,
                        event: Event::RationConsumed {
                            id,
                            food: 0,
                            water: 1,
                            hunger_before: 99,
                            hunger_after: 99,
                            thirst_before: 100,
                            thirst_after: 0,
                        },
                    },
                    advanced(50),
                ],
                expected_due: Some(51),
                expected_consumed: (0, 1),
                expected_cold_work: 1,
            },
            Case {
                name: "ration-duration-boundary-plus-one-multi-boundary",
                target: 51,
                living: living(Activity::Idle, 49, 0, 0, 1000),
                inventory: inventory(1, 1),
                expected: LivingState {
                    thirst: 2,
                    fatigue: 51,
                    sleep_debt: 51,
                    materialized_at: 51,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 50,
                        event: Event::RationConsumed {
                            id,
                            food: 0,
                            water: 1,
                            hunger_before: 99,
                            hunger_after: 99,
                            thirst_before: 100,
                            thirst_after: 0,
                        },
                    },
                    TimedEvent {
                        at: 51,
                        event: Event::RationConsumed {
                            id,
                            food: 1,
                            water: 0,
                            hunger_before: 100,
                            hunger_after: 0,
                            thirst_before: 2,
                            thirst_after: 2,
                        },
                    },
                    advanced(51),
                ],
                expected_due: Some(450),
                expected_consumed: (1, 1),
                expected_cold_work: 2,
            },
            Case {
                name: "severe-hunger-before-rest-food-exhausted",
                target: 1,
                living: living(Activity::Rest, 798, 0, 2, 1000),
                inventory: inventory(0, 1),
                expected: LivingState {
                    hunger: 799,
                    thirst: 1,
                    fatigue: 0,
                    materialized_at: 1,
                    activity: Activity::Rest,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 1),
                expected_events: vec![advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "severe-hunger-at-idle-food-exhausted",
                target: 1,
                living: living(Activity::Idle, 799, 0, 0, 1000),
                inventory: inventory(0, 1),
                expected: LivingState {
                    hunger: 800,
                    thirst: 2,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 996,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 1),
                expected_events: vec![deterioration(1, 1000, 1000, 996), advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "severe-hunger-after-idle-food-exhausted",
                target: 1,
                living: living(Activity::Idle, 800, 0, 0, 1000),
                inventory: inventory(0, 1),
                expected: LivingState {
                    hunger: 801,
                    thirst: 2,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 996,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 1),
                expected_events: vec![deterioration(1, 1000, 1000, 996), advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "severe-thirst-before-idle-water-exhausted",
                target: 1,
                living: living(Activity::Idle, 0, 797, 0, 1000),
                inventory: inventory(1, 0),
                expected: LivingState {
                    hunger: 1,
                    thirst: 799,
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 0),
                expected_events: vec![advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "severe-thirst-at-idle-water-exhausted",
                target: 1,
                living: living(Activity::Idle, 0, 798, 0, 1000),
                inventory: inventory(1, 0),
                expected: LivingState {
                    hunger: 1,
                    thirst: 800,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 990,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 0),
                expected_events: vec![deterioration(1, 1000, 1000, 990), advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "severe-thirst-after-idle-water-exhausted",
                target: 1,
                living: living(Activity::Idle, 0, 799, 0, 1000),
                inventory: inventory(1, 0),
                expected: LivingState {
                    hunger: 1,
                    thirst: 801,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 990,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 0),
                expected_events: vec![deterioration(1, 1000, 1000, 990), advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "forced-idle-before-march-both-exhausted",
                target: 1,
                living: living(Activity::March, 0, 0, 896, 1000),
                inventory: inventory(0, 0),
                expected: LivingState {
                    hunger: 2,
                    thirst: 3,
                    fatigue: 899,
                    sleep_debt: 2,
                    activity: Activity::March,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![advanced(1)],
                expected_due: Some(2),
                expected_consumed: (0, 0),
                expected_cold_work: 0,
            },
            Case {
                name: "forced-idle-at-march-both-exhausted",
                target: 1,
                living: living(Activity::March, 0, 0, 897, 1000),
                inventory: inventory(0, 0),
                expected: LivingState {
                    hunger: 2,
                    thirst: 3,
                    fatigue: 900,
                    sleep_debt: 2,
                    activity: Activity::Idle,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 1,
                        event: Event::ActivityChanged {
                            id,
                            before: Activity::March,
                            after: Activity::Idle,
                            forced: true,
                        },
                    },
                    advanced(1),
                ],
                expected_due: Some(400),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "forced-idle-after-march-both-exhausted",
                target: 1,
                living: living(Activity::March, 0, 0, 898, 1000),
                inventory: inventory(0, 0),
                expected: LivingState {
                    hunger: 2,
                    thirst: 3,
                    fatigue: 901,
                    sleep_debt: 2,
                    activity: Activity::Idle,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    TimedEvent {
                        at: 1,
                        event: Event::ActivityChanged {
                            id,
                            before: Activity::March,
                            after: Activity::Idle,
                            forced: true,
                        },
                    },
                    advanced(1),
                ],
                expected_due: Some(400),
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
            Case {
                name: "boundary-plus-one-starvation-death",
                target: 2,
                living: living(Activity::Idle, 799, 0, 0, 8),
                inventory: inventory(0, 1),
                expected: LivingState {
                    hunger: 801,
                    thirst: 4,
                    fatigue: 2,
                    sleep_debt: 2,
                    morale: 998,
                    health: 0,
                    life: LifeState::Dead {
                        at: 2,
                        cause: DeathCause::Starvation,
                    },
                    materialized_at: 2,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 1),
                expected_events: vec![
                    deterioration(1, 1000, 8, 4),
                    deterioration(2, 999, 4, 0),
                    TimedEvent {
                        at: 2,
                        event: Event::SoldierDied {
                            id,
                            cause: DeathCause::Starvation,
                            health_before: 4,
                        },
                    },
                    advanced(2),
                ],
                expected_due: None,
                expected_consumed: (0, 0),
                expected_cold_work: 2,
            },
            Case {
                name: "multi-boundary-both-exhausted",
                target: 3,
                living: living(Activity::Idle, 799, 798, 0, 40),
                inventory: inventory(0, 0),
                expected: LivingState {
                    hunger: 802,
                    thirst: 804,
                    fatigue: 3,
                    sleep_debt: 3,
                    morale: 997,
                    health: 10,
                    materialized_at: 3,
                    ..LivingState::default()
                },
                expected_inventory: inventory(0, 0),
                expected_events: vec![
                    deterioration(1, 1000, 40, 30),
                    deterioration(2, 999, 30, 20),
                    deterioration(3, 998, 20, 10),
                    advanced(3),
                ],
                expected_due: Some(4),
                expected_consumed: (0, 0),
                expected_cold_work: 3,
            },
            Case {
                name: "huge-time-terminal-dehydration-death",
                target: u64::MAX,
                living: living(Activity::Idle, 0, 798, 0, 10),
                inventory: inventory(1, 0),
                expected: LivingState {
                    hunger: 1,
                    thirst: 800,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 0,
                    life: LifeState::Dead {
                        at: 1,
                        cause: DeathCause::Dehydration,
                    },
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: inventory(1, 0),
                expected_events: vec![
                    deterioration(1, 1000, 10, 0),
                    TimedEvent {
                        at: 1,
                        event: Event::SoldierDied {
                            id,
                            cause: DeathCause::Dehydration,
                            health_before: 10,
                        },
                    },
                    advanced(u64::MAX),
                ],
                expected_due: None,
                expected_consumed: (0, 0),
                expected_cold_work: 1,
            },
        ];
        assert_eq!(cases.len(), 20);

        for case in cases {
            let mut world = World::new(0xC01D);
            let got = spawn(
                &mut world,
                SoldierSpec {
                    faction: 9,
                    position: Position {
                        x_mm: -12,
                        y_mm: 34,
                        cell: 56,
                    },
                    squad: None,
                    role: Role::Logistics,
                    rank: 4,
                    health: case.living.health,
                    ammunition: 23,
                    inventory: case.inventory,
                },
            );
            assert_eq!(got, id, "{}", case.name);
            world.soldiers.living[0] = case.living;
            world.soldiers.data[0].health = case.living.health;
            world.schedule_due(id).unwrap();

            let outcome = world.apply(Command::AdvanceTo {
                target: case.target,
            });
            assert_eq!(outcome.error, None, "{}", case.name);
            assert_eq!(outcome.events, case.expected_events, "{}", case.name);
            let expected_persistent = if case.expected_cold_work == 0 {
                case.living
            } else {
                case.expected
            };
            assert_eq!(
                world.soldiers.living[0], expected_persistent,
                "{}",
                case.name
            );
            assert_eq!(
                world.soldiers.data[0].inventory, case.expected_inventory,
                "{}",
                case.name
            );
            let public = world.soldier(id).unwrap();
            assert_eq!(
                public,
                Soldier {
                    id,
                    faction: 9,
                    position: Position {
                        x_mm: -12,
                        y_mm: 34,
                        cell: 56
                    },
                    squad: None,
                    role: Role::Logistics,
                    rank: 4,
                    health: case.expected.health,
                    needs: Needs {
                        fatigue: case.expected.fatigue,
                        hunger: case.expected.hunger,
                        thirst: case.expected.thirst,
                        sleep_debt: case.expected.sleep_debt
                    },
                    ammunition: 23,
                    inventory: case.expected_inventory,
                    living: case.expected,
                },
                "{}",
                case.name
            );
            assert_eq!(
                world.resource_totals(),
                ResourceTotals {
                    ammunition: 23,
                    stockpile_supplies: 0,
                    carried_food: u128::from(case.expected_inventory.food),
                    carried_water: u128::from(case.expected_inventory.water),
                    carried_medical: 7,
                    sourced_food: u128::from(case.inventory.food),
                    sourced_water: u128::from(case.inventory.water),
                    consumed_food: case.expected_consumed.0,
                    consumed_water: case.expected_consumed.1,
                    lost_food: 0,
                    lost_water: 0,
                },
                "{}",
                case.name
            );
            assert_eq!(
                world.living_work_counters(),
                (case.expected_cold_work, 0),
                "{}",
                case.name
            );
            assert!(world.hot_cells.is_empty(), "{}", case.name);
            assert_eq!(
                world.due_by_entity.get(&id).copied(),
                case.expected_due,
                "{}",
                case.name
            );
            assert_eq!(
                world.living_due.values().map(BTreeSet::len).sum::<usize>(),
                usize::from(case.expected_due.is_some()),
                "{}",
                case.name
            );
            if let Some(at) = case.expected_due {
                assert_eq!(
                    world.living_due.get(&at),
                    Some(&BTreeSet::from([id])),
                    "{}",
                    case.name
                );
            }
            if matches!(case.expected.life, LifeState::Dead { .. }) {
                let snapshot = world.snapshot();
                let counters = world.living_work_counters();
                let repeated = world.apply(Command::AdvanceTo {
                    target: case.target,
                });
                assert_eq!(repeated.error, None, "{}", case.name);
                assert!(repeated.events.is_empty(), "{}", case.name);
                assert_eq!(world.snapshot(), snapshot, "{}", case.name);
                assert_eq!(world.living_work_counters(), counters, "{}", case.name);
            }
        }
    }

    #[test]
    fn cold_and_hot_paths_each_match_hand_authored_boundary_fixtures() {
        #[derive(Clone, Copy)]
        struct Fixture<'a> {
            name: &'static str,
            living: LivingState,
            inventory: Inventory,
            expected: LivingState,
            expected_inventory: Inventory,
            expected_events: &'a [Event],
        }
        let id = EntityId::from_parts(0, 0);
        let ration_events = [Event::RationConsumed {
            id,
            food: 1,
            water: 1,
            hunger_before: 100,
            hunger_after: 0,
            thirst_before: 100,
            thirst_after: 0,
        }];
        let forced_events = [Event::ActivityChanged {
            id,
            before: Activity::March,
            after: Activity::Idle,
            forced: true,
        }];
        let deterioration_events = [Event::LivingDeteriorated {
            id,
            morale_before: 1000,
            morale_after: 999,
            health_before: 1000,
            health_after: 990,
        }];
        let death_events = [
            Event::LivingDeteriorated {
                id,
                morale_before: 1000,
                morale_after: 999,
                health_before: 10,
                health_after: 0,
            },
            Event::SoldierDied {
                id,
                cause: DeathCause::Dehydration,
                health_before: 10,
            },
        ];
        let fixtures = [
            Fixture {
                name: "ration-at",
                living: LivingState {
                    hunger: 99,
                    thirst: 98,
                    ..LivingState::default()
                },
                inventory: Inventory {
                    food: 1,
                    water: 1,
                    medical: 3,
                },
                expected: LivingState {
                    fatigue: 1,
                    sleep_debt: 1,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 3,
                },
                expected_events: &ration_events,
            },
            Fixture {
                name: "forced-idle-at",
                living: LivingState {
                    fatigue: 897,
                    activity: Activity::March,
                    ..LivingState::default()
                },
                inventory: Inventory::default(),
                expected: LivingState {
                    hunger: 2,
                    thirst: 3,
                    fatigue: 900,
                    sleep_debt: 2,
                    activity: Activity::Idle,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: Inventory::default(),
                expected_events: &forced_events,
            },
            Fixture {
                name: "deterioration-at",
                living: LivingState {
                    thirst: 798,
                    ..LivingState::default()
                },
                inventory: Inventory::default(),
                expected: LivingState {
                    hunger: 1,
                    thirst: 800,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 990,
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: Inventory::default(),
                expected_events: &deterioration_events,
            },
            Fixture {
                name: "death-at",
                living: LivingState {
                    thirst: 800,
                    health: 10,
                    ..LivingState::default()
                },
                inventory: Inventory::default(),
                expected: LivingState {
                    hunger: 1,
                    thirst: 802,
                    fatigue: 1,
                    sleep_debt: 1,
                    morale: 999,
                    health: 0,
                    life: LifeState::Dead {
                        at: 1,
                        cause: DeathCause::Dehydration,
                    },
                    materialized_at: 1,
                    ..LivingState::default()
                },
                expected_inventory: Inventory::default(),
                expected_events: &death_events,
            },
        ];
        for hot in [false, true] {
            for fixture in fixtures {
                let mut world = World::new(0);
                if hot {
                    world.apply(Command::SetRegionHot { cell: 1, hot: true });
                }
                let got = spawn(
                    &mut world,
                    SoldierSpec {
                        position: Position {
                            cell: 1,
                            ..Position::default()
                        },
                        health: fixture.living.health,
                        inventory: fixture.inventory,
                        ..SoldierSpec::default()
                    },
                );
                assert_eq!(got, id);
                world.soldiers.living[0] = fixture.living;
                world.soldiers.data[0].health = fixture.living.health;
                if !hot {
                    world.schedule_due(got).unwrap();
                }
                let outcome = world.apply(Command::AdvanceTo { target: 1 });
                assert_eq!(outcome.error, None, "{} hot={hot}", fixture.name);
                assert_eq!(
                    world.soldiers.living[0], fixture.expected,
                    "{} hot={hot}",
                    fixture.name
                );
                assert_eq!(
                    world.soldiers.data[0].inventory, fixture.expected_inventory,
                    "{} hot={hot}",
                    fixture.name
                );
                let mut expected_events: Vec<_> = fixture
                    .expected_events
                    .iter()
                    .cloned()
                    .map(|event| TimedEvent { at: 1, event })
                    .collect();
                expected_events.push(TimedEvent {
                    at: 1,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 1,
                        hot_cells_stepped: u64::from(hot),
                        fixed_steps_per_hot_cell: u64::from(hot),
                    },
                });
                assert_eq!(
                    outcome.events, expected_events,
                    "{} hot={hot}",
                    fixture.name
                );
                let public = world.soldier(got).unwrap();
                assert_eq!(public.health, fixture.expected.health);
                assert_eq!(public.living, fixture.expected);
                assert_eq!(public.inventory, fixture.expected_inventory);
                assert_eq!(
                    public.position,
                    Position {
                        cell: 1,
                        ..Position::default()
                    }
                );
                assert_eq!(public.faction, 0);
                assert_eq!(public.squad, None);
                assert_eq!(public.role, Role::Rifle);
                assert_eq!(public.rank, 0);
                assert_eq!(public.ammunition, 0);
                let consumed = u128::from(fixture.name == "ration-at");
                assert_eq!(
                    world.resource_totals(),
                    ResourceTotals {
                        ammunition: 0,
                        stockpile_supplies: 0,
                        carried_food: u128::from(fixture.expected_inventory.food),
                        carried_water: u128::from(fixture.expected_inventory.water),
                        carried_medical: u128::from(fixture.expected_inventory.medical),
                        sourced_food: u128::from(fixture.inventory.food),
                        sourced_water: u128::from(fixture.inventory.water),
                        consumed_food: consumed,
                        consumed_water: consumed,
                        lost_food: 0,
                        lost_water: 0,
                    }
                );
                assert_eq!(
                    world.living_work_counters(),
                    if hot { (0, 1) } else { (1, 0) }
                );
            }
        }
    }

    #[test]
    fn repeated_due_churn_keeps_exact_canonical_entries() {
        let mut world = World::new(5);
        let mut stale = Vec::new();
        for cycle in 0..64_u32 {
            let id = spawn(
                &mut world,
                SoldierSpec {
                    position: Position {
                        cell: cycle % 3,
                        ..Position::default()
                    },
                    inventory: Inventory {
                        food: 2,
                        water: 2,
                        medical: 0,
                    },
                    ..SoldierSpec::default()
                },
            );
            let due = world.canonical_due(id).unwrap().unwrap();
            assert_eq!(world.due_by_entity.get(&id), Some(&due));
            assert_eq!(
                world
                    .living_due
                    .get(&due)
                    .unwrap()
                    .iter()
                    .filter(|x| **x == id)
                    .count(),
                1
            );
            world.apply(Command::SetRegionHot {
                cell: cycle % 3,
                hot: true,
            });
            assert!(!world.due_by_entity.contains_key(&id));
            world.apply(Command::SetRegionHot {
                cell: cycle % 3,
                hot: false,
            });
            assert_eq!(
                world.due_by_entity.get(&id),
                world.canonical_due(id).unwrap().as_ref()
            );
            world.apply(Command::DespawnSoldier { id });
            assert!(!world.due_by_entity.contains_key(&id));
            assert!(world.living_due.values().all(|ids| !ids.contains(&id)));
            stale.push((id, due));
        }
        let replacement = spawn(&mut world, SoldierSpec::default());
        for (id, at) in stale {
            world.living_due.entry(at).or_default().insert(id);
            world.due_by_entity.insert(id, at);
        }
        let before = world.soldiers.living[replacement.index()];
        assert!(world
            .apply(Command::AdvanceTo { target: 100 })
            .error
            .is_none());
        assert_eq!(
            world.soldiers.living[replacement.index()].materialized_at,
            before.materialized_at
        );
        assert!(world
            .due_by_entity
            .keys()
            .all(|id| world.soldiers.valid(*id)));
    }

    #[test]
    fn consumed_food_and_fixed_step_defenses_roll_back_complete_state() {
        // Deliberately impossible defensive ledger state: conservation makes a
        // consumed-total overflow unreachable from a valid v6 snapshot, but a
        // corrupted in-memory state must still fail without partial mutation.
        let mut food = World::new(0);
        let id = spawn(
            &mut food,
            SoldierSpec {
                inventory: Inventory {
                    food: 2,
                    water: 0,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        food.soldiers.living[id.index()].hunger = RATION_THRESHOLD - 1;
        food.schedule_due(id).unwrap();
        food.consumed_food = u128::MAX - 1;
        food.sourced_food = u128::MAX;
        let before = food.snapshot();
        assert!(World::from_snapshot(&before).is_err());
        let outcome = food.apply(Command::AdvanceTo { target: 101 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(food.snapshot(), before);

        // Deliberately impossible defensive state: fixed_steps cannot overflow
        // from a valid v6 snapshot because fixed_steps == last - activated.
        let mut impossible = World::new(0);
        impossible.hot_cells.insert(
            1,
            HotCellState {
                activated_at: 0,
                last_stepped_at: 0,
                fixed_steps: u64::MAX,
            },
        );
        let before = impossible.snapshot();
        assert!(World::from_snapshot(&before).is_err());
        let outcome = impossible.apply(Command::AdvanceTo { target: 1 });
        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(impossible.snapshot(), before);
    }

    #[test]
    fn combined_membership_reuse_death_restore_and_replay_fixture() {
        let mut world = World::new(77);
        world.apply(Command::SetRegionHot {
            cell: 10,
            hot: true,
        });
        world.apply(Command::SetRegionHot {
            cell: 20,
            hot: true,
        });
        let dying = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 10,
                    ..Position::default()
                },
                health: 10,
                ..SoldierSpec::default()
            },
        );
        let removed = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 10,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        let cold = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 30,
                    ..Position::default()
                },
                ..SoldierSpec::default()
            },
        );
        world.soldiers.living[dying.index()].thirst = SEVERE_THIRST;
        world.apply(Command::DespawnSoldier { id: removed });
        let replacement = spawn(
            &mut world,
            SoldierSpec {
                position: Position {
                    cell: 20,
                    ..Position::default()
                },
                inventory: Inventory {
                    food: 1,
                    water: 1,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        assert_eq!(replacement.index(), removed.index());
        assert_ne!(replacement, removed);
        let checkpoint = world.snapshot();
        let mut restored = World::from_snapshot(&checkpoint).unwrap();
        assert_eq!(restored.cell_members[&10], BTreeSet::from([dying]));
        assert_eq!(restored.cell_members[&20], BTreeSet::from([replacement]));
        assert_eq!(restored.cell_members[&30], BTreeSet::from([cold]));
        assert!(restored
            .cell_members
            .values()
            .all(|members| !members.contains(&removed)));

        let expected = vec![
            TimedEvent {
                at: 1,
                event: Event::LivingDeteriorated {
                    id: dying,
                    morale_before: 1000,
                    morale_after: 999,
                    health_before: 10,
                    health_after: 0,
                },
            },
            TimedEvent {
                at: 1,
                event: Event::SoldierDied {
                    id: dying,
                    cause: DeathCause::Dehydration,
                    health_before: 10,
                },
            },
            TimedEvent {
                at: 1,
                event: Event::TimeAdvanced {
                    from: 0,
                    to: 1,
                    hot_cells_stepped: 2,
                    fixed_steps_per_hot_cell: 1,
                },
            },
        ];
        for candidate in [&mut world, &mut restored] {
            let outcome = candidate.apply(Command::AdvanceTo { target: 1 });
            assert_eq!(outcome.error, None);
            assert_eq!(outcome.events, expected);
            assert_eq!(candidate.hot_member_steps, 2);
            assert_eq!(candidate.cold_boundaries, 0);
            assert!(candidate.soldier(removed).is_none());
            assert_eq!(
                candidate
                    .soldier(replacement)
                    .unwrap()
                    .living
                    .materialized_at,
                1
            );
            assert_eq!(candidate.soldiers.living[cold.index()].materialized_at, 0);
            assert!(matches!(
                candidate.soldier(dying).unwrap().living.life,
                LifeState::Dead {
                    at: 1,
                    cause: DeathCause::Dehydration
                }
            ));
            assert_eq!(candidate.hot_cells[&10].fixed_steps, 1);
            assert_eq!(candidate.hot_cells[&20].fixed_steps, 1);
        }
        assert_eq!(world.snapshot(), restored.snapshot());
        assert_eq!(world.state_digest(), restored.state_digest());
        assert_eq!(world.state_digest(), 0x4efc_41f8_9a7f_a833);

        let later = vec![TimedEvent {
            at: 3,
            event: Event::TimeAdvanced {
                from: 1,
                to: 3,
                hot_cells_stepped: 2,
                fixed_steps_per_hot_cell: 2,
            },
        }];
        for candidate in [&mut world, &mut restored] {
            let outcome = candidate.apply(Command::AdvanceTo { target: 3 });
            assert_eq!(outcome.error, None);
            assert_eq!(outcome.events, later);
            assert_eq!(candidate.hot_member_steps, 4);
            assert_eq!(candidate.hot_cells[&10].fixed_steps, 3);
            assert_eq!(candidate.hot_cells[&20].fixed_steps, 3);
            assert_eq!(
                candidate.soldiers.living[replacement.index()].materialized_at,
                3
            );
            assert_eq!(candidate.soldiers.living[dying.index()].materialized_at, 1);
            assert_eq!(candidate.soldiers.living[cold.index()].materialized_at, 0);
            assert!(candidate
                .cell_members
                .values()
                .all(|members| !members.contains(&removed)));
        }
        assert_eq!(world.snapshot(), restored.snapshot());
        assert_eq!(world.state_digest(), restored.state_digest());
    }
}
