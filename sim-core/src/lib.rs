//! Deterministic authoritative M0 simulation kernel.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

pub const SNAPSHOT_VERSION: u32 = 7;
pub const NEED_MAX: u32 = 1_000;
pub const RATION_THRESHOLD: u32 = 100;
pub const FOOD_RATION: u32 = 1;
pub const WATER_RATION: u32 = 1;
pub const SEVERE_HUNGER: u32 = 800;
pub const SEVERE_THIRST: u32 = 800;
pub const FORCED_IDLE_FATIGUE: u32 = 900;
pub const BLOOD_MAX: u32 = 5_000;
pub const INCAPACITATED_SHOCK: u32 = 700;
pub const HEMOSTATIC_DURATION: u64 = 10;
pub const SHOCK_TREATMENT_DURATION: u64 = 15;
pub const HEMOSTATIC_COST: u32 = 1;
pub const SHOCK_TREATMENT_COST: u32 = 2;
/// Recovery is evaluated on exact five-second boundaries.
pub const RECOVERY_INTERVAL: u64 = 5;
pub const RECOVERY_BLOOD_PER_TICK: u32 = 100;
pub const RECOVERY_SHOCK_PER_TICK: u32 = 50;
pub const RECOVERY_HEALTH_PER_TICK: u16 = 25;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct WoundId(pub u64);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct TreatmentId(pub u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WoundSpec {
    pub trauma: u16,
    pub bleeding_per_second: u16,
    pub shock: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Wound {
    pub id: WoundId,
    pub patient: EntityId,
    pub created_at: u64,
    pub spec: WoundSpec,
    pub controlled: bool,
    pub healed: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CasualtyState {
    pub blood: u32,
    pub shock: u32,
    /// Canonical tenths-of-a-shock-unit carried across materialization boundaries.
    pub shock_remainder: u8,
    pub incapacitated: bool,
    pub recovering: bool,
    /// The one canonical recovery boundary. `None` unless `recovering`.
    pub recovery_next_at: Option<u64>,
    pub materialized_at: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreatmentKind {
    Hemostatic,
    Shock,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptionReason {
    Explicit,
    MedicDied,
    PatientDied,
    MedicRemoved,
    PatientRemoved,
    Ineligible,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreatmentStatus {
    Active,
    Completed { at: u64 },
    Interrupted { at: u64, reason: InterruptionReason },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Treatment {
    pub id: TreatmentId,
    pub medic: EntityId,
    pub patient: EntityId,
    pub wound: Option<WoundId>,
    pub kind: TreatmentKind,
    pub started_at: u64,
    pub completes_at: u64,
    pub consumed: u32,
    pub status: TreatmentStatus,
}

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
    ImmediateTrauma,
    Hemorrhage,
    TraumaticShock,
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
    InflictWound {
        patient: EntityId,
        wound: WoundSpec,
    },
    StartTreatment {
        medic: EntityId,
        patient: EntityId,
        wound: Option<WoundId>,
        kind: TreatmentKind,
    },
    RequestTreatment {
        patient: EntityId,
        wound: Option<WoundId>,
        kind: TreatmentKind,
    },
    InterruptTreatment {
        id: TreatmentId,
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
    WoundInflicted {
        id: WoundId,
        patient: EntityId,
        wound: WoundSpec,
    },
    TreatmentStarted {
        id: TreatmentId,
        medic: EntityId,
        patient: EntityId,
        wound: Option<WoundId>,
        kind: TreatmentKind,
        completes_at: u64,
        consumed: u32,
    },
    TreatmentCompleted {
        id: TreatmentId,
        medic: EntityId,
        patient: EntityId,
        kind: TreatmentKind,
    },
    TreatmentInterrupted {
        id: TreatmentId,
        reason: InterruptionReason,
    },
    RecoveryChanged {
        id: EntityId,
        before: bool,
        after: bool,
        next_at: Option<u64>,
    },
    RecoveryTicked {
        id: EntityId,
        blood_before: u32,
        blood_after: u32,
        shock_before: u32,
        shock_after: u32,
        health_before: u16,
        health_after: u16,
    },
    WoundHealed {
        id: WoundId,
        patient: EntityId,
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
    pub sourced_medical: u128,
    pub consumed_medical: u128,
    pub lost_medical: u128,
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
    InvalidWound,
    InvalidTreatment,
    NoEligibleMedic,
    BusyEntity,
    InsufficientMedical,
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
    next_wound_id: u64,
    next_treatment_id: u64,
    wounds: BTreeMap<WoundId, Wound>,
    wound_ids_by_patient: BTreeMap<EntityId, BTreeSet<WoundId>>,
    bleeding_rate_by_patient: BTreeMap<EntityId, u64>,
    casualty: BTreeMap<EntityId, CasualtyState>,
    treatments: BTreeMap<TreatmentId, Treatment>,
    treatment_due: BTreeMap<u64, BTreeSet<TreatmentId>>,
    due_by_treatment: BTreeMap<TreatmentId, u64>,
    active_by_entity: BTreeMap<EntityId, TreatmentId>,
    treatment_ids_by_entity: BTreeMap<EntityId, BTreeSet<TreatmentId>>,
    medic_index: BTreeMap<(u16, u32), BTreeSet<EntityId>>,
    available_medics: BTreeMap<(u16, u32, u32), BTreeSet<EntityId>>,
    /// Exact reverse membership for bounded availability refreshes.
    availability_by_medic: BTreeMap<EntityId, BTreeSet<(u16, u32, u32)>>,
    sourced_medical: u128,
    consumed_medical: u128,
    lost_medical: u128,
    /// Test-only structural evidence: entities selected from a due bucket or
    /// indexed hot membership for an automatic timestamp. This is deliberately
    /// absent from snapshots, digests, and release builds.
    #[cfg(test)]
    automatic_journal_visits: u64,
    #[cfg(test)]
    automatic_execution_visits: u64,
    #[cfg(test)]
    medical_entity_candidates: u64,
    #[cfg(test)]
    wound_index_visits: u64,
    #[cfg(test)]
    treatment_completion_candidates: u64,
    #[cfg(test)]
    selection_candidates: u64,
}

#[derive(Clone)]
struct CompositeTransition {
    living: LivingState,
    casualty: Option<CasualtyState>,
    interruption: Option<(TreatmentId, InterruptionReason)>,
    inventory: Inventory,
    consumed_food: u128,
    consumed_water: u128,
    events: Vec<TimedEvent>,
    recovery_completed: bool,
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
        if let Some(c) = self.casualty.get(&id) {
            let rate = self.bleeding_rate_by_patient.get(&id).copied().unwrap_or(0);
            if rate != 0 {
                let ceil_loss = |amount: u64| amount.saturating_add(rate - 1) / rate;
                let blood = ceil_loss(u64::from(c.blood)).max(1);
                let shock_loss = u64::from(1000_u32.saturating_sub(c.shock))
                    .saturating_mul(10)
                    .saturating_sub(u64::from(c.shock_remainder));
                let shock = ceil_loss(shock_loss).max(1);
                let incap_blood = if !c.incapacitated && c.blood > BLOOD_MAX / 3 {
                    ceil_loss(u64::from(c.blood - BLOOD_MAX / 3))
                } else {
                    u64::MAX
                };
                let incap_shock = if !c.incapacitated && c.shock < INCAPACITATED_SHOCK {
                    let loss = u64::from(INCAPACITATED_SHOCK - c.shock)
                        .saturating_mul(10)
                        .saturating_sub(u64::from(c.shock_remainder));
                    ceil_loss(loss)
                } else {
                    u64::MAX
                };
                let medical_at = c
                    .materialized_at
                    .checked_add(blood.min(shock).min(incap_blood).min(incap_shock));
                if let Some(at) = medical_at {
                    d = d.min(at.saturating_sub(l.materialized_at).max(1));
                }
            }
            if c.recovering {
                let recovery_at = c.recovery_next_at.ok_or(SimError::InvalidTreatment)?;
                if recovery_at <= l.materialized_at {
                    return Err(SimError::InvalidTreatment);
                }
                d = d.min(recovery_at - l.materialized_at);
            }
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
        casualty: Option<CasualtyState>,
        bleeding_rate: u64,
        active_treatment: Option<(TreatmentId, bool)>,
    ) -> Result<CompositeTransition, SimError> {
        if living.life != LifeState::Alive {
            return Ok(CompositeTransition {
                living,
                casualty,
                interruption: None,
                inventory,
                consumed_food: 0,
                consumed_water: 0,
                events: Vec::new(),
                recovery_completed: false,
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
        let mut casualty = casualty;
        let mut interruption = None;
        let mut recovery_completed = false;
        // Living needs and casualty physiology share this materialization
        // boundary.  In particular, a needs death at `at` must not leave the
        // casualty half of the authoritative record at an older timestamp.
        // Living death has precedence when both mechanisms become fatal in
        // the same second; casualty state is still fully materialized, but a
        // second death event is never emitted.
        if let Some(mut c) = casualty {
            let elapsed = at
                .checked_sub(c.materialized_at)
                .ok_or(SimError::TimeReversal)?;
            let loss = bleeding_rate
                .checked_mul(elapsed)
                .ok_or(SimError::ArithmeticOverflow)?;
            c.blood = c.blood.saturating_sub(
                u32::try_from(loss.min(u64::from(u32::MAX)))
                    .map_err(|_| SimError::ArithmeticOverflow)?,
            );
            let shock_numerator = u64::from(c.shock_remainder)
                .checked_add(loss)
                .ok_or(SimError::ArithmeticOverflow)?;
            c.shock = c
                .shock
                .saturating_add(u32::try_from(shock_numerator / 10).unwrap_or(u32::MAX))
                .min(1000);
            c.shock_remainder = u8::try_from(shock_numerator % 10).expect("remainder below ten");
            let was_incapacitated = c.incapacitated;
            c.incapacitated = c.shock >= INCAPACITATED_SHOCK || c.blood <= BLOOD_MAX / 3;
            c.materialized_at = at;
            let medically_fatal = c.blood == 0 || c.shock >= 1000;
            // Death is terminal for recovery.  Physiology is materialized for
            // audit, but no recovery/healing event may follow a death at this
            // boundary (including a same-second living-needs death).
            if l.life != LifeState::Alive || medically_fatal {
                c.recovering = false;
                c.recovery_next_at = None;
            } else if c.recovering && c.recovery_next_at == Some(at) && bleeding_rate == 0 {
                let blood_before = c.blood;
                let shock_before = c.shock;
                let health_before = l.health;
                c.blood = c
                    .blood
                    .saturating_add(RECOVERY_BLOOD_PER_TICK)
                    .min(BLOOD_MAX);
                c.shock = c.shock.saturating_sub(RECOVERY_SHOCK_PER_TICK);
                l.health = l.health.saturating_add(RECOVERY_HEALTH_PER_TICK).min(1000);
                c.incapacitated = c.shock >= INCAPACITATED_SHOCK || c.blood <= BLOOD_MAX / 3;
                events.push(TimedEvent {
                    at,
                    event: Event::RecoveryTicked {
                        id,
                        blood_before,
                        blood_after: c.blood,
                        shock_before,
                        shock_after: c.shock,
                        health_before,
                        health_after: l.health,
                    },
                });
                if c.blood == BLOOD_MAX && c.shock == 0 && l.health == 1000 {
                    c.recovering = false;
                    c.recovery_next_at = None;
                    recovery_completed = true;
                } else {
                    c.recovery_next_at = Some(
                        at.checked_add(RECOVERY_INTERVAL)
                            .ok_or(SimError::ArithmeticOverflow)?,
                    );
                }
            }
            if l.life == LifeState::Alive
                && c.incapacitated
                && !was_incapacitated
                && l.activity != Activity::Idle
            {
                let before = l.activity;
                l.activity = Activity::Idle;
                events.push(TimedEvent {
                    at,
                    event: Event::ActivityChanged {
                        id,
                        before,
                        after: Activity::Idle,
                        forced: true,
                    },
                });
            }
            if l.life == LifeState::Alive && c.incapacitated && !was_incapacitated {
                interruption =
                    active_treatment.map(|(tid, _)| (tid, InterruptionReason::Ineligible));
            }
            if l.life == LifeState::Alive && medically_fatal {
                let cause = if c.blood == 0 {
                    DeathCause::Hemorrhage
                } else {
                    DeathCause::TraumaticShock
                };
                let health_before = l.health;
                l.health = 0;
                l.life = LifeState::Dead { at, cause };
                events.push(TimedEvent {
                    at,
                    event: Event::SoldierDied {
                        id,
                        cause,
                        health_before,
                    },
                });
            }
            casualty = Some(c);
        }
        if l.life != LifeState::Alive {
            interruption = active_treatment.map(|(tid, is_medic)| {
                (
                    tid,
                    if is_medic {
                        InterruptionReason::MedicDied
                    } else {
                        InterruptionReason::PatientDied
                    },
                )
            });
        }
        Ok(CompositeTransition {
            living: l,
            casualty,
            interruption,
            inventory: inv,
            consumed_food,
            consumed_water,
            events,
            recovery_completed,
        })
    }
    fn commit_transition(
        &mut self,
        id: EntityId,
        transition: CompositeTransition,
        out: &mut Vec<TimedEvent>,
    ) {
        let i = id.index();
        self.soldiers.living[i] = transition.living;
        if let Some(casualty) = transition.casualty {
            self.casualty.insert(id, casualty);
        }
        self.soldiers.data[i].inventory = transition.inventory;
        self.soldiers.data[i].health = transition.living.health;
        self.refresh_medic_availability(id);
        self.consumed_food += transition.consumed_food;
        self.consumed_water += transition.consumed_water;
        out.extend(transition.events);
        if transition.recovery_completed {
            if let Some(ids) = self.wound_ids_by_patient.get(&id) {
                for wound_id in ids {
                    let wound = self.wounds.get_mut(wound_id).expect("indexed wound");
                    if wound.controlled && !wound.healed {
                        wound.healed = true;
                        out.push(TimedEvent {
                            at: transition.living.materialized_at,
                            event: Event::WoundHealed {
                                id: *wound_id,
                                patient: id,
                            },
                        });
                    }
                }
            }
            out.push(TimedEvent {
                at: transition.living.materialized_at,
                event: Event::RecoveryChanged {
                    id,
                    before: true,
                    after: false,
                    next_at: None,
                },
            });
        }
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
            next_wound_id: 0,
            next_treatment_id: 0,
            wounds: BTreeMap::new(),
            wound_ids_by_patient: BTreeMap::new(),
            bleeding_rate_by_patient: BTreeMap::new(),
            casualty: BTreeMap::new(),
            treatments: BTreeMap::new(),
            treatment_due: BTreeMap::new(),
            due_by_treatment: BTreeMap::new(),
            active_by_entity: BTreeMap::new(),
            treatment_ids_by_entity: BTreeMap::new(),
            medic_index: BTreeMap::new(),
            available_medics: BTreeMap::new(),
            availability_by_medic: BTreeMap::new(),
            sourced_medical: 0,
            consumed_medical: 0,
            lost_medical: 0,
            #[cfg(test)]
            automatic_journal_visits: 0,
            #[cfg(test)]
            automatic_execution_visits: 0,
            #[cfg(test)]
            medical_entity_candidates: 0,
            #[cfg(test)]
            wound_index_visits: 0,
            #[cfg(test)]
            treatment_completion_candidates: 0,
            #[cfg(test)]
            selection_candidates: 0,
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
    pub fn wound(&self, id: WoundId) -> Option<Wound> {
        self.wounds.get(&id).copied()
    }
    pub fn casualty_state(&self, id: EntityId) -> Option<CasualtyState> {
        let mut casualty = self.casualty.get(&id).copied()?;
        if self.soldiers.valid(id)
            && self.soldiers.living[id.index()].life == LifeState::Alive
            && casualty.materialized_at < self.clock
        {
            let elapsed = self.clock - casualty.materialized_at;
            let rate = self.bleeding_rate_by_patient.get(&id).copied().unwrap_or(0);
            let loss = rate.checked_mul(elapsed)?;
            casualty.blood = casualty
                .blood
                .saturating_sub(u32::try_from(loss).unwrap_or(u32::MAX));
            let numerator = u64::from(casualty.shock_remainder).checked_add(loss)?;
            casualty.shock = casualty
                .shock
                .saturating_add(u32::try_from(numerator / 10).unwrap_or(u32::MAX))
                .min(1000);
            casualty.shock_remainder = u8::try_from(numerator % 10).ok()?;
            casualty.incapacitated =
                casualty.shock >= INCAPACITATED_SHOCK || casualty.blood <= BLOOD_MAX / 3;
            casualty.materialized_at = self.clock;
        }
        Some(casualty)
    }
    pub fn treatment(&self, id: TreatmentId) -> Option<Treatment> {
        self.treatments.get(&id).copied()
    }
    pub fn wounds_of(&self, patient: EntityId) -> Vec<Wound> {
        self.wound_ids_by_patient
            .get(&patient)
            .into_iter()
            .flatten()
            .filter_map(|id| self.wounds.get(id).copied())
            .collect()
    }
    pub fn apply(&mut self, c: Command) -> ApplyOutcome {
        let mut events = Vec::new();
        let mut blocked = None;
        let error = match c {
            Command::AdvanceTo { target } => self.advance(target, &mut events, &mut blocked),
            Command::InflictWound { patient, wound } => {
                self.inflict_wound(patient, wound).map(|es| {
                    events.extend(es.into_iter().map(|event| TimedEvent {
                        at: self.clock,
                        event,
                    }));
                })
            }
            Command::DespawnSoldier { id } => self.despawn_materialized(id, &mut events),
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
    /// Materialize an endpoint through the authoritative clock before removing
    /// it.  The transition is fully staged before mutation, so projection
    /// failures cannot partially clean relationships or resource accounting.
    fn despawn_materialized(
        &mut self,
        id: EntityId,
        events: &mut Vec<TimedEvent>,
    ) -> Result<(), SimError> {
        if !self.soldiers.valid(id) {
            return Err(SimError::InvalidEntity);
        }
        let active = self.active_by_entity.get(&id).copied().map(|tid| {
            (
                tid,
                self.treatments.get(&tid).is_some_and(|t| t.medic == id),
            )
        });
        let transition = Self::transition_second(
            self.soldiers.living[id.index()],
            self.soldiers.data[id.index()].inventory,
            id,
            self.clock,
            self.casualty.get(&id).copied(),
            self.bleeding_rate_by_patient.get(&id).copied().unwrap_or(0),
            active,
        )?;
        // Everything below this point mutates authoritative state.  Preflight
        // every checked accumulator and relationship which the commit will
        // touch so removal is a single, infallible transaction.
        let transition_interruption = transition.interruption;
        let mut spec = self.soldiers.data[id.index()];
        spec.inventory = transition.inventory;
        spec.health = transition.living.health;
        let lost_food = self
            .lost_food
            .checked_add(u128::from(transition.inventory.food))
            .ok_or(SimError::ArithmeticOverflow)?;
        let lost_water = self
            .lost_water
            .checked_add(u128::from(transition.inventory.water))
            .ok_or(SimError::ArithmeticOverflow)?;
        let lost_medical = self
            .lost_medical
            .checked_add(u128::from(transition.inventory.medical))
            .ok_or(SimError::ArithmeticOverflow)?;
        self.consumed_food
            .checked_add(transition.consumed_food)
            .ok_or(SimError::ArithmeticOverflow)?;
        self.consumed_water
            .checked_add(transition.consumed_water)
            .ok_or(SimError::ArithmeticOverflow)?;
        if let Some((tid, _)) = active.map(|(tid, is_medic)| {
            (
                tid,
                if is_medic {
                    InterruptionReason::MedicRemoved
                } else {
                    InterruptionReason::PatientRemoved
                },
            )
        }) {
            let treatment = self
                .treatments
                .get(&tid)
                .ok_or(SimError::InvalidTreatment)?;
            if treatment.status != TreatmentStatus::Active
                || self.active_by_entity.get(&treatment.medic) != Some(&tid)
                || self.active_by_entity.get(&treatment.patient) != Some(&tid)
            {
                return Err(SimError::InvalidTreatment);
            }
        }
        self.unschedule_due(id);
        self.commit_transition(id, transition, events);
        if let Some((tid, reason)) = transition_interruption {
            self.interrupt_internal(tid, reason)?;
            events.push(TimedEvent {
                at: self.clock,
                event: Event::TreatmentInterrupted { id: tid, reason },
            });
        }
        let removal_interruption = self.active_by_entity.get(&id).copied().map(|tid| {
            let reason = if self.treatments[&tid].medic == id {
                InterruptionReason::MedicRemoved
            } else {
                InterruptionReason::PatientRemoved
            };
            (tid, reason)
        });
        // The fallible ledger and relationship work was proved above.  Do not
        // call the general fallible command path after materialization events
        // have committed.
        if let Some(tid) = self.active_by_entity.get(&id).copied() {
            self.interrupt_internal(tid, removal_interruption.expect("active removal").1)
                .expect("despawn relationship preflighted");
        }
        if let Some(squad) = spec.squad {
            let q = self.squads.get_mut(&squad).expect("valid relationship");
            q.members.remove(&id);
            if q.officer == Some(id) {
                q.officer = None;
            }
        }
        self.unschedule_due(id);
        if let Some(members) = self.cell_members.get_mut(&spec.position.cell) {
            members.remove(&id);
        }
        self.lost_food = lost_food;
        self.lost_water = lost_water;
        self.lost_medical = lost_medical;
        if let Some(members) = self
            .medic_index
            .get_mut(&(spec.faction, spec.position.cell))
        {
            members.remove(&id);
            if members.is_empty() {
                self.medic_index.remove(&(spec.faction, spec.position.cell));
            }
        }
        self.casualty.remove(&id);
        if let Some(wounds) = self.wound_ids_by_patient.remove(&id) {
            for wound in wounds {
                self.wounds.remove(&wound);
            }
        }
        self.bleeding_rate_by_patient.remove(&id);
        if let Some(history) = self.treatment_ids_by_entity.remove(&id) {
            for tid in history {
                if let Some(treatment) = self.treatments.remove(&tid) {
                    if let Some(at) = self.due_by_treatment.remove(&tid) {
                        if let Some(bucket) = self.treatment_due.get_mut(&at) {
                            bucket.remove(&tid);
                            if bucket.is_empty() {
                                self.treatment_due.remove(&at);
                            }
                        }
                    }
                    let other = if treatment.medic == id {
                        treatment.patient
                    } else {
                        treatment.medic
                    };
                    if let Some(other_history) = self.treatment_ids_by_entity.get_mut(&other) {
                        other_history.remove(&tid);
                        if other_history.is_empty() {
                            self.treatment_ids_by_entity.remove(&other);
                        }
                    }
                }
            }
        }
        self.soldiers.remove(id);
        self.refresh_medic_availability(id);
        let removed = Event::SoldierRemoved {
            id,
            loadout: spec.into(),
        };
        if let Some((tid, reason)) = removal_interruption {
            events.push(TimedEvent {
                at: self.clock,
                event: Event::TreatmentInterrupted { id: tid, reason },
            });
        }
        events.push(TimedEvent {
            at: self.clock,
            event: removed,
        });
        Ok(())
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
                let new_medical = self
                    .sourced_medical
                    .checked_add(u128::from(spec.inventory.medical))
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
                self.sourced_medical = new_medical;
                self.cell_members
                    .entry(spec.position.cell)
                    .or_default()
                    .insert(id);
                if let Some(s) = spec.squad {
                    self.squads.get_mut(&s).expect("checked").members.insert(id);
                }
                self.schedule_due(id)?;
                if spec.role == Role::Medic {
                    self.medic_index
                        .entry((spec.faction, spec.position.cell))
                        .or_default()
                        .insert(id);
                }
                self.refresh_medic_availability(id);
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
                let lost_medical = self
                    .lost_medical
                    .checked_add(u128::from(spec.inventory.medical))
                    .ok_or(SimError::ArithmeticOverflow)?;
                if let Some(tid) = self.active_by_entity.get(&id).copied() {
                    let reason = if self.treatments[&tid].medic == id {
                        InterruptionReason::MedicRemoved
                    } else {
                        InterruptionReason::PatientRemoved
                    };
                    self.interrupt_internal(tid, reason)?;
                }
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
                self.lost_medical = lost_medical;
                self.medic_index
                    .entry((spec.faction, spec.position.cell))
                    .or_default()
                    .remove(&id);
                self.casualty.remove(&id);
                if let Some(wounds) = self.wound_ids_by_patient.remove(&id) {
                    for wound in wounds {
                        self.wounds.remove(&wound);
                    }
                }
                self.bleeding_rate_by_patient.remove(&id);
                if let Some(history) = self.treatment_ids_by_entity.remove(&id) {
                    for tid in history {
                        if let Some(treatment) = self.treatments.remove(&tid) {
                            if let Some(at) = self.due_by_treatment.remove(&tid) {
                                if let Some(bucket) = self.treatment_due.get_mut(&at) {
                                    bucket.remove(&tid);
                                    if bucket.is_empty() {
                                        self.treatment_due.remove(&at);
                                    }
                                }
                            }
                            let other = if treatment.medic == id {
                                treatment.patient
                            } else {
                                treatment.medic
                            };
                            if let Some(other_history) =
                                self.treatment_ids_by_entity.get_mut(&other)
                            {
                                other_history.remove(&tid);
                                if other_history.is_empty() {
                                    self.treatment_ids_by_entity.remove(&other);
                                }
                            }
                        }
                    }
                }
                self.soldiers.remove(id);
                self.refresh_medic_availability(id);
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
                if self.active_by_entity.contains_key(&id)
                    || (activity == Activity::March && self.is_incapacitated(id))
                {
                    return Err(SimError::BusyEntity);
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
                self.refresh_medic_availability(id);
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
            Command::InflictWound { .. } => unreachable!("multi-event command handled by apply"),
            Command::StartTreatment {
                medic,
                patient,
                wound,
                kind,
            } => self.start_treatment(medic, patient, wound, kind),
            Command::RequestTreatment {
                patient,
                wound,
                kind,
            } => {
                if !self.soldiers.valid(patient) {
                    return Err(SimError::InvalidEntity);
                }
                let spec = self.soldiers.data[patient.index()];
                let cost = match kind {
                    TreatmentKind::Hemostatic => HEMOSTATIC_COST,
                    TreatmentKind::Shock => SHOCK_TREATMENT_COST,
                };
                let mut visits = 0_u64;
                let medic = self
                    .available_medics
                    .get(&(spec.faction, spec.position.cell, cost))
                    .and_then(|ids| {
                        ids.iter().copied().find(|candidate| {
                            visits += 1;
                            *candidate != patient
                        })
                    })
                    .ok_or(SimError::NoEligibleMedic)?;
                let event = self.start_treatment(medic, patient, wound, kind)?;
                #[cfg(test)]
                {
                    self.selection_candidates = self
                        .selection_candidates
                        .checked_add(visits)
                        .expect("test selection counter overflow");
                }
                Ok(event)
            }
            Command::InterruptTreatment { id } => {
                self.interrupt_internal(id, InterruptionReason::Explicit)?;
                Ok(Event::TreatmentInterrupted {
                    id,
                    reason: InterruptionReason::Explicit,
                })
            }
            Command::AdvanceTo { .. } => unreachable!(),
        }
    }
    fn is_incapacitated(&self, id: EntityId) -> bool {
        self.casualty.get(&id).is_some_and(|c| c.incapacitated)
    }
    fn refresh_medic_availability(&mut self, id: EntityId) {
        if let Some(keys) = self.availability_by_medic.remove(&id) {
            for key in keys {
                if let Some(ids) = self.available_medics.get_mut(&key) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.available_medics.remove(&key);
                    }
                }
            }
        }
        if !self.soldiers.valid(id) {
            return;
        }
        let s = self.soldiers.data[id.index()];
        let available = s.role == Role::Medic
            && self.soldiers.living[id.index()].life == LifeState::Alive
            && self.soldiers.living[id.index()].activity != Activity::March
            && !self.is_incapacitated(id)
            && !self.active_by_entity.contains_key(&id);
        if available {
            for cost in [HEMOSTATIC_COST, SHOCK_TREATMENT_COST] {
                if s.inventory.medical >= cost {
                    let key = (s.faction, s.position.cell, cost);
                    self.available_medics.entry(key).or_default().insert(id);
                    self.availability_by_medic
                        .entry(id)
                        .or_default()
                        .insert(key);
                }
            }
        }
    }
    fn recovery_eligible(&self, id: EntityId) -> bool {
        self.soldiers.valid(id)
            && self.soldiers.living[id.index()].life == LifeState::Alive
            && self
                .casualty
                .get(&id)
                .is_some_and(|c| c.blood != 0 && c.shock < 1000)
            && self.bleeding_rate_by_patient.get(&id).copied().unwrap_or(0) == 0
            && self.wound_ids_by_patient.get(&id).is_some_and(|ids| {
                !ids.is_empty()
                    && ids.iter().all(|wid| {
                        self.wounds
                            .get(wid)
                            .is_some_and(|w| w.controlled || w.healed)
                    })
            })
    }
    fn reevaluate_recovery(&mut self, id: EntityId, at: u64) -> Result<Option<Event>, SimError> {
        let eligible = self.recovery_eligible(id);
        let Some(c) = self.casualty.get_mut(&id) else {
            return Ok(None);
        };
        if eligible && !c.recovering {
            let next = at
                .checked_add(RECOVERY_INTERVAL)
                .ok_or(SimError::ArithmeticOverflow)?;
            c.recovering = true;
            c.recovery_next_at = Some(next);
            Ok(Some(Event::RecoveryChanged {
                id,
                before: false,
                after: true,
                next_at: Some(next),
            }))
        } else if !eligible && c.recovering {
            c.recovering = false;
            c.recovery_next_at = None;
            Ok(Some(Event::RecoveryChanged {
                id,
                before: true,
                after: false,
                next_at: None,
            }))
        } else {
            Ok(None)
        }
    }
    fn eligible_medic(&self, medic: EntityId, patient: EntityId, kind: TreatmentKind) -> bool {
        if medic == patient || !self.soldiers.valid(medic) || !self.soldiers.valid(patient) {
            return false;
        }
        let m = self.soldiers.data[medic.index()];
        let p = self.soldiers.data[patient.index()];
        let cost = match kind {
            TreatmentKind::Hemostatic => HEMOSTATIC_COST,
            TreatmentKind::Shock => SHOCK_TREATMENT_COST,
        };
        m.role == Role::Medic
            && m.faction == p.faction
            && m.position.cell == p.position.cell
            && m.inventory.medical >= cost
            && self.soldiers.living[medic.index()].life == LifeState::Alive
            && self.soldiers.living[patient.index()].life == LifeState::Alive
            && self.soldiers.living[medic.index()].activity != Activity::March
            && self.soldiers.living[patient.index()].activity != Activity::March
            && !self.is_incapacitated(medic)
            && !self.active_by_entity.contains_key(&medic)
            && !self.active_by_entity.contains_key(&patient)
    }
    fn inflict_wound(
        &mut self,
        patient: EntityId,
        spec: WoundSpec,
    ) -> Result<Vec<Event>, SimError> {
        if !self.soldiers.valid(patient) {
            return Err(SimError::InvalidEntity);
        }
        if self.soldiers.living[patient.index()].life != LifeState::Alive {
            return Err(SimError::DeadEntity);
        }
        if (spec.trauma == 0 && spec.bleeding_per_second == 0 && spec.shock == 0)
            || spec.trauma > 1000
            || spec.bleeding_per_second > 1000
            || spec.shock > 1000
        {
            return Err(SimError::InvalidWound);
        }
        let id = WoundId(self.next_wound_id);
        let next = self
            .next_wound_id
            .checked_add(1)
            .ok_or(SimError::ArithmeticOverflow)?;
        // Preserve lazily projected living work before trauma changes the
        // shared materialization boundary.
        let mut old = self.soldiers.living[patient.index()];
        Self::project(&mut old, self.clock)?;
        let health = old.health.saturating_sub(spec.trauma);
        let old_rate = self
            .bleeding_rate_by_patient
            .get(&patient)
            .copied()
            .unwrap_or(0);
        let rate = old_rate
            .checked_add(u64::from(spec.bleeding_per_second))
            .ok_or(SimError::ArithmeticOverflow)?;
        let mut casualty = self
            .casualty
            .get(&patient)
            .copied()
            .unwrap_or(CasualtyState {
                blood: BLOOD_MAX,
                materialized_at: self.clock,
                ..CasualtyState::default()
            });
        let elapsed = self
            .clock
            .checked_sub(casualty.materialized_at)
            .ok_or(SimError::TimeReversal)?;
        let prior_loss = old_rate
            .checked_mul(elapsed)
            .ok_or(SimError::ArithmeticOverflow)?;
        casualty.blood = casualty.blood.saturating_sub(
            u32::try_from(prior_loss.min(u64::from(u32::MAX)))
                .map_err(|_| SimError::ArithmeticOverflow)?,
        );
        let shock_numerator = u64::from(casualty.shock_remainder)
            .checked_add(prior_loss)
            .ok_or(SimError::ArithmeticOverflow)?;
        casualty.shock = casualty
            .shock
            .saturating_add(u32::try_from(shock_numerator / 10).unwrap_or(u32::MAX))
            .min(1000);
        casualty.shock_remainder = u8::try_from(shock_numerator % 10).expect("remainder below ten");
        casualty.materialized_at = self.clock;
        let was_incapacitated = casualty.incapacitated;
        casualty.shock = casualty
            .shock
            .checked_add(u32::from(spec.shock))
            .ok_or(SimError::ArithmeticOverflow)?
            .min(1000);
        casualty.incapacitated =
            casualty.shock >= INCAPACITATED_SHOCK || casualty.blood <= BLOOD_MAX / 3;
        let recovery_was_active = casualty.recovering;
        casualty.recovering = false;
        casualty.recovery_next_at = None;
        let mut replacement = old;
        replacement.health = health;
        replacement.materialized_at = self.clock;
        let death_cause = if health == 0 {
            Some(DeathCause::ImmediateTrauma)
        } else if casualty.shock >= 1000 {
            Some(DeathCause::TraumaticShock)
        } else {
            None
        };
        if let Some(cause) = death_cause {
            replacement.life = LifeState::Dead {
                at: self.clock,
                cause,
            };
            replacement.health = 0;
        }
        let forced_idle = death_cause.is_none()
            && casualty.incapacitated
            && replacement.activity != Activity::Idle;
        let activity_before = replacement.activity;
        if forced_idle {
            replacement.activity = Activity::Idle;
        }
        let interruption = self
            .active_by_entity
            .get(&patient)
            .copied()
            .and_then(|tid| {
                if death_cause.is_some() {
                    let t = self.treatments[&tid];
                    Some((
                        tid,
                        if t.medic == patient {
                            InterruptionReason::MedicDied
                        } else {
                            InterruptionReason::PatientDied
                        },
                    ))
                } else if !was_incapacitated && casualty.incapacitated {
                    Some((tid, InterruptionReason::Ineligible))
                } else {
                    None
                }
            });
        self.next_wound_id = next;
        self.soldiers.living[patient.index()] = replacement;
        self.soldiers.data[patient.index()].health = replacement.health;
        self.casualty.insert(patient, casualty);
        self.wounds.insert(
            id,
            Wound {
                id,
                patient,
                created_at: self.clock,
                spec,
                controlled: false,
                healed: false,
            },
        );
        self.wound_ids_by_patient
            .entry(patient)
            .or_default()
            .insert(id);
        if rate != 0 {
            self.bleeding_rate_by_patient.insert(patient, rate);
        }
        self.schedule_due(patient)?;
        self.refresh_medic_availability(patient);
        let mut events = vec![Event::WoundInflicted {
            id,
            patient,
            wound: spec,
        }];
        if forced_idle {
            events.push(Event::ActivityChanged {
                id: patient,
                before: activity_before,
                after: Activity::Idle,
                forced: true,
            });
        }
        if let Some(cause) = death_cause {
            events.push(Event::SoldierDied {
                id: patient,
                cause,
                health_before: old.health,
            });
        }
        if recovery_was_active && death_cause.is_none() {
            events.push(Event::RecoveryChanged {
                id: patient,
                before: true,
                after: false,
                next_at: None,
            });
        }
        if let Some((tid, reason)) = interruption {
            self.interrupt_internal(tid, reason)?;
            events.push(Event::TreatmentInterrupted { id: tid, reason });
        }
        Ok(events)
    }
    fn start_treatment(
        &mut self,
        medic: EntityId,
        patient: EntityId,
        wound: Option<WoundId>,
        kind: TreatmentKind,
    ) -> Result<Event, SimError> {
        if !self.eligible_medic(medic, patient, kind) {
            return Err(SimError::InvalidTreatment);
        }
        if kind == TreatmentKind::Hemostatic
            && !wound
                .and_then(|id| self.wounds.get(&id))
                .is_some_and(|w| w.patient == patient && !w.controlled && !w.healed)
        {
            return Err(SimError::InvalidTreatment);
        }
        if kind == TreatmentKind::Shock && wound.is_some() {
            return Err(SimError::InvalidTreatment);
        }
        if kind == TreatmentKind::Shock
            && !self
                .casualty
                .get(&patient)
                .is_some_and(|c| c.shock > 0 || c.incapacitated)
        {
            return Err(SimError::InvalidTreatment);
        }
        let consumed = match kind {
            TreatmentKind::Hemostatic => HEMOSTATIC_COST,
            TreatmentKind::Shock => SHOCK_TREATMENT_COST,
        };
        let duration = match kind {
            TreatmentKind::Hemostatic => HEMOSTATIC_DURATION,
            TreatmentKind::Shock => SHOCK_TREATMENT_DURATION,
        };
        let completes_at = self
            .clock
            .checked_add(duration)
            .ok_or(SimError::ArithmeticOverflow)?;
        let id = TreatmentId(self.next_treatment_id);
        let next = self
            .next_treatment_id
            .checked_add(1)
            .ok_or(SimError::ArithmeticOverflow)?;
        let ledger = self
            .consumed_medical
            .checked_add(u128::from(consumed))
            .ok_or(SimError::ArithmeticOverflow)?;
        self.soldiers.data[medic.index()].inventory.medical -= consumed;
        self.consumed_medical = ledger;
        self.next_treatment_id = next;
        let treatment = Treatment {
            id,
            medic,
            patient,
            wound,
            kind,
            started_at: self.clock,
            completes_at,
            consumed,
            status: TreatmentStatus::Active,
        };
        self.treatments.insert(id, treatment);
        self.treatment_due
            .entry(completes_at)
            .or_default()
            .insert(id);
        self.due_by_treatment.insert(id, completes_at);
        self.active_by_entity.insert(medic, id);
        self.active_by_entity.insert(patient, id);
        self.treatment_ids_by_entity
            .entry(medic)
            .or_default()
            .insert(id);
        self.treatment_ids_by_entity
            .entry(patient)
            .or_default()
            .insert(id);
        self.refresh_medic_availability(medic);
        self.refresh_medic_availability(patient);
        Ok(Event::TreatmentStarted {
            id,
            medic,
            patient,
            wound,
            kind,
            completes_at,
            consumed,
        })
    }
    fn interrupt_internal(
        &mut self,
        id: TreatmentId,
        reason: InterruptionReason,
    ) -> Result<(), SimError> {
        let t = self
            .treatments
            .get_mut(&id)
            .ok_or(SimError::InvalidTreatment)?;
        if t.status != TreatmentStatus::Active {
            return Err(SimError::InvalidTreatment);
        }
        t.status = TreatmentStatus::Interrupted {
            at: self.clock,
            reason,
        };
        if let Some(at) = self.due_by_treatment.remove(&id) {
            if let Some(bucket) = self.treatment_due.get_mut(&at) {
                bucket.remove(&id);
                if bucket.is_empty() {
                    self.treatment_due.remove(&at);
                }
            }
        }
        self.active_by_entity.remove(&t.medic);
        self.active_by_entity.remove(&t.patient);
        let medic = t.medic;
        let patient = t.patient;
        self.refresh_medic_availability(medic);
        self.refresh_medic_availability(patient);
        Ok(())
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
            let treatment_at = self
                .treatment_due
                .keys()
                .next()
                .copied()
                .filter(|at| *at <= t);
            let Some(at) = cold_at.into_iter().chain(hot_at).chain(treatment_at).min() else {
                break;
            };
            let mut ids = BTreeSet::new();
            if cold_at == Some(at) {
                ids.extend(self.living_due.get(&at).into_iter().flatten().copied());
            }
            if treatment_at == Some(at) {
                if let Some(treatments) = self.treatment_due.get(&at) {
                    for tid in treatments {
                        if let Some(treatment) = self.treatments.get(tid) {
                            ids.insert(treatment.medic);
                            ids.insert(treatment.patient);
                        }
                    }
                }
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
            let mut interruptions = BTreeMap::new();
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
                let is_due = self.due_by_entity.get(&id) == Some(&at);
                let is_treatment_endpoint = treatment_at == Some(at)
                    && self
                        .active_by_entity
                        .get(&id)
                        .is_some_and(|tid| self.due_by_treatment.get(tid) == Some(&at));
                if !is_hot && !is_due && !is_treatment_endpoint {
                    continue;
                }
                if is_hot {
                    hot_count = hot_count
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                } else if is_due {
                    cold_count = cold_count
                        .checked_add(1)
                        .ok_or(SimError::ArithmeticOverflow)?;
                }
                let active = self.active_by_entity.get(&id).copied().map(|tid| {
                    (
                        tid,
                        self.treatments.get(&tid).is_some_and(|t| t.medic == id),
                    )
                });
                let transition = Self::transition_second(
                    self.soldiers.living[id.index()],
                    self.soldiers.data[id.index()].inventory,
                    id,
                    at,
                    self.casualty.get(&id).copied(),
                    self.bleeding_rate_by_patient.get(&id).copied().unwrap_or(0),
                    active,
                )?;
                #[cfg(test)]
                if transition.casualty.is_some() {
                    self.medical_entity_candidates += 1;
                }
                if let Some((tid, reason)) = transition.interruption {
                    interruptions.insert(tid, reason);
                }
                food_count = food_count
                    .checked_add(transition.consumed_food)
                    .ok_or(SimError::ArithmeticOverflow)?;
                water_count = water_count
                    .checked_add(transition.consumed_water)
                    .ok_or(SimError::ArithmeticOverflow)?;
                staged.push((id, is_hot, Some(transition)));
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
            for (tid, reason) in interruptions {
                let old_clock = self.clock;
                self.clock = at;
                let interrupted = self.interrupt_internal(tid, reason);
                self.clock = old_clock;
                interrupted?;
                out.push(TimedEvent {
                    at,
                    event: Event::TreatmentInterrupted { id: tid, reason },
                });
            }
            if treatment_at == Some(at) {
                self.complete_treatments_at(at, out)?;
            }
            self.hot_member_steps = next_hot;
            self.cold_boundaries = next_cold;
            for h in self
                .hot_cells
                .values_mut()
                .filter(|h| h.last_stepped_at < at)
            {
                let elapsed = at - h.last_stepped_at;
                h.fixed_steps = h
                    .fixed_steps
                    .checked_add(elapsed)
                    .ok_or(SimError::ArithmeticOverflow)?;
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

    fn complete_treatments_at(
        &mut self,
        at: u64,
        out: &mut Vec<TimedEvent>,
    ) -> Result<(), SimError> {
        let due = self.treatment_due.remove(&at).unwrap_or_default();
        #[cfg(test)]
        {
            self.treatment_completion_candidates += due.len() as u64;
        }
        for tid in due {
            if self.due_by_treatment.get(&tid) != Some(&at) {
                continue;
            }
            let t = self.treatments[&tid];
            if t.status != TreatmentStatus::Active {
                self.due_by_treatment.remove(&tid);
                continue;
            }
            // Revalidate the exact authoritative relationship at the benefit
            // boundary.  Busy/reverse corruption or newly ineligible endpoints
            // must never turn into a successful treatment.
            let relationship_valid = self.active_by_entity.get(&t.medic) == Some(&tid)
                && self.active_by_entity.get(&t.patient) == Some(&tid)
                && self.soldiers.valid(t.medic)
                && self.soldiers.valid(t.patient)
                && self.soldiers.data[t.medic.index()].role == Role::Medic
                && self.soldiers.data[t.medic.index()].faction
                    == self.soldiers.data[t.patient.index()].faction
                && self.soldiers.data[t.medic.index()].position.cell
                    == self.soldiers.data[t.patient.index()].position.cell
                && self.soldiers.living[t.medic.index()].activity != Activity::March
                && self.soldiers.living[t.patient.index()].activity != Activity::March
                && (self.soldiers.living[t.medic.index()].life != LifeState::Alive
                    || !self.is_incapacitated(t.medic));
            if !relationship_valid {
                let old_clock = self.clock;
                self.clock = at;
                let interrupted = self.interrupt_internal(tid, InterruptionReason::Ineligible);
                self.clock = old_clock;
                interrupted?;
                out.push(TimedEvent {
                    at,
                    event: Event::TreatmentInterrupted {
                        id: tid,
                        reason: InterruptionReason::Ineligible,
                    },
                });
                continue;
            }
            if self.soldiers.living[t.medic.index()].life != LifeState::Alive
                || self.soldiers.living[t.patient.index()].life != LifeState::Alive
            {
                let reason = if self.soldiers.living[t.medic.index()].life != LifeState::Alive {
                    InterruptionReason::MedicDied
                } else {
                    InterruptionReason::PatientDied
                };
                let old_clock = self.clock;
                self.clock = at;
                let interrupted = self.interrupt_internal(tid, reason);
                self.clock = old_clock;
                interrupted?;
                out.push(TimedEvent {
                    at,
                    event: Event::TreatmentInterrupted { id: tid, reason },
                });
                continue;
            }
            match t.kind {
                TreatmentKind::Hemostatic => {
                    let wid = t.wound.ok_or(SimError::InvalidTreatment)?;
                    let w = self
                        .wounds
                        .get_mut(&wid)
                        .ok_or(SimError::InvalidTreatment)?;
                    if w.patient != t.patient || w.controlled || w.healed {
                        return Err(SimError::InvalidTreatment);
                    }
                    w.controlled = true;
                    #[cfg(test)]
                    {
                        self.wound_index_visits += 1;
                    }
                    let rate = self
                        .bleeding_rate_by_patient
                        .get(&t.patient)
                        .copied()
                        .unwrap_or(0)
                        .checked_sub(u64::from(w.spec.bleeding_per_second))
                        .ok_or(SimError::ArithmeticOverflow)?;
                    if rate == 0 {
                        self.bleeding_rate_by_patient.remove(&t.patient);
                    } else {
                        self.bleeding_rate_by_patient.insert(t.patient, rate);
                    }
                }
                TreatmentKind::Shock => {
                    let c = self
                        .casualty
                        .get_mut(&t.patient)
                        .ok_or(SimError::InvalidTreatment)?;
                    c.shock = c.shock.saturating_sub(300);
                    c.incapacitated = c.shock >= INCAPACITATED_SHOCK || c.blood <= BLOOD_MAX / 3;
                }
            }
            let recovery_event = self.reevaluate_recovery(t.patient, at)?;
            self.treatments.get_mut(&tid).unwrap().status = TreatmentStatus::Completed { at };
            self.due_by_treatment.remove(&tid);
            self.active_by_entity.remove(&t.medic);
            self.active_by_entity.remove(&t.patient);
            self.refresh_medic_availability(t.medic);
            self.refresh_medic_availability(t.patient);
            self.schedule_due(t.patient)?;
            out.push(TimedEvent {
                at,
                event: Event::TreatmentCompleted {
                    id: tid,
                    medic: t.medic,
                    patient: t.patient,
                    kind: t.kind,
                },
            });
            if let Some(event) = recovery_event {
                out.push(TimedEvent { at, event });
            }
        }
        Ok(())
    }

    /// Runs one externally committed automatic segment transactionally.  The
    /// journal contains only records which can be reached by this segment: due
    /// entities through `t`, indexed members of hot cells, and the hot cells
    /// whose clocks advance.  It deliberately does not clone `World` or either
    /// world-sized entity index.
    fn automatic_to(&mut self, t: u64, out: &mut Vec<TimedEvent>) -> Result<(u64, u64), SimError> {
        #[cfg(test)]
        let structural_counters = (
            self.automatic_journal_visits,
            self.automatic_execution_visits,
            self.medical_entity_candidates,
            self.wound_index_visits,
            self.treatment_completion_candidates,
        );
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
        let mut due_treatment_ids: BTreeSet<_> = self
            .treatment_due
            .range(..=t)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        due_treatment_ids.extend(
            touched
                .iter()
                .filter_map(|id| self.active_by_entity.get(id).copied()),
        );
        for tid in &due_treatment_ids {
            if let Some(treatment) = self.treatments.get(tid) {
                touched.insert(treatment.medic);
                touched.insert(treatment.patient);
            }
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
        let casualty_records: Vec<_> = touched
            .iter()
            .map(|id| (*id, self.casualty.get(id).copied()))
            .collect();
        let wound_records: Vec<_> = touched
            .iter()
            .flat_map(|patient| {
                self.wound_ids_by_patient
                    .get(patient)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.wounds.get(id).copied())
            })
            .collect();
        let bleeding_records: Vec<_> = touched
            .iter()
            .map(|id| (*id, self.bleeding_rate_by_patient.get(id).copied()))
            .collect();
        let treatment_records: Vec<_> = due_treatment_ids
            .iter()
            .filter_map(|id| self.treatments.get(id).copied())
            .collect();
        let active_records: Vec<_> = touched
            .iter()
            .map(|id| (*id, self.active_by_entity.get(id).copied()))
            .collect();
        let treatment_due_records: Vec<_> = self
            .treatment_due
            .iter()
            .filter_map(|(at, ids)| {
                let relevant: BTreeSet<_> = ids.intersection(&due_treatment_ids).copied().collect();
                (!relevant.is_empty()).then_some((*at, relevant))
            })
            .collect();
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
                for (id, old) in casualty_records {
                    if let Some(old) = old {
                        self.casualty.insert(id, old);
                    } else {
                        self.casualty.remove(&id);
                    }
                }
                for id in &touched {
                    if let Some(ids) = self.wound_ids_by_patient.remove(id) {
                        for wound in ids {
                            self.wounds.remove(&wound);
                        }
                    }
                    self.bleeding_rate_by_patient.remove(id);
                }
                for wound in wound_records {
                    self.wound_ids_by_patient
                        .entry(wound.patient)
                        .or_default()
                        .insert(wound.id);
                    self.wounds.insert(wound.id, wound);
                }
                for (id, old) in bleeding_records {
                    if let Some(old) = old {
                        self.bleeding_rate_by_patient.insert(id, old);
                    }
                }
                for treatment in treatment_records {
                    self.treatments.insert(treatment.id, treatment);
                }
                for (id, _) in &active_records {
                    self.active_by_entity.remove(id);
                }
                for (id, old) in active_records {
                    if let Some(old) = old {
                        self.active_by_entity.insert(id, old);
                    }
                }
                for tid in &due_treatment_ids {
                    if let Some(at) = self.due_by_treatment.get(tid).copied() {
                        if let Some(bucket) = self.treatment_due.get_mut(&at) {
                            bucket.remove(tid);
                            if bucket.is_empty() {
                                self.treatment_due.remove(&at);
                            }
                        }
                    }
                }
                for (at, ids) in treatment_due_records {
                    self.treatment_due.entry(at).or_default().extend(ids);
                }
                for tid in due_treatment_ids {
                    self.due_by_treatment.remove(&tid);
                    if let Some(treatment) = self.treatments.get(&tid) {
                        if treatment.status == TreatmentStatus::Active {
                            self.due_by_treatment.insert(tid, treatment.completes_at);
                        }
                    }
                }
                for id in &touched {
                    self.refresh_medic_availability(*id);
                }
                self.consumed_food = counters.0;
                self.consumed_water = counters.1;
                self.cold_boundaries = counters.2;
                self.hot_member_steps = counters.3;
                #[cfg(test)]
                {
                    self.automatic_journal_visits = structural_counters.0;
                    self.automatic_execution_visits = structural_counters.1;
                    self.medical_entity_candidates = structural_counters.2;
                    self.wound_index_visits = structural_counters.3;
                    self.treatment_completion_candidates = structural_counters.4;
                }
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
            sourced_medical: self.sourced_medical,
            consumed_medical: self.consumed_medical,
            lost_medical: self.lost_medical,
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
        w.u128(self.sourced_medical);
        w.u128(self.consumed_medical);
        w.u128(self.lost_medical);
        w.u64(self.next_wound_id);
        w.u64(self.next_treatment_id);
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
        w.u32(self.casualty.len() as u32);
        for (id, c) in &self.casualty {
            w.u64(id.raw());
            w.u32(c.blood);
            w.u32(c.shock);
            w.u8(c.shock_remainder);
            w.bool(c.incapacitated);
            w.bool(c.recovering);
            w.bool(c.recovery_next_at.is_some());
            if let Some(at) = c.recovery_next_at {
                w.u64(at);
            }
            w.u64(c.materialized_at);
        }
        w.u32(self.wounds.len() as u32);
        for wound in self.wounds.values() {
            w.u64(wound.id.0);
            w.u64(wound.patient.raw());
            w.u64(wound.created_at);
            w.u16(wound.spec.trauma);
            w.u16(wound.spec.bleeding_per_second);
            w.u16(wound.spec.shock);
            w.bool(wound.controlled);
            w.bool(wound.healed);
        }
        w.u32(self.treatments.len() as u32);
        for t in self.treatments.values() {
            w.u64(t.id.0);
            w.u64(t.medic.raw());
            w.u64(t.patient.raw());
            w.bool(t.wound.is_some());
            if let Some(id) = t.wound {
                w.u64(id.0);
            }
            w.u8(match t.kind {
                TreatmentKind::Hemostatic => 0,
                TreatmentKind::Shock => 1,
            });
            w.u64(t.started_at);
            w.u64(t.completes_at);
            w.u32(t.consumed);
            match t.status {
                TreatmentStatus::Active => w.u8(0),
                TreatmentStatus::Completed { at } => {
                    w.u8(1);
                    w.u64(at)
                }
                TreatmentStatus::Interrupted { at, reason } => {
                    w.u8(2);
                    w.u64(at);
                    w.u8(reason as u8)
                }
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
        let sourced_medical = r.u128()?;
        let consumed_medical = r.u128()?;
        let lost_medical = r.u128()?;
        let next_wound_id = r.u64()?;
        let next_treatment_id = r.u64()?;
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
        let mut casualty = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = EntityId(r.u64()?);
            let c = CasualtyState {
                blood: r.u32()?,
                shock: r.u32()?,
                shock_remainder: r.u8()?,
                incapacitated: r.bool()?,
                recovering: r.bool()?,
                recovery_next_at: if r.bool()? { Some(r.u64()?) } else { None },
                materialized_at: r.u64()?,
            };
            if !soldiers.valid(id)
                || c.blood > BLOOD_MAX
                || c.shock > 1000
                || c.shock_remainder >= 10
                || c.materialized_at > clock
                || c.recovering != c.recovery_next_at.is_some()
                || c.recovery_next_at.is_some_and(|at| at <= clock)
                || casualty.insert(id, c).is_some()
            {
                return Err(SimError::Snapshot("casualty"));
            }
        }
        let mut wounds = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = WoundId(r.u64()?);
            let wound = Wound {
                id,
                patient: EntityId(r.u64()?),
                created_at: r.u64()?,
                spec: WoundSpec {
                    trauma: r.u16()?,
                    bleeding_per_second: r.u16()?,
                    shock: r.u16()?,
                },
                controlled: r.bool()?,
                healed: r.bool()?,
            };
            if id.0 >= next_wound_id
                || !soldiers.valid(wound.patient)
                || wound.created_at > clock
                || wounds.insert(id, wound).is_some()
            {
                return Err(SimError::Snapshot("wound"));
            }
        }
        let mut treatments = BTreeMap::new();
        let mut active_by_entity = BTreeMap::new();
        for _ in 0..r.u32()? {
            let id = TreatmentId(r.u64()?);
            let medic = EntityId(r.u64()?);
            let patient = EntityId(r.u64()?);
            let wound = if r.bool()? {
                Some(WoundId(r.u64()?))
            } else {
                None
            };
            let kind = match r.u8()? {
                0 => TreatmentKind::Hemostatic,
                1 => TreatmentKind::Shock,
                _ => return Err(SimError::Snapshot("treatment kind")),
            };
            let started_at = r.u64()?;
            let completes_at = r.u64()?;
            let consumed = r.u32()?;
            let status = match r.u8()? {
                0 => TreatmentStatus::Active,
                1 => TreatmentStatus::Completed { at: r.u64()? },
                2 => {
                    let at = r.u64()?;
                    let reason = match r.u8()? {
                        0 => InterruptionReason::Explicit,
                        1 => InterruptionReason::MedicDied,
                        2 => InterruptionReason::PatientDied,
                        3 => InterruptionReason::MedicRemoved,
                        4 => InterruptionReason::PatientRemoved,
                        5 => InterruptionReason::Ineligible,
                        _ => return Err(SimError::Snapshot("interruption reason")),
                    };
                    TreatmentStatus::Interrupted { at, reason }
                }
                _ => return Err(SimError::Snapshot("treatment status")),
            };
            let t = Treatment {
                id,
                medic,
                patient,
                wound,
                kind,
                started_at,
                completes_at,
                consumed,
                status,
            };
            if id.0 >= next_treatment_id || treatments.insert(id, t).is_some() {
                return Err(SimError::Snapshot("treatment"));
            }
            if status == TreatmentStatus::Active
                && (completes_at <= clock
                    || !soldiers.valid(medic)
                    || !soldiers.valid(patient)
                    || active_by_entity.insert(medic, id).is_some()
                    || active_by_entity.insert(patient, id).is_some())
            {
                return Err(SimError::Snapshot("active treatment"));
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
            next_wound_id,
            next_treatment_id,
            wounds,
            wound_ids_by_patient: BTreeMap::new(),
            bleeding_rate_by_patient: BTreeMap::new(),
            casualty,
            treatments,
            treatment_due: BTreeMap::new(),
            due_by_treatment: BTreeMap::new(),
            active_by_entity,
            treatment_ids_by_entity: BTreeMap::new(),
            medic_index: BTreeMap::new(),
            available_medics: BTreeMap::new(),
            availability_by_medic: BTreeMap::new(),
            sourced_medical,
            consumed_medical,
            lost_medical,
            #[cfg(test)]
            automatic_journal_visits: 0,
            #[cfg(test)]
            automatic_execution_visits: 0,
            #[cfg(test)]
            medical_entity_candidates: 0,
            #[cfg(test)]
            wound_index_visits: 0,
            #[cfg(test)]
            treatment_completion_candidates: 0,
            #[cfg(test)]
            selection_candidates: 0,
        };
        let mut w = w;
        for (id, wound) in &w.wounds {
            w.wound_ids_by_patient
                .entry(wound.patient)
                .or_default()
                .insert(*id);
            if !wound.controlled && !wound.healed {
                let rate = w.bleeding_rate_by_patient.entry(wound.patient).or_default();
                *rate = rate
                    .checked_add(u64::from(wound.spec.bleeding_per_second))
                    .ok_or(SimError::Snapshot("bleeding aggregate"))?;
            }
        }
        for (id, casualty) in &w.casualty {
            if casualty.recovering
                && (w.soldiers.living[id.index()].life != LifeState::Alive
                    || casualty.blood == 0
                    || casualty.shock >= 1000
                    || w.bleeding_rate_by_patient.get(id).copied().unwrap_or(0) != 0
                    || !w.wound_ids_by_patient.get(id).is_some_and(|ids| {
                        !ids.is_empty()
                            && ids.iter().all(|wid| {
                                w.wounds.get(wid).is_some_and(|x| x.controlled || x.healed)
                            })
                    }))
            {
                return Err(SimError::Snapshot("recovery state"));
            }
        }
        for (id, treatment) in &w.treatments {
            w.treatment_ids_by_entity
                .entry(treatment.medic)
                .or_default()
                .insert(*id);
            w.treatment_ids_by_entity
                .entry(treatment.patient)
                .or_default()
                .insert(*id);
            if treatment.status == TreatmentStatus::Active {
                w.treatment_due
                    .entry(treatment.completes_at)
                    .or_default()
                    .insert(*id);
                w.due_by_treatment.insert(*id, treatment.completes_at);
            }
        }
        for i in 0..w.soldiers.alive.len() {
            if w.soldiers.alive[i] {
                let id = EntityId::from_parts(i as u32, w.soldiers.generation[i]);
                w.cell_members
                    .entry(w.soldiers.data[i].position.cell)
                    .or_default()
                    .insert(id);
                if w.soldiers.data[i].role == Role::Medic {
                    let s = w.soldiers.data[i];
                    w.medic_index
                        .entry((s.faction, s.position.cell))
                        .or_default()
                        .insert(id);
                }
                w.refresh_medic_availability(id);
            }
        }
        w.validate()?;
        Ok(w)
    }
    fn validate(&self) -> Result<(), SimError> {
        let mut expected_wounds: BTreeMap<EntityId, BTreeSet<WoundId>> = BTreeMap::new();
        let mut expected_bleeding: BTreeMap<EntityId, u64> = BTreeMap::new();
        for (id, wound) in &self.wounds {
            if *id != wound.id
                || !self.soldiers.valid(wound.patient)
                || !self.casualty.contains_key(&wound.patient)
            {
                return Err(SimError::Snapshot("wound index"));
            }
            expected_wounds
                .entry(wound.patient)
                .or_default()
                .insert(*id);
            if !wound.controlled && !wound.healed {
                let rate = expected_bleeding.entry(wound.patient).or_default();
                *rate = rate
                    .checked_add(u64::from(wound.spec.bleeding_per_second))
                    .ok_or(SimError::Snapshot("bleeding aggregate"))?;
            }
        }
        if expected_wounds != self.wound_ids_by_patient
            || expected_bleeding != self.bleeding_rate_by_patient
        {
            return Err(SimError::Snapshot("wound index"));
        }
        let mut expected_treatment_due = BTreeMap::<u64, BTreeSet<TreatmentId>>::new();
        let mut expected_due_by_treatment = BTreeMap::new();
        for (id, treatment) in &self.treatments {
            if treatment.status == TreatmentStatus::Active {
                expected_treatment_due
                    .entry(treatment.completes_at)
                    .or_default()
                    .insert(*id);
                expected_due_by_treatment.insert(*id, treatment.completes_at);
            }
        }
        if expected_treatment_due != self.treatment_due
            || expected_due_by_treatment != self.due_by_treatment
        {
            return Err(SimError::Snapshot("treatment due coverage"));
        }
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
        let accounted_medical = carried
            .carried_medical
            .checked_add(self.consumed_medical)
            .and_then(|x| x.checked_add(self.lost_medical));
        if Some(self.sourced_food) != accounted_food
            || Some(self.sourced_water) != accounted_water
            || Some(self.sourced_medical) != accounted_medical
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
                    3 => DeathCause::ImmediateTrauma,
                    4 => DeathCause::Hemorrhage,
                    5 => DeathCause::TraumaticShock,
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
                            sourced_medical: 7,
                            consumed_medical: 0,
                            lost_medical: 0,
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
    fn v7_hot_living_materialization_must_equal_clock() {
        const MATERIALIZED_AT: usize = 278;
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
            let actual =
                World::transition_second(case.living, case.inventory, id, 1, None, 0, None)
                    .unwrap();
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
    fn v7_byte_corruption_matrix_rejects_every_living_class() {
        // Offsets are named from the canonical v7 writer, not found by matching
        // values (which would make fixtures ambiguous when fields are zero).
        const VERSION: usize = 0;
        const CLOCK: usize = 4;
        const SOURCED_FOOD: usize = 36;
        const SOURCED_WATER: usize = 52;
        const CONSUMED_FOOD: usize = 68;
        const CONSUMED_WATER: usize = 84;
        const LOST_FOOD: usize = 100;
        const LOST_WATER: usize = 116;
        const SOLDIER_ALIVE_TAG: usize = 220;
        const ROLE_TAG: usize = 236;
        const SPEC_HEALTH: usize = 238;
        const HUNGER: usize = 256;
        const THIRST: usize = 260;
        const FATIGUE: usize = 264;
        const SLEEP_DEBT: usize = 268;
        const MORALE: usize = 272;
        const LIVING_HEALTH: usize = 274;
        const ACTIVITY_TAG: usize = 276;
        const LIFE_TAG: usize = 277;
        const MATERIALIZED_ALIVE: usize = 278;

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
            342,
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
        let due_at = 310;
        let due_id = 322;
        mutated!("noncanonical_due_time", due_at, 99_u64.to_le_bytes());
        mutated!("stale_due_id", due_id, u64::MAX.to_le_bytes());
        let mut missing_due = base.clone();
        missing_due.drain(310..330);
        // Replace the due-bucket count with canonical zero coverage.
        overwrite(&mut missing_due, 306, 0_u32.to_le_bytes());
        cases.push((
            "missing_reverse_due_coverage",
            missing_due,
            expected("missing_reverse_due_coverage"),
        ));

        let mut hot = World::new(7);
        hot.apply(Command::SetRegionHot { cell: 0, hot: true });
        let _ = spawn(&mut hot, SoldierSpec::default());
        let hot_base = hot.snapshot();
        assert_eq!(hot_base.len(), 350, "hot fixture layout changed");
        for (name, offset, value) in [
            ("hot_activated_after_last", 306, 1_u64),
            ("hot_last_not_clock", 314, 1_u64),
            ("hot_fixed_step_mismatch", 322, 1_u64),
        ] {
            let mut b = hot_base.clone();
            if name == "hot_last_not_clock" {
                // Isolate last-step versus clock: fixed steps remains canonical
                // for activated=0,last=1 while only clock equality is broken.
                overwrite(&mut b, offset, 1_u64.to_le_bytes());
                overwrite(&mut b, 322, 1_u64.to_le_bytes());
            } else {
                overwrite(&mut b, offset, value.to_le_bytes());
            }
            cases.push((name, b, expected(name)));
        }
        let mut hot_due = hot_base.clone();
        overwrite(&mut hot_due, 334, 1_u32.to_le_bytes());
        hot_due.splice(
            338..338,
            [
                1_u64.to_le_bytes().as_slice(),
                1_u32.to_le_bytes().as_slice(),
                0_u64.to_le_bytes().as_slice(),
            ]
            .concat(),
        );
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
            ("invalid_death_cause", 286, 9_u64),
            ("death_time_mismatch", 278, 2_u64),
            ("dead_materialization_mismatch", 287, 2_u64),
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
        let dead_due_count = 315;
        overwrite(&mut dead_due, dead_due_count, 1_u32.to_le_bytes());
        dead_due.splice(
            319..319,
            [
                2_u64.to_le_bytes().as_slice(),
                1_u32.to_le_bytes().as_slice(),
                dead_id.raw().to_le_bytes().as_slice(),
            ]
            .concat(),
        );
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
            two_base.len() - 20,
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
            ordered_base[ordered_base.len() - 52..ordered_base.len() - 44]
                .try_into()
                .unwrap(),
        );
        let mut unordered = ordered_base.clone();
        overwrite(
            &mut unordered,
            ordered_base.len() - 32,
            first_at.to_le_bytes(),
        );
        cases.push((
            "noncanonical_due_order",
            unordered,
            expected("noncanonical_due_order"),
        ));

        let names: Vec<_> = cases.iter().map(|(name, _, _)| *name).collect();
        eprintln!(
            "v7 byte mutation cases ({}): {}",
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
                    sourced_medical: 7,
                    consumed_medical: 0,
                    lost_medical: 0,
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
                        sourced_medical: u128::from(fixture.inventory.medical),
                        consumed_medical: 0,
                        lost_medical: 0,
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
        assert_eq!(world.state_digest(), 0x6b79_7449_0d9e_4418);

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

    fn gate_wound(world: &mut World, patient: EntityId, rate: u16, shock: u16) -> WoundId {
        match world
            .apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: rate,
                    shock,
                },
            })
            .events[0]
            .event
        {
            Event::WoundInflicted { id, .. } => id,
            _ => unreachable!(),
        }
    }

    fn gate_start(
        world: &mut World,
        medic: EntityId,
        patient: EntityId,
        wound: WoundId,
    ) -> TreatmentId {
        match world
            .apply(Command::StartTreatment {
                medic,
                patient,
                wound: Some(wound),
                kind: TreatmentKind::Hemostatic,
            })
            .events[0]
            .event
        {
            Event::TreatmentStarted { id, .. } => id,
            _ => unreachable!(),
        }
    }

    #[test]
    fn gate_a_sparse_seven_casualties_ignore_two_thousand_cold_entities() {
        let mut world = World::new(201);
        for _ in 0..2_000 {
            spawn(&mut world, SoldierSpec::default());
        }
        let patients: Vec<_> = (0..7)
            .map(|_| spawn(&mut world, SoldierSpec::default()))
            .collect();
        for patient in &patients {
            gate_wound(&mut world, *patient, 68, 0);
            assert_eq!(world.due_by_entity.get(patient), Some(&50));
        }
        world.medical_entity_candidates = 0;
        for at in 1..50 {
            let scheduled = world.apply(Command::Schedule {
                at,
                command: ScheduledCommand::CreateStockpile {
                    id: at as u32,
                    initial: Stock::default(),
                },
            });
            assert_eq!(scheduled.error, None);
            let segment = world.apply(Command::AdvanceTo { target: at });
            assert_eq!(segment.error, None);
            assert_eq!(world.medical_entity_candidates, 0, "segment {at}");
        }
        let out = world.apply(Command::AdvanceTo { target: 50 });
        assert_eq!(world.medical_entity_candidates, 7);
        assert_eq!(
            out.events,
            vec![TimedEvent {
                at: 50,
                event: Event::TimeAdvanced {
                    from: 49,
                    to: 50,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            }]
        );
        for patient in patients {
            assert_eq!(world.casualty[&patient].blood, 1_600);
            assert!(world.casualty[&patient].incapacitated);
        }
    }

    #[test]
    fn gate_a_completion_bucket_never_traverses_treatment_history() {
        let mut world = World::new(202);
        for raw in 10_000..13_000 {
            world.treatments.insert(
                TreatmentId(raw),
                Treatment {
                    id: TreatmentId(raw),
                    medic: EntityId::from_raw(0),
                    patient: EntityId::from_raw(0),
                    wound: None,
                    kind: TreatmentKind::Shock,
                    started_at: 0,
                    completes_at: 1,
                    consumed: SHOCK_TREATMENT_COST,
                    status: TreatmentStatus::Interrupted {
                        at: 0,
                        reason: InterruptionReason::Explicit,
                    },
                },
            );
        }
        for _ in 0..3 {
            let medic = spawn(
                &mut world,
                SoldierSpec {
                    role: Role::Medic,
                    inventory: Inventory {
                        medical: 1,
                        ..Inventory::default()
                    },
                    ..SoldierSpec::default()
                },
            );
            let patient = spawn(&mut world, SoldierSpec::default());
            let wound = gate_wound(&mut world, patient, 1, 0);
            gate_start(&mut world, medic, patient, wound);
        }
        world.treatment_completion_candidates = 0;
        world.apply(Command::AdvanceTo {
            target: HEMOSTATIC_DURATION,
        });
        assert_eq!(world.treatment_completion_candidates, 3);
    }

    #[test]
    fn gate_a_wound_membership_aggregate_control_and_removal_are_exact() {
        let mut world = World::new(203);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 2,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let a = gate_wound(&mut world, patient, 3, 0);
        let b = gate_wound(&mut world, patient, 5, 0);
        assert_eq!(world.wound_ids_by_patient[&patient], BTreeSet::from([a, b]));
        assert_eq!(world.bleeding_rate_by_patient[&patient], 8);
        gate_start(&mut world, medic, patient, a);
        world.wound_index_visits = 0;
        world.apply(Command::AdvanceTo {
            target: HEMOSTATIC_DURATION,
        });
        assert_eq!(world.bleeding_rate_by_patient[&patient], 5);
        assert_eq!(world.wound_index_visits, 1);
        world.apply(Command::DespawnSoldier { id: patient });
        assert!(!world.wound_ids_by_patient.contains_key(&patient));
        assert!(!world.bleeding_rate_by_patient.contains_key(&patient));
    }

    #[test]
    fn gate_a_rate_seven_literal_non_idle_state_and_events() {
        let mut world = World::new(204);
        let patient = spawn(
            &mut world,
            SoldierSpec {
                inventory: Inventory {
                    food: 100,
                    water: 100,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        );
        world.apply(Command::SetActivity {
            id: patient,
            activity: Activity::Rest,
        });
        gate_wound(&mut world, patient, 7, 0);
        let incapacity = world.apply(Command::AdvanceTo { target: 477 });
        assert_eq!(
            world.casualty[&patient],
            CasualtyState {
                blood: 1_661,
                shock: 333,
                shock_remainder: 9,
                incapacitated: true,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 477
            }
        );
        assert_eq!(
            world.soldiers.living[patient.index()].activity,
            Activity::Idle
        );
        let mut expected = Vec::new();
        for at in [100, 200, 300, 400] {
            expected.push(TimedEvent {
                at,
                event: Event::RationConsumed {
                    id: patient,
                    food: 1,
                    water: 1,
                    hunger_before: 100,
                    hunger_after: 0,
                    thirst_before: 100,
                    thirst_after: 0,
                },
            });
        }
        expected.push(TimedEvent {
            at: 477,
            event: Event::ActivityChanged {
                id: patient,
                before: Activity::Rest,
                after: Activity::Idle,
                forced: true,
            },
        });
        expected.push(TimedEvent {
            at: 477,
            event: Event::TimeAdvanced {
                from: 0,
                to: 477,
                hot_cells_stepped: 0,
                fixed_steps_per_hot_cell: 0,
            },
        });
        assert_eq!(incapacity.events, expected);
        let death = world.apply(Command::AdvanceTo { target: 715 });
        assert_eq!(
            world.casualty[&patient],
            CasualtyState {
                blood: 0,
                shock: 500,
                shock_remainder: 5,
                incapacitated: true,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 715
            }
        );
        assert_eq!(
            death.events,
            vec![
                TimedEvent {
                    at: 489,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 0,
                        water: 1,
                        hunger_before: 89,
                        hunger_after: 89,
                        thirst_before: 101,
                        thirst_after: 1
                    }
                },
                TimedEvent {
                    at: 500,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 1,
                        water: 0,
                        hunger_before: 100,
                        hunger_after: 0,
                        thirst_before: 23,
                        thirst_after: 23
                    }
                },
                TimedEvent {
                    at: 539,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 0,
                        water: 1,
                        hunger_before: 39,
                        hunger_after: 39,
                        thirst_before: 101,
                        thirst_after: 1
                    }
                },
                TimedEvent {
                    at: 589,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 0,
                        water: 1,
                        hunger_before: 89,
                        hunger_after: 89,
                        thirst_before: 101,
                        thirst_after: 1
                    }
                },
                TimedEvent {
                    at: 600,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 1,
                        water: 0,
                        hunger_before: 100,
                        hunger_after: 0,
                        thirst_before: 23,
                        thirst_after: 23
                    }
                },
                TimedEvent {
                    at: 639,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 0,
                        water: 1,
                        hunger_before: 39,
                        hunger_after: 39,
                        thirst_before: 101,
                        thirst_after: 1
                    }
                },
                TimedEvent {
                    at: 689,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 0,
                        water: 1,
                        hunger_before: 89,
                        hunger_after: 89,
                        thirst_before: 101,
                        thirst_after: 1
                    }
                },
                TimedEvent {
                    at: 700,
                    event: Event::RationConsumed {
                        id: patient,
                        food: 1,
                        water: 0,
                        hunger_before: 100,
                        hunger_after: 0,
                        thirst_before: 23,
                        thirst_after: 23
                    }
                },
                TimedEvent {
                    at: 715,
                    event: Event::SoldierDied {
                        id: patient,
                        cause: DeathCause::Hemorrhage,
                        health_before: 1000
                    }
                },
                TimedEvent {
                    at: 715,
                    event: Event::TimeAdvanced {
                        from: 477,
                        to: 715,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                },
            ]
        );
    }

    #[test]
    fn gate_a_shared_living_and_medical_boundary_transitions_once() {
        let mut world = World::new(205);
        let patient = spawn(&mut world, SoldierSpec::default());
        world.soldiers.living[patient.index()].fatigue = 897;
        world.soldiers.living[patient.index()].activity = Activity::March;
        world.casualty.insert(
            patient,
            CasualtyState {
                blood: BLOOD_MAX / 3 + 1,
                materialized_at: 0,
                ..CasualtyState::default()
            },
        );
        gate_wound(&mut world, patient, 1, 0);
        world.medical_entity_candidates = 0;
        let out = world.apply(Command::AdvanceTo { target: 1 });
        assert_eq!(world.medical_entity_candidates, 1);
        assert_eq!(world.cold_boundaries, 1);
        assert_eq!(
            out.events,
            vec![
                TimedEvent {
                    at: 1,
                    event: Event::ActivityChanged {
                        id: patient,
                        before: Activity::March,
                        after: Activity::Idle,
                        forced: true
                    }
                },
                TimedEvent {
                    at: 1,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 1,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                },
            ]
        );
        assert_eq!(world.casualty[&patient].blood, 1_666);
    }

    #[test]
    fn gate_a_living_death_defeats_same_time_completion_and_materializes_casualty() {
        let mut world = World::new(206);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = gate_wound(&mut world, patient, 1, 0);
        let tid = gate_start(&mut world, medic, patient, wound);
        world.casualty.get_mut(&patient).unwrap().blood = BLOOD_MAX / 3 + 10;
        let l = &mut world.soldiers.living[patient.index()];
        l.health = 10;
        l.thirst = SEVERE_THIRST - 30;
        l.activity = Activity::March;
        world.soldiers.data[patient.index()].health = 10;
        world.schedule_due(patient).unwrap();
        let out = world.apply(Command::AdvanceTo { target: 10 });
        assert_eq!(
            out.events,
            vec![
                TimedEvent {
                    at: 10,
                    event: Event::LivingDeteriorated {
                        id: patient,
                        morale_before: 1000,
                        morale_after: 999,
                        health_before: 10,
                        health_after: 0
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::SoldierDied {
                        id: patient,
                        cause: DeathCause::Dehydration,
                        health_before: 10
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::TreatmentInterrupted {
                        id: tid,
                        reason: InterruptionReason::PatientDied
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 10,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                },
            ]
        );
        assert!(world.casualty[&patient].incapacitated);
        assert_eq!(
            world.soldiers.living[patient.index()].activity,
            Activity::March
        );
        assert!(matches!(
            world.treatments[&tid].status,
            TreatmentStatus::Interrupted {
                reason: InterruptionReason::PatientDied,
                ..
            }
        ));
        assert_eq!(world.casualty[&patient].materialized_at, 10);
        assert!(!world.active_by_entity.contains_key(&patient));
        assert!(!world.active_by_entity.contains_key(&medic));
        assert!(!world.due_by_treatment.contains_key(&tid));
        assert_eq!(
            out.events
                .iter()
                .filter(|e| matches!(e.event, Event::TreatmentInterrupted { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn gate_a_incapacitated_cold_casualty_leaps_to_real_boundary() {
        let mut world = World::new(207);
        let patient = spawn(&mut world, SoldierSpec::default());
        gate_wound(&mut world, patient, 100, INCAPACITATED_SHOCK as u16);
        world.medical_entity_candidates = 0;
        world.apply(Command::AdvanceTo { target: 30 });
        assert_eq!(world.medical_entity_candidates, 1);
        assert_eq!(world.casualty[&patient].blood, 2_000);
        assert_eq!(world.casualty[&patient].shock, 1_000);
    }

    #[test]
    fn gate_a_hot_cold_churn_preserves_due_deadline_remainder_and_inventory() {
        let mut world = World::new(208);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 2,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = gate_wound(&mut world, patient, 7, 0);
        let tid = gate_start(&mut world, medic, patient, wound);
        for at in 1..=9 {
            world.apply(Command::SetRegionHot {
                cell: 0,
                hot: at % 2 == 1,
            });
            world.apply(Command::AdvanceTo { target: at });
        }
        assert_eq!(world.treatments[&tid].completes_at, 10);
        assert_eq!(world.soldiers.data[medic.index()].inventory.medical, 1);
        assert_eq!(world.casualty[&patient].shock_remainder, 3);
        assert_eq!(world.due_by_treatment.get(&tid), Some(&10));
    }

    #[test]
    fn gate_a_generation_reuse_has_no_stale_medical_relationships() {
        let mut world = World::new(209);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = gate_wound(&mut world, patient, 1, 0);
        let tid = gate_start(&mut world, medic, patient, wound);
        world.apply(Command::DespawnSoldier { id: patient });
        let replacement = spawn(&mut world, SoldierSpec::default());
        assert_eq!(replacement.index(), patient.index());
        assert_ne!(replacement, patient);
        assert!(!world.casualty.contains_key(&replacement));
        assert!(!world.active_by_entity.contains_key(&replacement));
        assert!(!world.due_by_treatment.contains_key(&tid));
    }

    #[test]
    fn gate_a_later_checked_failure_restores_future_interrupted_treatment() {
        let mut world = World::new(210);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        world.apply(Command::SetActivity {
            id: patient,
            activity: Activity::Rest,
        });
        let wound = gate_wound(&mut world, patient, 1_000, 0);
        let tid = gate_start(&mut world, medic, patient, wound);
        let unrelated = spawn(&mut world, SoldierSpec::default());
        world.soldiers.living[unrelated.index()].activity = Activity::March;
        world.soldiers.living[unrelated.index()].fatigue = 882;
        world.schedule_due(unrelated).unwrap();
        assert_eq!(world.due_by_entity.get(&patient), Some(&4));
        assert_eq!(world.due_by_entity.get(&unrelated), Some(&6));
        assert_eq!(world.due_by_treatment.get(&tid), Some(&10));

        let mut control = World::from_snapshot(&world.snapshot()).unwrap();
        let control_out = control.apply(Command::AdvanceTo { target: 6 });
        assert_eq!(control_out.error, None);
        assert!(matches!(
            control.soldiers.living[patient.index()].life,
            LifeState::Dead {
                at: 5,
                cause: DeathCause::Hemorrhage
            }
        ));
        assert_eq!(
            control.treatments[&tid].status,
            TreatmentStatus::Interrupted {
                at: 4,
                reason: InterruptionReason::Ineligible
            }
        );
        assert_eq!(control_out.events.iter().filter(|event| matches!(event.event, Event::TreatmentInterrupted { id, .. } if id == tid)).count(), 1);

        world.cold_boundaries = u64::MAX - 2;
        let before = world.snapshot();
        let digest = world.state_digest();
        let counters = (
            world.hot_member_steps,
            world.cold_boundaries,
            world.automatic_journal_visits,
            world.automatic_execution_visits,
            world.medical_entity_candidates,
            world.treatment_completion_candidates,
            world.wound_index_visits,
        );
        let out = world.apply(Command::AdvanceTo { target: 10 });
        assert_eq!(out.error, Some(SimError::ArithmeticOverflow));
        assert!(out.events.is_empty());
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.state_digest(), digest);
        assert_eq!(world.clock, 0);
        assert_eq!(world.treatments[&tid].status, TreatmentStatus::Active);
        assert_eq!(world.due_by_treatment.get(&tid), Some(&10));
        assert_eq!(world.treatment_due.get(&10), Some(&BTreeSet::from([tid])));
        assert_eq!(world.active_by_entity.get(&medic), Some(&tid));
        assert_eq!(world.active_by_entity.get(&patient), Some(&tid));
        assert_eq!(
            world.casualty[&patient],
            CasualtyState {
                blood: BLOOD_MAX,
                ..CasualtyState::default()
            }
        );
        assert_eq!(
            world.soldiers.living[patient.index()].life,
            LifeState::Alive
        );
        assert_eq!(
            (
                world.hot_member_steps,
                world.cold_boundaries,
                world.automatic_journal_visits,
                world.automatic_execution_visits,
                world.medical_entity_candidates,
                world.treatment_completion_candidates,
                world.wound_index_visits,
            ),
            counters
        );
    }

    #[test]
    fn gate_a_snapshot_continuation_rebuilds_identical_due_indexes_and_events() {
        let mut world = World::new(211);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = gate_wound(&mut world, patient, 7, 0);
        gate_start(&mut world, medic, patient, wound);
        let mut restored = World::from_snapshot(&world.snapshot()).unwrap();
        assert_eq!(world.living_due, restored.living_due);
        assert_eq!(world.treatment_due, restored.treatment_due);
        let a = world.apply(Command::AdvanceTo { target: 10 });
        let b = restored.apply(Command::AdvanceTo { target: 10 });
        assert_eq!(a.events, b.events);
        assert_eq!(world.snapshot(), restored.snapshot());
        assert_eq!(world.state_digest(), restored.state_digest());
    }

    #[test]
    fn gate_a_low_blood_incapacity_survives_new_wound_and_shock_care() {
        let mut world = World::new(212);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: SHOCK_TREATMENT_COST,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        world.casualty.insert(
            patient,
            CasualtyState {
                blood: BLOOD_MAX / 3,
                shock: 400,
                shock_remainder: 0,
                incapacitated: true,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 0,
            },
        );
        gate_wound(&mut world, patient, 0, 1);
        assert!(world.casualty[&patient].incapacitated);
        let started = world.apply(Command::StartTreatment {
            medic,
            patient,
            wound: None,
            kind: TreatmentKind::Shock,
        });
        assert_eq!(started.error, None);
        assert!(matches!(
            started.events.as_slice(),
            [TimedEvent {
                event: Event::TreatmentStarted { .. },
                ..
            }]
        ));
        let completed = world.apply(Command::AdvanceTo { target: 15 });
        assert_eq!(completed.error, None);
        assert!(completed.events.iter().any(|event| matches!(
            event.event,
            Event::TreatmentCompleted {
                kind: TreatmentKind::Shock,
                ..
            }
        )));
        assert_eq!(world.casualty[&patient].shock, 101);
        assert!(world.casualty[&patient].incapacitated);
    }

    #[test]
    fn gate_a_dead_completion_defensive_path_cleans_every_index() {
        let mut world = World::new(213);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = gate_wound(&mut world, patient, 1, 0);
        let tid = gate_start(&mut world, medic, patient, wound);
        world.soldiers.living[patient.index()].life = LifeState::Dead {
            at: 10,
            cause: DeathCause::Starvation,
        };
        world.clock = 10;
        let mut events = Vec::new();
        world.complete_treatments_at(10, &mut events).unwrap();
        assert!(matches!(
            world.treatments[&tid].status,
            TreatmentStatus::Interrupted {
                reason: InterruptionReason::PatientDied,
                ..
            }
        ));
        assert!(!world.active_by_entity.contains_key(&medic));
        assert!(!world.active_by_entity.contains_key(&patient));
        assert!(!world.due_by_treatment.contains_key(&tid));
    }

    #[test]
    fn gate_b_fatal_wound_stops_recovery_without_post_death_events() {
        let mut world = World::new(301);
        let patient = spawn(&mut world, SoldierSpec::default());
        world.casualty.insert(
            patient,
            CasualtyState {
                recovering: true,
                recovery_next_at: Some(5),
                ..CasualtyState::default()
            },
        );
        let outcome = world.apply(Command::InflictWound {
            patient,
            wound: WoundSpec {
                trauma: 1000,
                bleeding_per_second: 0,
                shock: 0,
            },
        });
        assert_eq!(outcome.error, None);
        assert!(matches!(
            outcome.events.as_slice(),
            [
                TimedEvent {
                    event: Event::WoundInflicted { .. },
                    ..
                },
                TimedEvent {
                    event: Event::SoldierDied {
                        cause: DeathCause::ImmediateTrauma,
                        ..
                    },
                    ..
                }
            ]
        ));
        assert_eq!(world.soldiers.living[patient.index()].health, 0);
        assert!(!world.casualty[&patient].recovering);
        assert_eq!(world.casualty[&patient].recovery_next_at, None);
        assert_eq!(world.wounds_of(patient).len(), 1);
    }

    #[test]
    fn gate_b_same_second_needs_death_defeats_recovery_tick() {
        let id = EntityId::from_parts(0, 0);
        let living = LivingState {
            thirst: SEVERE_THIRST,
            health: 10,
            materialized_at: 0,
            ..LivingState::default()
        };
        let casualty = CasualtyState {
            blood: 4_000,
            shock: 100,
            recovering: true,
            recovery_next_at: Some(1),
            materialized_at: 0,
            ..CasualtyState::default()
        };
        let transition =
            World::transition_second(living, Inventory::default(), id, 1, Some(casualty), 0, None)
                .unwrap();
        assert!(matches!(
            transition.living.life,
            LifeState::Dead {
                cause: DeathCause::Dehydration,
                ..
            }
        ));
        assert_eq!(transition.living.health, 0);
        assert!(!transition.casualty.unwrap().recovering);
        assert!(!transition.events.iter().any(|event| matches!(
            event.event,
            Event::RecoveryTicked { .. }
                | Event::RecoveryChanged { .. }
                | Event::WoundHealed { .. }
        )));
    }

    #[test]
    fn gate_b_request_excludes_self_and_counts_actual_candidates() {
        let mut world = World::new(302);
        let patient = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let other = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let wound = gate_wound(&mut world, patient, 1, 0);
        let outcome = world.apply(Command::RequestTreatment {
            patient,
            wound: Some(wound),
            kind: TreatmentKind::Hemostatic,
        });
        assert_eq!(outcome.error, None);
        assert!(
            matches!(outcome.events[0].event, Event::TreatmentStarted { medic, patient: p, .. } if medic == other && p == patient)
        );
        assert_eq!(world.selection_candidates, 2);
    }

    #[test]
    fn gate_b_availability_refresh_touches_only_reverse_membership() {
        let mut world = World::new(303);
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: 2,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        for cell in 1..=2_000 {
            world.available_medics.entry((9, cell, 1)).or_default();
        }
        let keys = world.availability_by_medic[&medic].clone();
        assert_eq!(keys.len(), 2);
        world.refresh_medic_availability(medic);
        assert_eq!(world.availability_by_medic[&medic], keys);
        assert_eq!(world.available_medics.len(), 2_002);
    }

    #[test]
    fn gate_b_recovery_reschedule_overflow_is_explicit() {
        let id = EntityId::from_parts(0, 0);
        let living = LivingState {
            health: 500,
            materialized_at: u64::MAX - RECOVERY_INTERVAL,
            ..LivingState::default()
        };
        let casualty = CasualtyState {
            blood: 4_000,
            shock: 100,
            recovering: true,
            recovery_next_at: Some(u64::MAX),
            materialized_at: u64::MAX - RECOVERY_INTERVAL,
            ..CasualtyState::default()
        };
        let result = World::transition_second(
            living,
            Inventory::default(),
            id,
            u64::MAX,
            Some(casualty),
            0,
            None,
        );
        assert!(matches!(result, Err(SimError::ArithmeticOverflow)));
    }

    #[test]
    fn gate_b_failed_request_preserves_selection_counter_and_authority() {
        let mut world = World::new(305);
        let patient = spawn(&mut world, SoldierSpec::default());
        let _medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: HEMOSTATIC_COST,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let before = world.snapshot();
        let digest = world.state_digest();
        let counter = world.selection_candidates;
        let rejected = world.apply(Command::RequestTreatment {
            patient,
            wound: Some(WoundId(999)),
            kind: TreatmentKind::Hemostatic,
        });
        assert_eq!(rejected.error, Some(SimError::InvalidTreatment));
        assert!(rejected.events.is_empty());
        assert_eq!(world.selection_candidates, counter);
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.state_digest(), digest);
        assert!(world.treatments.is_empty());
        assert_eq!(world.next_treatment_id, 0);
    }

    #[test]
    fn gate_b_despawn_loss_overflow_rolls_back_materialized_recovery() {
        let mut world = World::new(306);
        let id = spawn(
            &mut world,
            SoldierSpec {
                inventory: Inventory {
                    medical: 1,
                    ..Inventory::default()
                },
                health: 500,
                ..SoldierSpec::default()
            },
        );
        world.casualty.insert(
            id,
            CasualtyState {
                blood: 4_000,
                shock: 100,
                recovering: true,
                recovery_next_at: Some(RECOVERY_INTERVAL),
                ..CasualtyState::default()
            },
        );
        world.clock = RECOVERY_INTERVAL;
        world.lost_medical = u128::MAX;
        let before = world.snapshot();
        let digest = world.state_digest();
        let counters = (
            world.automatic_journal_visits,
            world.automatic_execution_visits,
            world.medical_entity_candidates,
            world.wound_index_visits,
            world.treatment_completion_candidates,
            world.selection_candidates,
        );

        let outcome = world.apply(Command::DespawnSoldier { id });

        assert_eq!(outcome.error, Some(SimError::ArithmeticOverflow));
        assert!(outcome.events.is_empty());
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.state_digest(), digest);
        assert_eq!(
            (
                world.automatic_journal_visits,
                world.automatic_execution_visits,
                world.medical_entity_candidates,
                world.wound_index_visits,
                world.treatment_completion_candidates,
                world.selection_candidates,
            ),
            counters
        );
        assert!(world.soldiers.valid(id));
        assert!(world.casualty[&id].recovering);
        assert_eq!(
            world.casualty[&id].recovery_next_at,
            Some(RECOVERY_INTERVAL)
        );
        assert_eq!(world.soldiers.data[id.index()].inventory.medical, 1);
        assert_eq!(world.lost_medical, u128::MAX);
    }

    fn gate_b_recovering_world(hot: bool) -> (World, EntityId) {
        let mut world = World::new(8_001 + u64::from(hot));
        let medic = spawn(
            &mut world,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: HEMOSTATIC_COST,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = match world
            .apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 100,
                    bleeding_per_second: 10,
                    shock: 200,
                },
            })
            .events[0]
            .event
        {
            Event::WoundInflicted { id, .. } => id,
            _ => unreachable!(),
        };
        assert!(world
            .apply(Command::StartTreatment {
                medic,
                patient,
                wound: Some(wound),
                kind: TreatmentKind::Hemostatic,
            })
            .error
            .is_none());
        if hot {
            assert!(world
                .apply(Command::SetRegionHot { cell: 0, hot: true })
                .error
                .is_none());
        }
        assert!(world
            .apply(Command::AdvanceTo {
                target: HEMOSTATIC_DURATION,
            })
            .error
            .is_none());
        (world, patient)
    }

    #[test]
    fn gate_b_acceptance_private_01_canonical_recovery_due_is_exact() {
        let (world, patient) = gate_b_recovering_world(false);
        assert_eq!(world.due_by_entity.get(&patient), Some(&15));
        assert_eq!(world.living_due.get(&15), Some(&BTreeSet::from([patient])));

        let checkpoint = world.snapshot();
        let mut before = World::from_snapshot(&checkpoint).unwrap();
        assert!(before
            .apply(Command::AdvanceTo { target: 14 })
            .error
            .is_none());
        assert_eq!(before.due_by_entity.get(&patient), Some(&15));
        assert_eq!(before.living_due.get(&15), Some(&BTreeSet::from([patient])));

        let mut exact = World::from_snapshot(&checkpoint).unwrap();
        assert!(exact
            .apply(Command::AdvanceTo { target: 15 })
            .error
            .is_none());
        assert_eq!(exact.due_by_entity.get(&patient), Some(&20));
        assert_eq!(exact.living_due.get(&20), Some(&BTreeSet::from([patient])));
        assert_eq!(exact.living_due.len(), 2); // the untreated medic plus the patient
    }

    #[test]
    fn gate_b_acceptance_private_02_hot_and_cold_recovery_work_is_exact() {
        let (mut cold, cold_patient) = gate_b_recovering_world(false);
        let cold_candidates = cold.medical_entity_candidates;
        let cold_steps = cold.hot_member_steps;
        assert!(cold
            .apply(Command::AdvanceTo { target: 35 })
            .error
            .is_none());
        assert_eq!(cold.medical_entity_candidates - cold_candidates, 5);
        assert_eq!(cold.hot_member_steps - cold_steps, 0);
        assert!(!cold.casualty[&cold_patient].recovering);

        let (mut hot, hot_patient) = gate_b_recovering_world(true);
        let hot_candidates = hot.medical_entity_candidates;
        let hot_steps = hot.hot_member_steps;
        assert!(hot.apply(Command::AdvanceTo { target: 35 }).error.is_none());
        assert_eq!(hot.hot_member_steps - hot_steps, 50);
        assert_eq!(hot.medical_entity_candidates - hot_candidates, 25);
        assert!(!hot.casualty[&hot_patient].recovering);
    }

    fn gate_b_assert_failed_command_preserves_all_authority(
        world: &mut World,
        command: Command,
        error: SimError,
    ) {
        let clock = world.clock;
        let snapshot = world.snapshot();
        let digest = world.state_digest();
        let ids = (world.next_wound_id, world.next_treatment_id);
        let ledgers = (
            world.sourced_food,
            world.sourced_water,
            world.consumed_food,
            world.consumed_water,
            world.lost_food,
            world.lost_water,
            world.sourced_medical,
            world.consumed_medical,
            world.lost_medical,
        );
        let private_indexes = (
            world.living_due.clone(),
            world.due_by_entity.clone(),
            world.treatment_due.clone(),
            world.due_by_treatment.clone(),
            world.active_by_entity.clone(),
            world.treatment_ids_by_entity.clone(),
            world.wound_ids_by_patient.clone(),
            world.bleeding_rate_by_patient.clone(),
            world.medic_index.clone(),
            world.available_medics.clone(),
            world.availability_by_medic.clone(),
        );
        let counters = (
            world.cold_boundaries,
            world.hot_member_steps,
            world.automatic_journal_visits,
            world.automatic_execution_visits,
            world.medical_entity_candidates,
            world.wound_index_visits,
            world.treatment_completion_candidates,
            world.selection_candidates,
        );

        let outcome = world.apply(command);

        assert_eq!(outcome.error, Some(error));
        assert!(outcome.events.is_empty());
        assert_eq!(outcome.clock, clock);
        assert_eq!(world.clock, clock);
        assert_eq!(world.snapshot(), snapshot);
        assert_eq!(world.state_digest(), digest);
        assert_eq!((world.next_wound_id, world.next_treatment_id), ids);
        assert_eq!(
            (
                world.sourced_food,
                world.sourced_water,
                world.consumed_food,
                world.consumed_water,
                world.lost_food,
                world.lost_water,
                world.sourced_medical,
                world.consumed_medical,
                world.lost_medical,
            ),
            ledgers
        );
        assert_eq!(
            (
                world.living_due.clone(),
                world.due_by_entity.clone(),
                world.treatment_due.clone(),
                world.due_by_treatment.clone(),
                world.active_by_entity.clone(),
                world.treatment_ids_by_entity.clone(),
                world.wound_ids_by_patient.clone(),
                world.bleeding_rate_by_patient.clone(),
                world.medic_index.clone(),
                world.available_medics.clone(),
                world.availability_by_medic.clone(),
            ),
            private_indexes
        );
        assert_eq!(
            (
                world.cold_boundaries,
                world.hot_member_steps,
                world.automatic_journal_visits,
                world.automatic_execution_visits,
                world.medical_entity_candidates,
                world.wound_index_visits,
                world.treatment_completion_candidates,
                world.selection_candidates,
            ),
            counters
        );
    }

    fn gate_b_overflow_world(hot: bool) -> World {
        let mut world = World::new(8_700 + u64::from(hot));
        let patient = spawn(&mut world, SoldierSpec::default());
        let wound = WoundId(0);
        world.next_wound_id = 1;
        world.wounds.insert(
            wound,
            Wound {
                id: wound,
                patient,
                created_at: u64::MAX - 1,
                spec: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 0,
                    shock: 1,
                },
                controlled: true,
                healed: false,
            },
        );
        world
            .wound_ids_by_patient
            .insert(patient, BTreeSet::from([wound]));
        world.casualty.insert(
            patient,
            CasualtyState {
                blood: 4_900,
                shock: 100,
                recovering: true,
                recovery_next_at: Some(u64::MAX),
                materialized_at: u64::MAX - 1,
                ..CasualtyState::default()
            },
        );
        world.clock = u64::MAX - 1;
        world.soldiers.living[patient.index()].materialized_at = world.clock;
        world.schedule_due(patient).unwrap();
        if hot {
            assert!(world
                .apply(Command::SetRegionHot { cell: 0, hot: true })
                .error
                .is_none());
        }
        world
    }

    #[test]
    fn gate_b_acceptance_07_world_apply_failures_are_fully_atomic() {
        let mut invalid_wound = World::new(8_701);
        let patient = spawn(&mut invalid_wound, SoldierSpec::default());
        gate_b_assert_failed_command_preserves_all_authority(
            &mut invalid_wound,
            Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 0,
                },
            },
            SimError::InvalidWound,
        );

        let mut stale = World::new(8_702);
        let old = spawn(&mut stale, SoldierSpec::default());
        assert!(stale
            .apply(Command::DespawnSoldier { id: old })
            .error
            .is_none());
        let replacement = spawn(&mut stale, SoldierSpec::default());
        assert_ne!(old, replacement);
        gate_b_assert_failed_command_preserves_all_authority(
            &mut stale,
            Command::InflictWound {
                patient: old,
                wound: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 0,
                    shock: 0,
                },
            },
            SimError::InvalidEntity,
        );

        let mut treatment = World::new(8_703);
        let medic = spawn(
            &mut treatment,
            SoldierSpec {
                role: Role::Medic,
                inventory: Inventory {
                    medical: HEMOSTATIC_COST,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        );
        let patient = spawn(&mut treatment, SoldierSpec::default());
        gate_b_assert_failed_command_preserves_all_authority(
            &mut treatment,
            Command::StartTreatment {
                medic,
                patient,
                wound: Some(WoundId(999)),
                kind: TreatmentKind::Hemostatic,
            },
            SimError::InvalidTreatment,
        );
        gate_b_assert_failed_command_preserves_all_authority(
            &mut treatment,
            Command::RequestTreatment {
                patient,
                wound: Some(WoundId(999)),
                kind: TreatmentKind::Hemostatic,
            },
            SimError::InvalidTreatment,
        );

        for hot in [false, true] {
            let mut overflow = gate_b_overflow_world(hot);
            gate_b_assert_failed_command_preserves_all_authority(
                &mut overflow,
                Command::AdvanceTo { target: u64::MAX },
                SimError::ArithmeticOverflow,
            );
        }
    }

    #[test]
    fn gate_b_acceptance_08_living_death_at_completion_has_role_precedence_and_cleans_indexes() {
        for dying_medic in [true, false] {
            let mut world = World::new(8_800 + u64::from(dying_medic));
            let medic = spawn(
                &mut world,
                SoldierSpec {
                    role: Role::Medic,
                    inventory: Inventory {
                        medical: HEMOSTATIC_COST,
                        ..Inventory::default()
                    },
                    ..SoldierSpec::default()
                },
            );
            let patient = spawn(&mut world, SoldierSpec::default());
            let wound = gate_wound(&mut world, patient, 1, 0);
            let treatment = gate_start(&mut world, medic, patient, wound);
            let dying = if dying_medic { medic } else { patient };
            world.soldiers.living[dying.index()].health = 10;
            world.soldiers.living[dying.index()].thirst = SEVERE_THIRST - 30;
            world.soldiers.living[dying.index()].activity = Activity::March;
            world.soldiers.data[dying.index()].health = 10;
            world.schedule_due(dying).unwrap();
            let reason = if dying_medic {
                InterruptionReason::MedicDied
            } else {
                InterruptionReason::PatientDied
            };

            let outcome = world.apply(Command::AdvanceTo { target: 10 });

            assert_eq!(outcome.error, None);
            assert_eq!(
                outcome.events,
                vec![
                    TimedEvent {
                        at: 10,
                        event: Event::LivingDeteriorated {
                            id: dying,
                            morale_before: 1_000,
                            morale_after: 999,
                            health_before: 10,
                            health_after: 0
                        }
                    },
                    TimedEvent {
                        at: 10,
                        event: Event::SoldierDied {
                            id: dying,
                            cause: DeathCause::Dehydration,
                            health_before: 10
                        }
                    },
                    TimedEvent {
                        at: 10,
                        event: Event::TreatmentInterrupted {
                            id: treatment,
                            reason
                        }
                    },
                    TimedEvent {
                        at: 10,
                        event: Event::TimeAdvanced {
                            from: 0,
                            to: 10,
                            hot_cells_stepped: 0,
                            fixed_steps_per_hot_cell: 0
                        }
                    },
                ]
            );
            assert_eq!(
                world.treatments[&treatment].status,
                TreatmentStatus::Interrupted { at: 10, reason }
            );
            assert!(!world.wounds[&wound].controlled);
            assert_eq!(world.soldiers.data[medic.index()].inventory.medical, 0);
            assert!(!world.active_by_entity.contains_key(&medic));
            assert!(!world.active_by_entity.contains_key(&patient));
            assert!(!world.due_by_treatment.contains_key(&treatment));
            assert!(world
                .treatment_due
                .values()
                .all(|ids| !ids.contains(&treatment)));
            assert!(!world.availability_by_medic.contains_key(&medic));
            assert!(world
                .available_medics
                .values()
                .all(|medics| !medics.contains(&medic)));
            assert_eq!(outcome.events.iter().filter(|event| matches!(event.event, Event::TreatmentInterrupted { id, .. } if id == treatment)).count(), 1);
            assert!(!outcome.events.iter().any(|event| matches!(event.event, Event::TreatmentCompleted { id, .. } if id == treatment)));
            let later = world.apply(Command::AdvanceTo { target: 20 });
            assert!(!later.events.iter().any(|event| matches!(event.event, Event::TreatmentCompleted { id, .. } if id == treatment)));
        }
    }

    #[test]
    fn gate_b_acceptance_08_automatic_incapacity_preserves_audit_and_cleans_indexes() {
        for incapacitated_medic in [true, false] {
            let mut world = World::new(8_810 + u64::from(incapacitated_medic));
            let medic = spawn(
                &mut world,
                SoldierSpec {
                    role: Role::Medic,
                    inventory: Inventory {
                        medical: if incapacitated_medic {
                            HEMOSTATIC_COST
                        } else {
                            HEMOSTATIC_COST + SHOCK_TREATMENT_COST
                        },
                        ..Inventory::default()
                    },
                    ..SoldierSpec::default()
                },
            );
            let patient = spawn(&mut world, SoldierSpec::default());
            let treated_wound = gate_wound(&mut world, patient, 1, 0);
            let treatment = gate_start(&mut world, medic, patient, treated_wound);
            let endpoint = if incapacitated_medic { medic } else { patient };
            gate_wound(&mut world, endpoint, 1_000, 0);

            let interrupted = world.apply(Command::AdvanceTo { target: 4 });
            assert_eq!(interrupted.error, None);
            assert_eq!(
                interrupted
                    .events
                    .iter()
                    .filter(|event| matches!(event.event, Event::TreatmentInterrupted { id, reason: InterruptionReason::Ineligible } if id == treatment))
                    .count(),
                1
            );
            assert_eq!(
                world.treatments[&treatment].status,
                TreatmentStatus::Interrupted {
                    at: 4,
                    reason: InterruptionReason::Ineligible
                }
            );
            assert!(!world.active_by_entity.contains_key(&medic));
            assert!(!world.active_by_entity.contains_key(&patient));
            assert!(!world.due_by_treatment.contains_key(&treatment));
            assert!(world
                .treatment_due
                .values()
                .all(|ids| !ids.contains(&treatment)));
            assert_eq!(
                world.treatment_ids_by_entity.get(&medic),
                Some(&BTreeSet::from([treatment]))
            );
            assert_eq!(
                world.treatment_ids_by_entity.get(&patient),
                Some(&BTreeSet::from([treatment]))
            );

            if incapacitated_medic {
                assert!(!world.availability_by_medic.contains_key(&medic));
                assert!(world
                    .available_medics
                    .values()
                    .all(|medics| !medics.contains(&medic)));
            } else {
                let expected_keys =
                    BTreeSet::from([(0, 0, HEMOSTATIC_COST), (0, 0, SHOCK_TREATMENT_COST)]);
                assert_eq!(world.availability_by_medic[&medic], expected_keys);
                assert_eq!(
                    world.available_medics[&(0, 0, HEMOSTATIC_COST)],
                    BTreeSet::from([medic])
                );
                assert_eq!(
                    world.available_medics[&(0, 0, SHOCK_TREATMENT_COST)],
                    BTreeSet::from([medic])
                );
            }

            let carried = world
                .soldiers
                .data
                .iter()
                .map(|soldier| u128::from(soldier.inventory.medical))
                .sum::<u128>();
            assert_eq!(world.consumed_medical, u128::from(HEMOSTATIC_COST));
            assert_eq!(world.lost_medical, 0);
            assert_eq!(
                world.sourced_medical,
                carried + world.consumed_medical + world.lost_medical
            );

            let stable_treatment = world.treatments[&treatment];
            let stable_active = world.active_by_entity.clone();
            let stable_due = world.due_by_treatment.clone();
            let stable_buckets = world.treatment_due.clone();
            let stable_history = world.treatment_ids_by_entity.clone();
            let stable_available = world.available_medics.clone();
            let stable_availability = world.availability_by_medic.clone();
            let stable_accounting = (
                world.sourced_medical,
                world.consumed_medical,
                world.lost_medical,
            );

            let later = world.apply(Command::AdvanceTo { target: 20 });
            assert!(!later.events.iter().any(|event| matches!(
                event.event,
                Event::TreatmentInterrupted { id, .. }
                    | Event::TreatmentCompleted { id, .. } if id == treatment
            )));
            assert_eq!(world.treatments[&treatment], stable_treatment);
            assert_eq!(world.active_by_entity, stable_active);
            assert_eq!(world.due_by_treatment, stable_due);
            assert_eq!(world.treatment_due, stable_buckets);
            assert_eq!(world.treatment_ids_by_entity, stable_history);
            assert_eq!(world.available_medics, stable_available);
            assert_eq!(world.availability_by_medic, stable_availability);
            assert_eq!(
                (
                    world.sourced_medical,
                    world.consumed_medical,
                    world.lost_medical,
                ),
                stable_accounting
            );
        }
    }

    #[test]
    fn gate_b_acceptance_09_real_medic_churn_visits_only_eligible_candidates() {
        let mut world = World::new(409);
        let availability_keys =
            BTreeSet::from([(0, 0, HEMOSTATIC_COST), (0, 0, SHOCK_TREATMENT_COST)]);
        let medic_spec = |medical| SoldierSpec {
            role: Role::Medic,
            inventory: Inventory {
                medical,
                ..Inventory::default()
            },
            ..SoldierSpec::default()
        };
        let patient = spawn(&mut world, medic_spec(4));
        let marching = spawn(&mut world, medic_spec(4));
        let busy = spawn(&mut world, medic_spec(4));
        let poor = spawn(&mut world, medic_spec(0));
        let recovering = spawn(&mut world, medic_spec(4));
        let dead = spawn(&mut world, medic_spec(4));
        let eligible = spawn(&mut world, medic_spec(4));
        let helper = spawn(&mut world, medic_spec(20));

        assert_eq!(
            world
                .apply(Command::SetActivity {
                    id: marching,
                    activity: Activity::March,
                })
                .error,
            None
        );
        let busy_patient = spawn(&mut world, SoldierSpec::default());
        let busy_wound = gate_wound(&mut world, busy_patient, 1, 0);
        let _busy_treatment = gate_start(&mut world, busy, busy_patient, busy_wound);
        let recovery_wound = gate_wound(&mut world, recovering, 1, 700);
        let dead_wound = world.apply(Command::InflictWound {
            patient: dead,
            wound: WoundSpec {
                trauma: 1000,
                bleeding_per_second: 0,
                shock: 0,
            },
        });
        assert_eq!(dead_wound.error, None);
        assert!(matches!(
            world.soldiers.living[dead.index()].life,
            LifeState::Dead {
                cause: DeathCause::ImmediateTrauma,
                ..
            }
        ));
        let mut other_faction = BTreeSet::new();
        let mut other_cell = BTreeSet::new();
        for i in 0..2_000 {
            let id = spawn(
                &mut world,
                SoldierSpec {
                    faction: if i % 2 == 0 { 1 } else { 0 },
                    position: Position {
                        cell: if i % 2 == 0 { 0 } else { 99 },
                        ..Position::default()
                    },
                    role: Role::Medic,
                    inventory: Inventory {
                        medical: 10,
                        ..Inventory::default()
                    },
                    ..SoldierSpec::default()
                },
            );
            if i % 2 == 0 {
                other_faction.insert(id);
            } else {
                other_cell.insert(id);
            }
        }

        let requested_cohort = BTreeSet::from([
            patient, marching, busy, poor, recovering, dead, eligible, helper,
        ]);
        assert_eq!(world.medic_index[&(0, 0)], requested_cohort);
        assert_eq!(world.medic_index[&(1, 0)], other_faction);
        assert_eq!(world.medic_index[&(0, 99)], other_cell);
        assert_eq!(world.medic_index.len(), 3);
        assert_eq!(world.medic_index[&(1, 0)].len(), 1_000);
        assert_eq!(world.medic_index[&(0, 99)].len(), 1_000);
        for cost in [HEMOSTATIC_COST, SHOCK_TREATMENT_COST] {
            assert_eq!(world.available_medics[&(1, 0, cost)], other_faction);
            assert_eq!(world.available_medics[&(0, 99, cost)], other_cell);
        }
        for id in [marching, busy, poor, recovering, dead] {
            assert!(!world.availability_by_medic.contains_key(&id));
        }
        for id in [patient, eligible, helper] {
            assert_eq!(world.availability_by_medic[&id], availability_keys);
        }
        assert_eq!(
            world.available_medics[&(0, 0, HEMOSTATIC_COST)],
            BTreeSet::from([patient, eligible, helper])
        );
        assert_eq!(
            world.available_medics[&(0, 0, SHOCK_TREATMENT_COST)],
            BTreeSet::from([patient, eligible, helper])
        );

        let patient_wound = gate_wound(&mut world, patient, 1, 0);
        assert_eq!(
            world.available_medics[&(0, 0, HEMOSTATIC_COST)],
            BTreeSet::from([patient, eligible, helper])
        );
        let before = world.selection_candidates;
        let selected = world.apply(Command::RequestTreatment {
            patient,
            wound: Some(patient_wound),
            kind: TreatmentKind::Hemostatic,
        });
        assert_eq!(selected.error, None);
        assert_eq!(
            selected.events,
            vec![TimedEvent {
                at: 0,
                event: Event::TreatmentStarted {
                    id: TreatmentId(1),
                    medic: eligible,
                    patient,
                    wound: Some(patient_wound),
                    kind: TreatmentKind::Hemostatic,
                    completes_at: 10,
                    consumed: HEMOSTATIC_COST,
                },
            }]
        );
        assert_eq!(world.selection_candidates - before, 2); // self, then the sole eligible medic
        assert_eq!(world.consumed_medical, 2);
        assert_eq!(world.soldiers.data[eligible.index()].inventory.medical, 3);
        assert_eq!(
            world.resource_totals(),
            ResourceTotals {
                carried_medical: 20_042,
                sourced_medical: 20_044,
                consumed_medical: 2,
                ..ResourceTotals::default()
            }
        );

        assert_eq!(
            world
                .apply(Command::InterruptTreatment { id: TreatmentId(1) })
                .error,
            None
        );
        assert_eq!(
            world
                .apply(Command::StartTreatment {
                    medic: helper,
                    patient: recovering,
                    wound: Some(recovery_wound),
                    kind: TreatmentKind::Hemostatic,
                })
                .error,
            None
        );
        let recovery = world.apply(Command::AdvanceTo { target: 15 });
        assert_eq!(recovery.error, None);
        assert_eq!(
            recovery.events,
            vec![
                TimedEvent {
                    at: 10,
                    event: Event::TreatmentCompleted {
                        id: TreatmentId(0),
                        medic: busy,
                        patient: busy_patient,
                        kind: TreatmentKind::Hemostatic
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::RecoveryChanged {
                        id: busy_patient,
                        before: false,
                        after: true,
                        next_at: Some(15)
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::TreatmentCompleted {
                        id: TreatmentId(2),
                        medic: helper,
                        patient: recovering,
                        kind: TreatmentKind::Hemostatic
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::RecoveryChanged {
                        id: recovering,
                        before: false,
                        after: true,
                        next_at: Some(15)
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryTicked {
                        id: recovering,
                        blood_before: 4_990,
                        blood_after: 5_000,
                        shock_before: 701,
                        shock_after: 651,
                        health_before: 1_000,
                        health_after: 1_000
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryTicked {
                        id: busy_patient,
                        blood_before: 4_990,
                        blood_after: 5_000,
                        shock_before: 1,
                        shock_after: 0,
                        health_before: 1_000,
                        health_after: 1_000
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::WoundHealed {
                        id: busy_wound,
                        patient: busy_patient
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryChanged {
                        id: busy_patient,
                        before: true,
                        after: false,
                        next_at: None
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 15,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                },
            ]
        );
        assert!(!world.casualty[&recovering].incapacitated);
        assert_eq!(world.casualty[&recovering].blood, 5_000);
        assert_eq!(world.casualty[&recovering].shock, 651);
        assert!(world.casualty[&recovering].recovering);
        assert_eq!(world.casualty[&recovering].recovery_next_at, Some(20));
        assert_eq!(
            world
                .apply(Command::SetActivity {
                    id: busy,
                    activity: Activity::March,
                })
                .error,
            None
        );
        assert_eq!(world.medic_index[&(0, 0)], requested_cohort);
        assert_eq!(world.medic_index[&(1, 0)], other_faction);
        assert_eq!(world.medic_index[&(0, 99)], other_cell);
        for cost in [HEMOSTATIC_COST, SHOCK_TREATMENT_COST] {
            assert_eq!(world.available_medics[&(1, 0, cost)], other_faction);
            assert_eq!(world.available_medics[&(0, 99, cost)], other_cell);
        }
        for id in [marching, busy, poor, dead] {
            assert!(!world.availability_by_medic.contains_key(&id));
        }
        for id in [patient, recovering, eligible, helper] {
            assert_eq!(world.availability_by_medic[&id], availability_keys);
        }
        assert_eq!(
            world.available_medics[&(0, 0, HEMOSTATIC_COST)],
            BTreeSet::from([patient, recovering, eligible, helper])
        );

        let second_wound = gate_wound(&mut world, patient, 1, 0);
        assert_eq!(
            world.available_medics[&(0, 0, HEMOSTATIC_COST)],
            BTreeSet::from([patient, recovering, eligible, helper])
        );
        let before = world.selection_candidates;
        let recovered_selected = world.apply(Command::RequestTreatment {
            patient,
            wound: Some(second_wound),
            kind: TreatmentKind::Hemostatic,
        });
        assert_eq!(recovered_selected.error, None);
        assert_eq!(
            recovered_selected.events,
            vec![TimedEvent {
                at: 15,
                event: Event::TreatmentStarted {
                    id: TreatmentId(3),
                    medic: recovering,
                    patient,
                    wound: Some(second_wound),
                    kind: TreatmentKind::Hemostatic,
                    completes_at: 25,
                    consumed: HEMOSTATIC_COST
                }
            }]
        );
        assert_eq!(world.selection_candidates - before, 2);
        assert_eq!(world.soldiers.data[recovering.index()].inventory.medical, 3);
        assert_eq!(
            world.resource_totals(),
            ResourceTotals {
                carried_medical: 20_040,
                sourced_medical: 20_044,
                consumed_medical: 4,
                ..ResourceTotals::default()
            }
        );
        let active = world.active_by_entity[&patient];
        assert_eq!(
            world
                .apply(Command::InterruptTreatment { id: active })
                .error,
            None
        );

        let removed = world.apply(Command::DespawnSoldier { id: recovering });
        assert_eq!(removed.error, None);
        assert!(!world.medic_index[&(0, 0)].contains(&recovering));
        assert!(!world.availability_by_medic.contains_key(&recovering));
        assert!(world
            .available_medics
            .values()
            .all(|ids| !ids.contains(&recovering)));
        let replacement = spawn(&mut world, medic_spec(4));
        assert_eq!(replacement.index(), recovering.index());
        assert_eq!(replacement.generation(), recovering.generation() + 1);
        assert_eq!(world.availability_by_medic[&replacement], availability_keys);
        assert_eq!(
            world.medic_index[&(0, 0)],
            BTreeSet::from([
                patient,
                marching,
                busy,
                poor,
                replacement,
                dead,
                eligible,
                helper
            ])
        );
        assert!(world
            .available_medics
            .values()
            .all(|ids| !ids.contains(&recovering)));
        for id in [marching, busy, poor, dead] {
            assert!(!world.availability_by_medic.contains_key(&id));
        }
        for id in [patient, replacement, eligible, helper] {
            assert_eq!(world.availability_by_medic[&id], availability_keys);
        }
        for cost in [HEMOSTATIC_COST, SHOCK_TREATMENT_COST] {
            assert_eq!(
                world.available_medics[&(0, 0, cost)],
                BTreeSet::from([patient, replacement, eligible, helper])
            );
            assert_eq!(world.available_medics[&(1, 0, cost)], other_faction);
            assert_eq!(world.available_medics[&(0, 99, cost)], other_cell);
        }
        let third_wound = gate_wound(&mut world, patient, 1, 0);
        assert_eq!(
            world.available_medics[&(0, 0, HEMOSTATIC_COST)],
            BTreeSet::from([patient, replacement, eligible, helper])
        );
        let before = world.selection_candidates;
        let reused_selected = world.apply(Command::RequestTreatment {
            patient,
            wound: Some(third_wound),
            kind: TreatmentKind::Hemostatic,
        });
        assert_eq!(reused_selected.error, None);
        assert_eq!(
            reused_selected.events,
            vec![TimedEvent {
                at: 15,
                event: Event::TreatmentStarted {
                    id: TreatmentId(4),
                    medic: eligible,
                    patient,
                    wound: Some(third_wound),
                    kind: TreatmentKind::Hemostatic,
                    completes_at: 25,
                    consumed: HEMOSTATIC_COST
                }
            }]
        );
        assert_eq!(world.selection_candidates - before, 2);
        assert_eq!(world.soldiers.data[eligible.index()].inventory.medical, 2);
        assert_eq!(
            world.resource_totals(),
            ResourceTotals {
                carried_medical: 20_040,
                sourced_medical: 20_048,
                consumed_medical: 5,
                lost_medical: 3,
                ..ResourceTotals::default()
            }
        );
        assert!(world
            .available_medics
            .values()
            .all(|ids| !ids.contains(&recovering)));
        assert!(!world.treatment_ids_by_entity.contains_key(&recovering));
        assert_eq!(poor.index(), 3);
    }

    #[test]
    fn gate_b_acceptance_10_materialized_removal_cleans_both_endpoint_roles() {
        for remove_medic in [true, false] {
            let mut world = World::new(if remove_medic { 410 } else { 411 });
            assert_eq!(
                world.apply(Command::CreateSquad { id: 77 }).events,
                vec![TimedEvent {
                    at: 0,
                    event: Event::SquadCreated { id: 77 }
                }]
            );
            let medic = spawn(
                &mut world,
                SoldierSpec {
                    squad: Some(77),
                    role: Role::Medic,
                    inventory: Inventory {
                        food: 7,
                        water: 8,
                        medical: 5,
                    },
                    ..SoldierSpec::default()
                },
            );
            let patient = spawn(
                &mut world,
                SoldierSpec {
                    squad: Some(77),
                    inventory: Inventory {
                        food: 11,
                        water: 12,
                        medical: 2,
                    },
                    ..SoldierSpec::default()
                },
            );
            let medic_wound = gate_wound(&mut world, medic, 1, 0);
            let patient_wound = gate_wound(&mut world, patient, 2, 0);
            let treatment = gate_start(&mut world, medic, patient, patient_wound);
            assert_eq!(world.apply(Command::AdvanceTo { target: 3 }).error, None);
            assert_eq!(world.clock, 3);
            assert_eq!(world.casualty[&medic].materialized_at, 0);
            assert_eq!(world.casualty[&patient].materialized_at, 0);
            assert_eq!(world.squads[&77].members, BTreeSet::from([medic, patient]));
            assert_eq!(world.cell_members[&0], BTreeSet::from([medic, patient]));

            let removed = if remove_medic { medic } else { patient };
            let survivor = if remove_medic { patient } else { medic };
            let reason = if remove_medic {
                InterruptionReason::MedicRemoved
            } else {
                InterruptionReason::PatientRemoved
            };
            let expected_loadout = if remove_medic {
                Loadout {
                    ammunition: 0,
                    food: 7,
                    water: 8,
                    medical: 4,
                }
            } else {
                Loadout {
                    ammunition: 0,
                    food: 11,
                    water: 12,
                    medical: 2,
                }
            };
            let outcome = world.apply(Command::DespawnSoldier { id: removed });
            assert_eq!(outcome.error, None);
            assert_eq!(
                outcome.events,
                vec![
                    TimedEvent {
                        at: 3,
                        event: Event::TreatmentInterrupted {
                            id: treatment,
                            reason,
                        },
                    },
                    TimedEvent {
                        at: 3,
                        event: Event::SoldierRemoved {
                            id: removed,
                            loadout: expected_loadout,
                        },
                    },
                ]
            );
            assert_eq!(world.soldiers.living[removed.index()].materialized_at, 3);
            assert_eq!(world.soldiers.living[removed.index()].health, 1_000);
            assert_eq!(
                world.soldiers.data[removed.index()].inventory,
                Inventory {
                    food: expected_loadout.food,
                    water: expected_loadout.water,
                    medical: expected_loadout.medical
                }
            );
            assert!(!world.soldiers.valid(removed));
            assert!(world.soldiers.valid(survivor));
            assert_eq!(world.squads[&77].members, BTreeSet::from([survivor]));
            assert_eq!(world.cell_members[&0], BTreeSet::from([survivor]));
            assert!(!world.casualty.contains_key(&removed));
            assert!(!world.wound_ids_by_patient.contains_key(&removed));
            assert!(!world.bleeding_rate_by_patient.contains_key(&removed));
            assert!(!world.due_by_entity.contains_key(&removed));
            assert!(!world.active_by_entity.contains_key(&removed));
            assert!(!world.active_by_entity.contains_key(&survivor));
            assert!(!world.treatments.contains_key(&treatment));
            assert!(!world.due_by_treatment.contains_key(&treatment));
            assert!(world
                .treatment_due
                .values()
                .all(|ids| !ids.contains(&treatment)));
            assert!(!world.treatment_ids_by_entity.contains_key(&removed));
            assert!(!world.treatment_ids_by_entity.contains_key(&survivor));
            assert!(!world.availability_by_medic.contains_key(&removed));
            if remove_medic {
                assert_eq!(world.medic_index.get(&(0, 0)), None);
                assert!(!world.availability_by_medic.contains_key(&survivor));
            } else {
                assert_eq!(world.medic_index[&(0, 0)], BTreeSet::from([survivor]));
                let keys = BTreeSet::from([(0, 0, HEMOSTATIC_COST), (0, 0, SHOCK_TREATMENT_COST)]);
                assert_eq!(world.availability_by_medic[&survivor], keys);
                for key in keys {
                    assert_eq!(world.available_medics[&key], BTreeSet::from([survivor]));
                }
            }
            assert!(world
                .available_medics
                .values()
                .all(|ids| !ids.contains(&removed)));
            assert!(world
                .cell_members
                .values()
                .all(|ids| !ids.contains(&removed)));
            assert!(!world.wounds.contains_key(&if remove_medic {
                medic_wound
            } else {
                patient_wound
            }));
            assert_eq!(world.sourced_medical, 7);
            assert_eq!(world.consumed_medical, 1);
            assert_eq!(world.lost_medical, u128::from(expected_loadout.medical));
            let carried_medical: u128 = world
                .soldiers
                .data
                .iter()
                .enumerate()
                .filter(|(index, _)| world.soldiers.alive[*index])
                .map(|(_, spec)| u128::from(spec.inventory.medical))
                .sum();
            assert_eq!(
                world.sourced_medical,
                carried_medical + world.consumed_medical + world.lost_medical
            );
            assert_eq!(world.lost_food, u128::from(expected_loadout.food));
            assert_eq!(world.lost_water, u128::from(expected_loadout.water));
            let (carried_food, carried_water) = world
                .soldiers
                .data
                .iter()
                .enumerate()
                .filter(|(index, _)| world.soldiers.alive[*index])
                .fold((0_u128, 0_u128), |(food, water), (_, spec)| {
                    (
                        food + u128::from(spec.inventory.food),
                        water + u128::from(spec.inventory.water),
                    )
                });
            assert_eq!(world.sourced_food, 18);
            assert_eq!(world.sourced_water, 20);
            assert_eq!(
                world.sourced_food,
                carried_food + world.consumed_food + world.lost_food
            );
            assert_eq!(
                world.sourced_water,
                carried_water + world.consumed_water + world.lost_water
            );

            let survivor_wound = if remove_medic {
                patient_wound
            } else {
                medic_wound
            };
            assert_eq!(
                world.wound_ids_by_patient[&survivor],
                BTreeSet::from([survivor_wound])
            );
            assert_eq!(world.wounds[&survivor_wound].patient, survivor);
            assert_eq!(
                world.wounds[&survivor_wound].spec.bleeding_per_second,
                if remove_medic { 2 } else { 1 }
            );
            assert!(!world.wounds[&survivor_wound].controlled);
            assert!(!world.wounds[&survivor_wound].healed);
            assert_eq!(
                world.bleeding_rate_by_patient[&survivor],
                if remove_medic { 2 } else { 1 }
            );
            assert_eq!(world.casualty[&survivor].materialized_at, 0);
            assert_eq!(world.casualty[&survivor].blood, 5_000);
            assert_eq!(world.casualty[&survivor].shock, 0);
            assert!(!world.casualty[&survivor].incapacitated);
            assert!(!world.casualty[&survivor].recovering);
            assert_eq!(world.casualty[&survivor].recovery_next_at, None);
            assert_eq!(
                world.soldiers.living[survivor.index()].life,
                LifeState::Alive
            );
            assert_eq!(
                world.soldiers.living[survivor.index()].activity,
                Activity::Idle
            );
            assert_eq!(world.due_by_entity[&survivor], 50);
            assert!(world.living_due[&50].contains(&survivor));
            assert_eq!(world.treatment_ids_by_entity.get(&survivor), None);

            let after_deadline = world.apply(Command::AdvanceTo { target: 20 });
            assert_eq!(after_deadline.error, None);
            assert_eq!(
                after_deadline.events,
                vec![TimedEvent {
                    at: 20,
                    event: Event::TimeAdvanced {
                        from: 3,
                        to: 20,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                }]
            );
            assert!(world.treatments.is_empty());
            assert!(world.treatment_due.values().all(BTreeSet::is_empty));
            assert!(world.due_by_treatment.is_empty());
            assert!(world.active_by_entity.is_empty());
            assert!(world.treatment_ids_by_entity.is_empty());
            assert!(world.living_due.values().all(|ids| !ids.contains(&removed)));
            assert!(!world.due_by_entity.contains_key(&removed));

            let replacement = spawn(&mut world, SoldierSpec::default());
            assert_eq!(replacement.index(), removed.index());
            assert_eq!(replacement.generation(), removed.generation() + 1);
            assert!(!world.casualty.contains_key(&replacement));
            assert!(!world.wound_ids_by_patient.contains_key(&replacement));
            assert!(!world.active_by_entity.contains_key(&replacement));
            assert!(!world.treatment_ids_by_entity.contains_key(&replacement));
            assert_eq!(world.squads[&77].members, BTreeSet::from([survivor]));
            assert_eq!(
                world.cell_members[&0],
                BTreeSet::from([survivor, replacement])
            );
            assert!(world.cell_members[&0].contains(&replacement));
            assert!(!world.cell_members[&0].contains(&removed));
            let stable_digest = world.state_digest();
            let before = world.snapshot();
            let stable_allocator = (
                world.soldiers.generation.clone(),
                world.soldiers.alive.clone(),
                world.soldiers.free.clone(),
                world.soldiers.live,
            );
            let stable_private = (
                (
                    world.medic_index.clone(),
                    world.available_medics.clone(),
                    world.availability_by_medic.clone(),
                    world.living_due.clone(),
                    world.due_by_entity.clone(),
                    world.treatment_due.clone(),
                    world.due_by_treatment.clone(),
                ),
                (
                    world.active_by_entity.clone(),
                    world.treatment_ids_by_entity.clone(),
                    world.wound_ids_by_patient.clone(),
                    world.bleeding_rate_by_patient.clone(),
                    world.cell_members.clone(),
                    world.squads.clone(),
                ),
            );
            let stable_scalars = (
                (
                    world.next_wound_id,
                    world.next_treatment_id,
                    world.sourced_food,
                    world.sourced_water,
                    world.consumed_food,
                    world.consumed_water,
                    world.lost_food,
                    world.lost_water,
                    world.sourced_medical,
                    world.consumed_medical,
                ),
                (
                    world.lost_medical,
                    world.hot_member_steps,
                    world.cold_boundaries,
                    world.automatic_journal_visits,
                    world.automatic_execution_visits,
                    world.medical_entity_candidates,
                    world.wound_index_visits,
                    world.treatment_completion_candidates,
                    world.selection_candidates,
                ),
            );
            let stale = world.apply(Command::DespawnSoldier { id: removed });
            assert_eq!(stale.error, Some(SimError::InvalidEntity));
            assert!(stale.events.is_empty());
            assert_eq!(world.snapshot(), before);
            assert_eq!(world.state_digest(), stable_digest);
            assert_eq!(
                stable_allocator,
                (
                    world.soldiers.generation.clone(),
                    world.soldiers.alive.clone(),
                    world.soldiers.free.clone(),
                    world.soldiers.live,
                )
            );
            assert_eq!(
                stable_private,
                (
                    (
                        world.medic_index.clone(),
                        world.available_medics.clone(),
                        world.availability_by_medic.clone(),
                        world.living_due.clone(),
                        world.due_by_entity.clone(),
                        world.treatment_due.clone(),
                        world.due_by_treatment.clone(),
                    ),
                    (
                        world.active_by_entity.clone(),
                        world.treatment_ids_by_entity.clone(),
                        world.wound_ids_by_patient.clone(),
                        world.bleeding_rate_by_patient.clone(),
                        world.cell_members.clone(),
                        world.squads.clone(),
                    ),
                )
            );
            assert_eq!(
                stable_scalars,
                (
                    (
                        world.next_wound_id,
                        world.next_treatment_id,
                        world.sourced_food,
                        world.sourced_water,
                        world.consumed_food,
                        world.consumed_water,
                        world.lost_food,
                        world.lost_water,
                        world.sourced_medical,
                        world.consumed_medical,
                    ),
                    (
                        world.lost_medical,
                        world.hot_member_steps,
                        world.cold_boundaries,
                        world.automatic_journal_visits,
                        world.automatic_execution_visits,
                        world.medical_entity_candidates,
                        world.wound_index_visits,
                        world.treatment_completion_candidates,
                        world.selection_candidates,
                    ),
                )
            );
        }
    }
}
