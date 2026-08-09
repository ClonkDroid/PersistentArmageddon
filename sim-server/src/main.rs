use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use sim_core::{
    Activity, BlockedCommand, Command, DeathCause, Event, InterruptionReason, ScheduledCommand,
    SimError, SoldierSpec, Stock, TimedEvent, TreatmentId, TreatmentKind, World, WoundId,
    WoundSpec,
};
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const MAX_REQUEST: usize = 64 * 1024;
const MAX_HEADER: usize = 16 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONSECUTIVE_ACCEPT_ERRORS: u32 = 3;
const ACCEPT_BACKOFF: Duration = Duration::from_millis(25);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().any(|a| a == "--benchmark") {
        return benchmark();
    }
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    let mut world = World::new(1);
    eprintln!("sim-server listening on http://127.0.0.1:8080");
    serve_listener(
        &listener,
        &mut world,
        CONNECTION_TIMEOUT,
        None,
        serve_connection,
    )?;
    Ok(())
}

fn serve_listener<F>(
    listener: &TcpListener,
    world: &mut World,
    timeout: Duration,
    limit: Option<usize>,
    mut handler: F,
) -> std::io::Result<()>
where
    F: FnMut(TcpStream, &mut World, Instant) -> std::io::Result<()>,
{
    serve_listener_with(listener, world, timeout, limit, &mut handler, thread::sleep)
}

fn serve_listener_with<F, B>(
    listener: &TcpListener,
    world: &mut World,
    timeout: Duration,
    limit: Option<usize>,
    handler: &mut F,
    mut backoff: B,
) -> std::io::Result<()>
where
    F: FnMut(TcpStream, &mut World, Instant) -> std::io::Result<()>,
    B: FnMut(Duration),
{
    let mut accepted = 0;
    let mut consecutive_errors = 0;
    while limit.is_none_or(|limit| accepted < limit) {
        match listener.accept() {
            Ok((stream, _)) => {
                consecutive_errors = 0;
                accepted += 1;
                if let Err(error) = handler(stream, world, connection_deadline(timeout)) {
                    eprintln!("contained connection failure: {error}");
                }
            }
            Err(error) if retryable_accept_error(&error) => {
                let Some(delay) = retry_accept(&mut consecutive_errors) else {
                    return Err(error);
                };
                eprintln!("retryable accept failure {consecutive_errors}: {error}");
                backoff(delay);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn retryable_accept_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::ConnectionAborted
    )
}

fn retry_accept(consecutive_errors: &mut u32) -> Option<Duration> {
    *consecutive_errors += 1;
    (*consecutive_errors < MAX_CONSECUTIVE_ACCEPT_ERRORS)
        .then(|| ACCEPT_BACKOFF.saturating_mul(*consecutive_errors))
}

fn serve_connection(
    mut stream: TcpStream,
    world: &mut World,
    deadline: Instant,
) -> std::io::Result<()> {
    let (status, kind, body) = read_and_handle_deadline(&mut stream, world, deadline);
    let header = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
    write_all_deadline(&mut stream, header.as_bytes(), deadline)?;
    write_all_deadline(&mut stream, &body, deadline)
}

#[cfg(test)]
fn read_and_handle(r: &mut impl Read, w: &mut World) -> (&'static str, &'static str, Vec<u8>) {
    read_and_handle_with(r, w, || Ok(()))
}

fn read_and_handle_deadline(
    r: &mut TcpStream,
    w: &mut World,
    deadline: Instant,
) -> (&'static str, &'static str, Vec<u8>) {
    let stream = r.try_clone();
    match stream {
        Ok(stream) => read_and_handle_with(r, w, || set_remaining_read(&stream, deadline)),
        Err(_) => bad(),
    }
}

fn read_and_handle_with(
    r: &mut impl Read,
    w: &mut World,
    mut before_read: impl FnMut() -> std::io::Result<()>,
) -> (&'static str, &'static str, Vec<u8>) {
    let mut header = Vec::new();
    let mut byte = [0];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() == MAX_HEADER {
            return too_large();
        }
        if before_read().is_err() {
            return timeout_response();
        }
        match r.read(&mut byte) {
            Ok(1) => header.push(byte[0]),
            Err(error) if is_timeout(&error) => return timeout_response(),
            _ => return bad(),
        }
    }
    let head = match std::str::from_utf8(&header[..header.len() - 4]) {
        Ok(h) => h,
        Err(_) => return bad(),
    };
    let mut lines = head.split("\r\n");
    let request = match lines.next() {
        Some(x) => x,
        None => return bad(),
    };
    let mut length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return bad();
        };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return bad();
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return bad();
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some()
                || value.trim().is_empty()
                || !value.trim().bytes().all(|b| b.is_ascii_digit())
            {
                return bad();
            }
            length = value.trim().parse::<usize>().ok();
            if length.is_none() {
                return bad();
            }
        }
    }
    if request == "GET /health HTTP/1.1" {
        if length.unwrap_or(0) != 0 {
            return bad();
        }
        return (
            "200 OK",
            "application/json",
            format!(
                "{{\"status\":\"ok\",\"clock\":{},\"soldiers\":{},\"digest\":\"{:016x}\"}}",
                w.clock(),
                w.soldier_count(),
                w.state_digest()
            )
            .into_bytes(),
        );
    }
    if request == "GET /snapshot HTTP/1.1" {
        if length.unwrap_or(0) != 0 {
            return bad();
        }
        return ("200 OK", "application/octet-stream", w.snapshot());
    }
    if request != "POST /v1/command HTTP/1.1" {
        return ("404 Not Found", "text/plain", b"not found\n".to_vec());
    }
    let Some(declared) = length else { return bad() };
    if declared > MAX_REQUEST {
        return too_large();
    }
    let mut body = vec![0; declared];
    let mut offset = 0;
    while offset < declared {
        if before_read().is_err() {
            return timeout_response();
        }
        match r.read(&mut body[offset..]) {
            Ok(0) => return bad(),
            Ok(n) => offset += n,
            Err(error) if is_timeout(&error) => return timeout_response(),
            Err(_) => return bad(),
        }
    }
    let wire: Wire = match serde_json::from_slice(&body) {
        Ok(x) => x,
        Err(_) => return bad(),
    };
    let command = match wire.into_command() {
        Ok(x) => x,
        Err(_) => return bad(),
    };
    let o = w.apply(command);
    (
        "200 OK",
        "application/json",
        outcome_json(
            o.clock,
            &o.events,
            o.error.as_ref(),
            o.blocked,
            w.state_digest(),
        )
        .into_bytes(),
    )
}
fn connection_deadline(timeout: Duration) -> Instant {
    Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now)
}
fn remaining(deadline: Instant) -> std::io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "connection deadline expired")
        })
}
fn set_remaining_read(stream: &TcpStream, deadline: Instant) -> std::io::Result<()> {
    stream.set_read_timeout(Some(remaining(deadline)?))
}

trait DeadlineWriter {
    fn now(&self) -> Instant;
    fn set_timeout(&mut self, timeout: Duration) -> std::io::Result<()>;
    fn write_chunk(&mut self, bytes: &[u8]) -> std::io::Result<usize>;
}

impl DeadlineWriter for TcpStream {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn set_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
        self.set_write_timeout(Some(timeout))
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.write(bytes)
    }
}

fn write_all_deadline(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
) -> std::io::Result<()> {
    write_all_deadline_with(stream, bytes, deadline)
}

fn write_all_deadline_with<W: DeadlineWriter>(
    stream: &mut W,
    mut bytes: &[u8],
    deadline: Instant,
) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let timeout = deadline
            .checked_duration_since(stream.now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "connection deadline expired")
            })?;
        stream.set_timeout(timeout)?;
        match stream.write_chunk(bytes) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "failed to write response",
                ))
            }
            Ok(n) => bytes = &bytes[n..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}
fn timeout_response() -> (&'static str, &'static str, Vec<u8>) {
    (
        "408 Request Timeout",
        "application/json",
        b"{\"error\":\"connection_deadline_expired\"}".to_vec(),
    )
}
fn too_large() -> (&'static str, &'static str, Vec<u8>) {
    (
        "413 Payload Too Large",
        "application/json",
        b"{\"error\":\"request_too_large\"}".to_vec(),
    )
}
fn bad() -> (&'static str, &'static str, Vec<u8>) {
    (
        "400 Bad Request",
        "application/json",
        b"{\"error\":\"malformed_or_unsupported\"}".to_vec(),
    )
}

struct Wire {
    fields: BTreeMap<String, Value>,
}
impl<'de> Deserialize<'de> for Wire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WireVisitor;
        impl<'de> Visitor<'de> for WireVisitor {
            type Value = Wire;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a command object with unique keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Wire, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if !known_wire_field(&key) {
                        return Err(de::Error::unknown_field(&key, WIRE_FIELDS));
                    }
                    if fields.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format_args!("duplicate field {key}")));
                    }
                }
                Ok(Wire { fields })
            }
        }
        deserializer.deserialize_map(WireVisitor)
    }
}
impl Wire {
    fn into_command(mut self) -> Result<Command, ()> {
        validate_legacy_wire_types(&self.fields)?;
        if take_u64(&mut self.fields, "version")? != 1 {
            return Err(());
        }
        let command = take_string(&mut self.fields, "command")?;
        let strict_medical = matches!(
            command.as_str(),
            "inflict_wound" | "start_treatment" | "request_treatment" | "interrupt_treatment"
        );
        let parsed = match command.as_str() {
            "advance_to" => Ok(Command::AdvanceTo {
                target: take_u64(&mut self.fields, "target")?,
            }),
            "create_stockpile" => Ok(Command::CreateStockpile {
                id: take_u32(&mut self.fields, "id")?,
                initial: Stock {
                    ammunition: take_u64(&mut self.fields, "ammunition")?,
                    supplies: take_u64(&mut self.fields, "supplies")?,
                },
            }),
            "transfer" => Ok(Command::Transfer {
                from: take_u32(&mut self.fields, "from")?,
                to: take_u32(&mut self.fields, "to")?,
                ammunition: take_u64(&mut self.fields, "ammunition")?,
                supplies: take_u64(&mut self.fields, "supplies")?,
            }),
            "set_region_hot" => Ok(Command::SetRegionHot {
                cell: take_u32(&mut self.fields, "cell")?,
                hot: take_bool(&mut self.fields, "hot")?,
            }),
            "schedule_hot" => Ok(Command::Schedule {
                at: take_u64(&mut self.fields, "at")?,
                command: ScheduledCommand::SetRegionHot {
                    cell: take_u32(&mut self.fields, "cell")?,
                    hot: take_bool(&mut self.fields, "hot")?,
                },
            }),
            "schedule_transfer" => Ok(Command::Schedule {
                at: take_u64(&mut self.fields, "at")?,
                command: ScheduledCommand::Transfer {
                    from: take_u32(&mut self.fields, "from")?,
                    to: take_u32(&mut self.fields, "to")?,
                    ammunition: take_u64(&mut self.fields, "ammunition")?,
                    supplies: take_u64(&mut self.fields, "supplies")?,
                },
            }),
            "cancel_scheduled" => Ok(Command::CancelScheduled {
                id: take_u64(&mut self.fields, "id")?,
            }),
            "set_activity" => Ok(Command::SetActivity {
                id: sim_core::EntityId::from_raw(take_u64(&mut self.fields, "id")?),
                activity: match take_string(&mut self.fields, "activity")?.as_str() {
                    "rest" => Activity::Rest,
                    "idle" => Activity::Idle,
                    "march" => Activity::March,
                    _ => return Err(()),
                },
            }),
            "inflict_wound" => Ok(Command::InflictWound {
                patient: sim_core::EntityId::from_raw(take_u64(&mut self.fields, "patient")?),
                wound: WoundSpec {
                    trauma: take_u16(&mut self.fields, "trauma")?,
                    bleeding_per_second: take_u16(&mut self.fields, "bleeding_per_second")?,
                    shock: take_u16(&mut self.fields, "shock")?,
                },
            }),
            "start_treatment" => {
                let medic = sim_core::EntityId::from_raw(take_u64(&mut self.fields, "medic")?);
                let patient = sim_core::EntityId::from_raw(take_u64(&mut self.fields, "patient")?);
                let kind = parse_kind(&take_string(&mut self.fields, "kind")?)?;
                let wound = treatment_wound(&mut self.fields, kind)?;
                Ok(Command::StartTreatment {
                    medic,
                    patient,
                    wound,
                    kind,
                })
            }
            "request_treatment" => {
                let patient = sim_core::EntityId::from_raw(take_u64(&mut self.fields, "patient")?);
                let kind = parse_kind(&take_string(&mut self.fields, "kind")?)?;
                let wound = treatment_wound(&mut self.fields, kind)?;
                Ok(Command::RequestTreatment {
                    patient,
                    wound,
                    kind,
                })
            }
            "interrupt_treatment" => Ok(Command::InterruptTreatment {
                id: TreatmentId(take_u64(&mut self.fields, "treatment")?),
            }),
            _ => Err(()),
        }?;
        (!strict_medical || self.fields.is_empty())
            .then_some(parsed)
            .ok_or(())
    }
}
// The pre-Gate-C2 wire type was a typed struct whose optional fields were
// deserialized before command selection.  Keep that observable behaviour for
// legacy requests: null remains a valid absent optional value, but every
// present recognized value must have the historical scalar type even when the
// selected legacy command does not consume it.  Medical commands additionally
// enforce their exact key sets below.
fn validate_legacy_wire_types(fields: &BTreeMap<String, Value>) -> Result<(), ()> {
    for (key, value) in fields {
        if value.is_null() {
            continue;
        }
        let valid = match key.as_str() {
            "version" | "from" | "to" | "cell" => value
                .as_u64()
                .is_some_and(|value| u32::try_from(value).is_ok()),
            "trauma" | "bleeding_per_second" | "shock" => value
                .as_u64()
                .is_some_and(|value| u16::try_from(value).is_ok()),
            "target" | "id" | "ammunition" | "supplies" | "at" | "patient" | "medic" | "wound"
            | "treatment" => value.as_u64().is_some(),
            "hot" => value.as_bool().is_some(),
            "command" | "activity" | "kind" => value.as_str().is_some(),
            _ => false,
        };
        if !valid {
            return Err(());
        }
    }
    Ok(())
}
const WIRE_FIELDS: &[&str] = &[
    "version",
    "command",
    "target",
    "id",
    "from",
    "to",
    "ammunition",
    "supplies",
    "cell",
    "hot",
    "at",
    "activity",
    "patient",
    "medic",
    "wound",
    "trauma",
    "bleeding_per_second",
    "shock",
    "kind",
    "treatment",
];
fn known_wire_field(field: &str) -> bool {
    WIRE_FIELDS.contains(&field)
}
fn take_value(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<Value, ()> {
    fields
        .remove(key)
        .filter(|value| !value.is_null())
        .ok_or(())
}
fn take_u64(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<u64, ()> {
    take_value(fields, key)?.as_u64().ok_or(())
}
fn take_u32(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<u32, ()> {
    u32::try_from(take_u64(fields, key)?).map_err(|_| ())
}
fn take_u16(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<u16, ()> {
    u16::try_from(take_u64(fields, key)?).map_err(|_| ())
}
fn take_bool(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<bool, ()> {
    take_value(fields, key)?.as_bool().ok_or(())
}
fn take_string(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<String, ()> {
    take_value(fields, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or(())
}
fn treatment_wound(
    fields: &mut BTreeMap<String, Value>,
    kind: TreatmentKind,
) -> Result<Option<WoundId>, ()> {
    match kind {
        TreatmentKind::Hemostatic => Ok(Some(WoundId(take_u64(fields, "wound")?))),
        TreatmentKind::Shock => (!fields.contains_key("wound")).then_some(None).ok_or(()),
    }
}
fn parse_kind(k: &str) -> Result<TreatmentKind, ()> {
    match k {
        "hemostatic" => Ok(TreatmentKind::Hemostatic),
        "shock" => Ok(TreatmentKind::Shock),
        _ => Err(()),
    }
}
fn outcome_json(
    clock: u64,
    events: &[TimedEvent],
    error: Option<&SimError>,
    blocked: Option<BlockedCommand>,
    digest: u64,
) -> String {
    json!({"version":1,"clock":clock,"events":events.iter().map(event_json).collect::<Vec<_>>(),
        "terminal_error":error.map(error_code), "blocked":blocked.map(blocked_json), "digest":format!("{digest:016x}")}).to_string()
}
fn stock_json(s: Stock) -> Value {
    json!({"ammunition":s.ammunition,"supplies":s.supplies})
}
fn command_json(c: ScheduledCommand) -> Value {
    match c {
        ScheduledCommand::Transfer {
            from,
            to,
            ammunition,
            supplies,
        } => {
            json!({"type":"transfer","from":from,"to":to,"ammunition":ammunition,"supplies":supplies})
        }
        ScheduledCommand::SetRegionHot { cell, hot } => {
            json!({"type":"set_region_hot","cell":cell,"hot":hot})
        }
        ScheduledCommand::CreateStockpile { id, initial } => {
            json!({"type":"create_stockpile","id":id,"initial":stock_json(initial)})
        }
    }
}
fn blocked_json(b: BlockedCommand) -> Value {
    json!({"id":b.id,"at":b.at,"command":command_json(b.command)})
}
fn error_code(e: &SimError) -> &'static str {
    match e {
        SimError::InvalidEntity => "invalid_entity",
        SimError::TimeReversal => "time_reversal",
        SimError::UnknownStockpile => "unknown_stockpile",
        SimError::InsufficientStock => "insufficient_stock",
        SimError::InvalidTransfer => "invalid_transfer",
        SimError::StockpileAlreadyExists => "stockpile_already_exists",
        SimError::ArithmeticOverflow => "arithmetic_overflow",
        SimError::InvalidSquad => "invalid_squad",
        SimError::SquadAlreadyExists => "squad_already_exists",
        SimError::InvalidOfficerRole => "invalid_officer_role",
        SimError::InvalidScheduledCommand => "invalid_scheduled_command",
        SimError::UnknownScheduledCommand => "unknown_scheduled_command",
        SimError::Snapshot(_) => "snapshot_invalid",
        SimError::DeadEntity => "dead_entity",
        SimError::InvalidHealth => "invalid_health",
        SimError::InvalidWound => "invalid_wound",
        SimError::InvalidTreatment => "invalid_treatment",
        SimError::NoEligibleMedic => "no_eligible_medic",
        SimError::BusyEntity => "busy_entity",
        SimError::InsufficientMedical => "insufficient_medical",
    }
}
fn event_json(x: &TimedEvent) -> Value {
    let payload = match x.event {
        Event::SoldierSpawned { id, loadout } => {
            json!({"type":"soldier_spawned","id":id.raw(),"loadout":{"ammunition":loadout.ammunition,"food":loadout.food,"water":loadout.water,"medical":loadout.medical}})
        }
        Event::SoldierRemoved { id, loadout } => {
            json!({"type":"soldier_removed","id":id.raw(),"loadout":{"ammunition":loadout.ammunition,"food":loadout.food,"water":loadout.water,"medical":loadout.medical}})
        }
        Event::SquadCreated { id } => json!({"type":"squad_created","id":id}),
        Event::OfficerAssigned { squad, officer } => {
            json!({"type":"officer_assigned","squad":squad,"officer":officer.raw()})
        }
        Event::StockpileCreated { id, initial } => {
            json!({"type":"stockpile_created","id":id,"initial":stock_json(initial)})
        }
        Event::TransferCompleted { from, to, stock } => {
            json!({"type":"transfer_completed","from":from,"to":to,"stock":stock_json(stock)})
        }
        Event::RegionFidelityChanged {
            cell,
            hot,
            fixed_steps,
        } => {
            json!({"type":"region_fidelity_changed","cell":cell,"hot":hot,"fixed_steps":fixed_steps})
        }
        Event::TimeAdvanced {
            from,
            to,
            hot_cells_stepped,
            fixed_steps_per_hot_cell,
        } => {
            json!({"type":"time_advanced","from":from,"to":to,"hot_cells_stepped":hot_cells_stepped,"fixed_steps_per_hot_cell":fixed_steps_per_hot_cell})
        }
        Event::Scheduled { id, at } => json!({"type":"scheduled","id":id,"at":at}),
        Event::ScheduleCancelled { id, at } => json!({"type":"schedule_cancelled","id":id,"at":at}),
        Event::RandomGenerated { value } => json!({"type":"random_generated","value":value}),
        Event::ActivityChanged {
            id,
            before,
            after,
            forced,
        } => {
            json!({"type":"activity_changed","id":id.raw(),"before":activity_name(before),"after":activity_name(after),"forced":forced})
        }
        Event::RationConsumed {
            id,
            food,
            water,
            hunger_before,
            hunger_after,
            thirst_before,
            thirst_after,
        } => {
            json!({"type":"ration_consumed","id":id.raw(),"food":food,"water":water,"hunger_before":hunger_before,"hunger_after":hunger_after,"thirst_before":thirst_before,"thirst_after":thirst_after})
        }
        Event::LivingDeteriorated {
            id,
            morale_before,
            morale_after,
            health_before,
            health_after,
        } => {
            json!({"type":"living_deteriorated","id":id.raw(),"morale_before":morale_before,"morale_after":morale_after,"health_before":health_before,"health_after":health_after})
        }
        Event::SoldierDied {
            id,
            cause,
            health_before,
        } => {
            json!({"type":"soldier_died","id":id.raw(),"cause":match cause{DeathCause::Dehydration=>"dehydration",DeathCause::Starvation=>"starvation",DeathCause::Exhaustion=>"exhaustion",DeathCause::ImmediateTrauma=>"immediate_trauma",DeathCause::Hemorrhage=>"hemorrhage",DeathCause::TraumaticShock=>"traumatic_shock"},"health_before":health_before})
        }
        Event::WoundInflicted { id, patient, wound } => {
            json!({"type":"wound_inflicted","id":id.0,"patient":patient.raw(),"trauma":wound.trauma,"bleeding_per_second":wound.bleeding_per_second,"shock":wound.shock})
        }
        Event::TreatmentStarted {
            id,
            medic,
            patient,
            wound,
            kind,
            completes_at,
            consumed,
        } => {
            json!({"type":"treatment_started","id":id.0,"medic":medic.raw(),"patient":patient.raw(),"wound":wound.map(|x|x.0),"kind":treatment_kind(kind),"completes_at":completes_at,"consumed":consumed})
        }
        Event::TreatmentCompleted {
            id,
            medic,
            patient,
            kind,
        } => {
            json!({"type":"treatment_completed","id":id.0,"medic":medic.raw(),"patient":patient.raw(),"kind":treatment_kind(kind)})
        }
        Event::TreatmentInterrupted { id, reason } => {
            json!({"type":"treatment_interrupted","id":id.0,"reason":interruption_reason(reason)})
        }
        Event::RecoveryChanged {
            id,
            before,
            after,
            next_at,
        } => {
            json!({"type":"recovery_changed","id":id.raw(),"before":before,"after":after,"next_at":next_at})
        }
        Event::RecoveryTicked {
            id,
            blood_before,
            blood_after,
            shock_before,
            shock_after,
            health_before,
            health_after,
        } => {
            json!({"type":"recovery_ticked","id":id.raw(),"blood_before":blood_before,"blood_after":blood_after,"shock_before":shock_before,"shock_after":shock_after,"health_before":health_before,"health_after":health_after})
        }
        Event::WoundHealed { id, patient } => {
            json!({"type":"wound_healed","id":id.0,"patient":patient.raw()})
        }
    };
    json!({"at":x.at,"event":payload})
}
fn treatment_kind(k: TreatmentKind) -> &'static str {
    match k {
        TreatmentKind::Hemostatic => "hemostatic",
        TreatmentKind::Shock => "shock",
    }
}
fn interruption_reason(reason: InterruptionReason) -> &'static str {
    match reason {
        InterruptionReason::Explicit => "explicit",
        InterruptionReason::MedicDied => "medic_died",
        InterruptionReason::PatientDied => "patient_died",
        InterruptionReason::MedicRemoved => "medic_removed",
        InterruptionReason::PatientRemoved => "patient_removed",
        InterruptionReason::Ineligible => "ineligible",
    }
}
fn activity_name(a: Activity) -> &'static str {
    match a {
        Activity::Rest => "rest",
        Activity::Idle => "idle",
        Activity::March => "march",
    }
}

fn apply_ok(w: &mut World, c: Command) {
    let o = w.apply(c);
    assert!(o.error.is_none(), "{:?}", o.error)
}
fn benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let count = env::var("PA_SOLDIERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_410_000usize);
    let dense = env::var("PA_DENSE_COMMANDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000usize);
    let mut w = World::new(0x5eed);
    let t = Instant::now();
    let hot_cohort = (count / 100).clamp(1, 1_000);
    for i in 0..count {
        let outcome = w.apply(Command::SpawnSoldier {
            spec: SoldierSpec {
                faction: (i % 2) as u16,
                position: sim_core::Position {
                    cell: if i < hot_cohort {
                        0
                    } else {
                        u32::try_from(dense).unwrap_or(u32::MAX)
                    },
                    ..Default::default()
                },
                inventory: sim_core::Inventory {
                    food: 1 + (i % 3) as u32,
                    water: 1 + (i % 4) as u32,
                    medical: (i % 2) as u32,
                },
                ..SoldierSpec::default()
            },
        });
        assert!(outcome.error.is_none());
        if i % 3 != 1 {
            let id = match outcome.events[0].event {
                Event::SoldierSpawned { id, .. } => id,
                _ => unreachable!(),
            };
            apply_ok(
                &mut w,
                Command::SetActivity {
                    id,
                    activity: if i % 3 == 0 {
                        Activity::March
                    } else {
                        Activity::Rest
                    },
                },
            );
        }
    }
    let init = t.elapsed();
    apply_ok(
        &mut w,
        Command::CreateStockpile {
            id: 1,
            initial: Stock {
                ammunition: count as u64 * 30,
                supplies: count as u64 * 2,
            },
        },
    );
    apply_ok(
        &mut w,
        Command::CreateStockpile {
            id: 2,
            initial: Stock::default(),
        },
    );
    for cell in 0..dense {
        apply_ok(
            &mut w,
            Command::Schedule {
                at: 20,
                command: ScheduledCommand::SetRegionHot {
                    cell: cell as u32,
                    hot: true,
                },
            },
        )
    }
    let cold_start = Instant::now();
    let dense_out = w.apply(Command::AdvanceTo { target: 20 });
    let cold_time = cold_start.elapsed();
    apply_ok(
        &mut w,
        Command::Schedule {
            at: 30,
            command: ScheduledCommand::Transfer {
                from: 1,
                to: 2,
                ammunition: 1000,
                supplies: 500,
            },
        },
    );
    let mixed_start = Instant::now();
    let sparse_out = w.apply(Command::AdvanceTo { target: 60 });
    let mixed_time = mixed_start.elapsed();
    assert!(dense_out.error.is_none() && sparse_out.error.is_none());
    let repeated_start = Instant::now();
    let mut repeated_events = 0;
    for target in 61..=63 {
        let outcome = w.apply(Command::AdvanceTo { target });
        assert!(outcome.error.is_none());
        repeated_events += outcome.events.len();
    }
    let repeated_time = repeated_start.elapsed();
    let needs = Instant::now();
    let checksum = w.needs_checksum();
    let needs_time = needs.elapsed();
    let combined = cold_time + mixed_time + repeated_time + needs_time;
    let rate = 63.0 / combined.as_secs_f64();
    let st = Instant::now();
    let snapshot = w.snapshot();
    let snapshot_time = st.elapsed();
    let dt = Instant::now();
    let digest = w.state_digest();
    let digest_time = dt.elapsed();
    let (rss, peak) = memory_kib();
    let (cold_boundaries, hot_member_steps) = w.living_work_counters();
    println!("soldiers={count}\ninitialization_seconds={:.6}\ndense_scheduler_commands={dense}\nadvance_simulated_seconds=63\ncold_advance_seconds={:.6}\nmixed_hot_due_advance_seconds={:.6}\nrepeated_one_second_advances=3\nrepeated_one_second_seconds={:.6}\nrepeated_one_second_events={repeated_events}\nadvance_events={}\ncold_due_transitions={cold_boundaries}\nhot_indexed_members={hot_cohort}\nhot_member_steps={hot_member_steps}\nfull_living_checksum_seconds={:.6}\nsimulated_seconds_per_wall_second={rate:.3}\ndesign_goal_simulated_seconds_per_wall_second=1.000\ndesign_goal_met={}\nliving_checksum={checksum:016x}\nsnapshot_seconds={:.6}\nsnapshot_bytes={}\ndigest_seconds={:.6}\ndigest={digest:016x}\ncurrent_rss_kib={rss}\npeak_rss_kib={peak}",init.as_secs_f64(),cold_time.as_secs_f64(),mixed_time.as_secs_f64(),repeated_time.as_secs_f64(),dense_out.events.len()+sparse_out.events.len()+repeated_events,needs_time.as_secs_f64(),rate>=1.0,snapshot_time.as_secs_f64(),snapshot.len(),digest_time.as_secs_f64());
    Ok(())
}
fn memory_kib() -> (u64, u64) {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .map(|s| {
            let v = |n: &str| {
                s.lines()
                    .find(|l| l.starts_with(n))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0)
            };
            (v("VmRSS:"), v("VmHWM:"))
        })
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{EntityId, InterruptionReason, Inventory, Role};
    use std::net::Shutdown;
    use std::thread;
    fn req(body: &str) -> Vec<u8> {
        format!(
            "POST /v1/command HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }
    fn exchange(request: Vec<u8>, shutdown: bool, world: World) -> (Vec<u8>, World) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut world = world;
            let (stream, _) = listener.accept().unwrap();
            serve_connection(
                stream,
                &mut world,
                connection_deadline(Duration::from_secs(2)),
            )
            .unwrap();
            world
        });
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(&request).unwrap();
        if shutdown {
            client.shutdown(Shutdown::Write).unwrap();
        }
        let mut response = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            match client.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&chunk[..n]),
                Err(error)
                    if error.kind() == std::io::ErrorKind::ConnectionReset
                        && !response.is_empty() =>
                {
                    break
                }
                Err(error) => panic!("response read: {error}"),
            }
        }
        (response, server.join().unwrap())
    }
    fn status(response: &[u8]) -> &str {
        std::str::from_utf8(response)
            .unwrap()
            .split("\r\n")
            .next()
            .unwrap()
    }
    fn json_body(response: &[u8]) -> Value {
        let split = response.windows(4).position(|x| x == b"\r\n\r\n").unwrap();
        serde_json::from_slice(&response[split + 4..]).unwrap()
    }

    fn raw_body(response: &[u8]) -> &[u8] {
        let split = response.windows(4).position(|x| x == b"\r\n\r\n").unwrap();
        &response[split + 4..]
    }

    fn pinned_http_fixture(seed: u64) -> World {
        let mut world = World::new(seed);
        let outcome = world.apply(Command::CreateStockpile {
            id: 9,
            initial: Stock {
                ammunition: 17,
                supplies: 6,
            },
        });
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.clock, 0);
        assert_eq!(world.clock(), 0);
        assert_eq!(
            world.stockpile(9),
            Some(Stock {
                ammunition: 17,
                supplies: 6,
            })
        );
        world
    }

    fn parse_wire(body: &str) -> Result<Command, ()> {
        serde_json::from_str::<Wire>(body)
            .map_err(|_| ())?
            .into_command()
    }

    #[test]
    fn gate_c2_exact_medical_command_shapes_parse_to_literal_commands() {
        let cases = [
            (
                r#"{"shock":30,"command":"inflict_wound","version":1,"bleeding_per_second":20,"patient":7,"trauma":10}"#,
                Command::InflictWound {
                    patient: EntityId::from_raw(7),
                    wound: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                },
            ),
            (
                r#"{"version":1,"command":"start_treatment","medic":2,"patient":7,"wound":4,"kind":"hemostatic"}"#,
                Command::StartTreatment {
                    medic: EntityId::from_raw(2),
                    patient: EntityId::from_raw(7),
                    wound: Some(WoundId(4)),
                    kind: TreatmentKind::Hemostatic,
                },
            ),
            (
                r#"{"kind":"shock","patient":7,"medic":2,"command":"start_treatment","version":1}"#,
                Command::StartTreatment {
                    medic: EntityId::from_raw(2),
                    patient: EntityId::from_raw(7),
                    wound: None,
                    kind: TreatmentKind::Shock,
                },
            ),
            (
                r#"{"version":1,"command":"request_treatment","patient":7,"wound":4,"kind":"hemostatic"}"#,
                Command::RequestTreatment {
                    patient: EntityId::from_raw(7),
                    wound: Some(WoundId(4)),
                    kind: TreatmentKind::Hemostatic,
                },
            ),
            (
                r#"{"version":1,"command":"request_treatment","patient":7,"kind":"shock"}"#,
                Command::RequestTreatment {
                    patient: EntityId::from_raw(7),
                    wound: None,
                    kind: TreatmentKind::Shock,
                },
            ),
            (
                r#"{"version":1,"command":"interrupt_treatment","treatment":9}"#,
                Command::InterruptTreatment { id: TreatmentId(9) },
            ),
        ];
        for (literal, expected) in cases {
            assert_eq!(parse_wire(literal), Ok(expected), "{literal}");
        }
    }

    #[test]
    fn gate_c2_strict_parser_rejects_malformed_shapes_ranges_and_duplicates() {
        let invalid = [
            r#"{}"#,
            r#"{"version":null,"command":"interrupt_treatment","treatment":1}"#,
            r#"{"version":1,"version":1,"command":"interrupt_treatment","treatment":1}"#,
            r#"{"version":2,"command":"interrupt_treatment","treatment":1}"#,
            r#"{"version":1,"command":"Interrupt_Treatment","treatment":1}"#,
            r#"{"version":1,"command":"interrupt_treatment","treatment":null}"#,
            r#"{"version":1,"command":"interrupt_treatment","treatment":-1}"#,
            r#"{"version":1,"command":"interrupt_treatment","treatment":1.0}"#,
            r#"{"version":1,"command":"interrupt_treatment","treatment":18446744073709551616}"#,
            r#"{"version":1,"command":"interrupt_treatment","treatment":1,"patient":null}"#,
            r#"{"version":1,"command":"inflict_wound","patient":1,"trauma":65536,"bleeding_per_second":0,"shock":0}"#,
            r#"{"version":1,"command":"inflict_wound","patient":1,"trauma":1,"bleeding_per_second":0,"shock":0,"wound":null}"#,
            r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"kind":"shock","wound":null}"#,
            r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"kind":"hemostatic"}"#,
            r#"{"version":1,"command":"request_treatment","patient":1,"kind":"HEMOSTATIC","wound":0}"#,
            r#"{"version":1,"command":"request_treatment","patient":1,"kind":true}"#,
            r#"{"version":1,"command":"request_treatment","patient":1,"kind":"shock","unknown":0}"#,
            r#"{"version":1,"command":"request_treatment","patient":1,"kind":"shock"} trailing"#,
        ];
        for literal in invalid {
            assert_eq!(parse_wire(literal), Err(()), "accepted {literal}");
        }
    }

    #[test]
    fn gate_c2_legacy_real_http_compatibility_and_wrong_typed_extras() {
        let legacy = [
            (
                r#"{"version":1,"command":"create_stockpile","id":7,"ammunition":9,"supplies":3,"target":null,"from":null,"to":null,"cell":null,"hot":null,"at":null,"activity":null,"patient":null,"medic":null,"wound":null,"trauma":null,"bleeding_per_second":null,"shock":null,"kind":null,"treatment":null}"#,
                json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"stockpile_created","id":7,"initial":{"ammunition":9,"supplies":3}}}],"terminal_error":null,"blocked":null,"digest":"c333945e80f9b8d1"}),
                Some(Stock {
                    ammunition: 9,
                    supplies: 3,
                }),
            ),
            (
                r#"{"version":1,"command":"advance_to","target":1,"id":null,"from":null,"to":null,"ammunition":null,"supplies":null,"cell":null,"hot":null,"at":null,"activity":null,"patient":null,"medic":null,"wound":null,"trauma":null,"bleeding_per_second":null,"shock":null,"kind":null,"treatment":null}"#,
                json!({"version":1,"clock":1,"events":[{"at":1,"event":{"type":"time_advanced","from":0,"to":1,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"4215b2950c54e5bc"}),
                None,
            ),
        ];
        for (body, expected, stock) in legacy {
            let (response, world) = exchange(req(body), false, World::new(31));
            assert_eq!(status(&response), "HTTP/1.1 200 OK");
            assert_eq!(json_body(&response), expected);
            assert_eq!(world.clock(), expected["clock"].as_u64().unwrap());
            assert_eq!(world.stockpile(7), stock);
        }

        let wrong_typed_extras = [
            (
                "Boolean",
                r#"{"version":1,"command":"advance_to","target":1,"hot":"wrong"}"#,
                37,
                0x81df_6de6_6513_26f8,
            ),
            (
                "string",
                r#"{"version":1,"command":"advance_to","target":1,"activity":false}"#,
                38,
                0x861c_8ca5_3f60_578b,
            ),
            (
                "u16",
                r#"{"version":1,"command":"advance_to","target":1,"trauma":65536}"#,
                39,
                0xaebe_7a08_130a_27da,
            ),
            (
                "u32",
                r#"{"version":1,"command":"advance_to","target":1,"cell":4294967296}"#,
                40,
                0xd4b7_bd8d_a212_49e5,
            ),
            (
                "u64",
                r#"{"version":1,"command":"advance_to","target":1,"id":[]}"#,
                41,
                0xfd59_aaf0_75bc_1a34,
            ),
        ];
        for (name, body, seed, expected_digest) in wrong_typed_extras {
            let world = pinned_http_fixture(seed);
            let before = world.snapshot();
            let digest = world.state_digest();
            assert_eq!(digest, expected_digest, "{name}");
            assert_eq!(world.clock(), 0, "{name}");
            assert_eq!(
                world.stockpile(9),
                Some(Stock {
                    ammunition: 17,
                    supplies: 6
                }),
                "{name}"
            );
            let (response, world) = exchange(req(body), false, world);
            assert_eq!(status(&response), "HTTP/1.1 400 Bad Request", "{name}");
            assert_eq!(
                raw_body(&response),
                b"{\"error\":\"malformed_or_unsupported\"}",
                "{name}"
            );
            assert_eq!(world.clock(), 0, "{name}");
            assert_eq!(
                world.stockpile(9),
                Some(Stock {
                    ammunition: 17,
                    supplies: 6
                }),
                "{name}"
            );
            assert_eq!(world.snapshot(), before, "{name}");
            assert_eq!(world.state_digest(), digest, "{name}");
            assert_eq!(world.state_digest(), expected_digest, "{name}");
        }
    }

    #[test]
    fn gate_c2_event_enum_reason_cause_and_error_serialization_matrix() {
        let p = EntityId::from_raw(7);
        let m = EntityId::from_raw(2);
        let events = [
            (
                Event::WoundInflicted {
                    id: WoundId(3),
                    patient: p,
                    wound: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                },
                json!({"at":41,"event":{"type":"wound_inflicted","id":3,"patient":7,"trauma":10,"bleeding_per_second":20,"shock":30}}),
            ),
            (
                Event::TreatmentStarted {
                    id: TreatmentId(4),
                    medic: m,
                    patient: p,
                    wound: Some(WoundId(3)),
                    kind: TreatmentKind::Hemostatic,
                    completes_at: 51,
                    consumed: 1,
                },
                json!({"at":41,"event":{"type":"treatment_started","id":4,"medic":2,"patient":7,"wound":3,"kind":"hemostatic","completes_at":51,"consumed":1}}),
            ),
            (
                Event::TreatmentStarted {
                    id: TreatmentId(5),
                    medic: m,
                    patient: p,
                    wound: None,
                    kind: TreatmentKind::Shock,
                    completes_at: 56,
                    consumed: 2,
                },
                json!({"at":41,"event":{"type":"treatment_started","id":5,"medic":2,"patient":7,"wound":null,"kind":"shock","completes_at":56,"consumed":2}}),
            ),
            (
                Event::TreatmentCompleted {
                    id: TreatmentId(4),
                    medic: m,
                    patient: p,
                    kind: TreatmentKind::Hemostatic,
                },
                json!({"at":41,"event":{"type":"treatment_completed","id":4,"medic":2,"patient":7,"kind":"hemostatic"}}),
            ),
            (
                Event::TreatmentCompleted {
                    id: TreatmentId(5),
                    medic: m,
                    patient: p,
                    kind: TreatmentKind::Shock,
                },
                json!({"at":41,"event":{"type":"treatment_completed","id":5,"medic":2,"patient":7,"kind":"shock"}}),
            ),
            (
                Event::RecoveryChanged {
                    id: p,
                    before: false,
                    after: true,
                    next_at: Some(46),
                },
                json!({"at":41,"event":{"type":"recovery_changed","id":7,"before":false,"after":true,"next_at":46}}),
            ),
            (
                Event::RecoveryChanged {
                    id: p,
                    before: true,
                    after: false,
                    next_at: None,
                },
                json!({"at":41,"event":{"type":"recovery_changed","id":7,"before":true,"after":false,"next_at":null}}),
            ),
            (
                Event::RecoveryTicked {
                    id: p,
                    blood_before: 4000,
                    blood_after: 4100,
                    shock_before: 200,
                    shock_after: 150,
                    health_before: 700,
                    health_after: 725,
                },
                json!({"at":41,"event":{"type":"recovery_ticked","id":7,"blood_before":4000,"blood_after":4100,"shock_before":200,"shock_after":150,"health_before":700,"health_after":725}}),
            ),
            (
                Event::WoundHealed {
                    id: WoundId(3),
                    patient: p,
                },
                json!({"at":41,"event":{"type":"wound_healed","id":3,"patient":7}}),
            ),
        ];
        for (event, expected) in events {
            assert_eq!(event_json(&TimedEvent { at: 41, event }), expected);
        }
        for (reason, name) in [
            (InterruptionReason::Explicit, "explicit"),
            (InterruptionReason::MedicDied, "medic_died"),
            (InterruptionReason::PatientDied, "patient_died"),
            (InterruptionReason::MedicRemoved, "medic_removed"),
            (InterruptionReason::PatientRemoved, "patient_removed"),
            (InterruptionReason::Ineligible, "ineligible"),
        ] {
            assert_eq!(
                event_json(&TimedEvent {
                    at: 9,
                    event: Event::TreatmentInterrupted {
                        id: TreatmentId(8),
                        reason
                    }
                }),
                json!({"at":9,"event":{"type":"treatment_interrupted","id":8,"reason":name}})
            );
        }
        for (cause, name) in [
            (DeathCause::Dehydration, "dehydration"),
            (DeathCause::Starvation, "starvation"),
            (DeathCause::Exhaustion, "exhaustion"),
            (DeathCause::ImmediateTrauma, "immediate_trauma"),
            (DeathCause::Hemorrhage, "hemorrhage"),
            (DeathCause::TraumaticShock, "traumatic_shock"),
        ] {
            assert_eq!(
                event_json(&TimedEvent {
                    at: 9,
                    event: Event::SoldierDied {
                        id: p,
                        cause,
                        health_before: 321
                    }
                }),
                json!({"at":9,"event":{"type":"soldier_died","id":7,"cause":name,"health_before":321}})
            );
        }
        let errors = [
            (SimError::DeadEntity, "dead_entity"),
            (SimError::InvalidHealth, "invalid_health"),
            (SimError::InvalidWound, "invalid_wound"),
            (SimError::InvalidTreatment, "invalid_treatment"),
            (SimError::NoEligibleMedic, "no_eligible_medic"),
            (SimError::BusyEntity, "busy_entity"),
            (SimError::InsufficientMedical, "insufficient_medical"),
            (SimError::ArithmeticOverflow, "arithmetic_overflow"),
            (SimError::Snapshot("x"), "snapshot_invalid"),
        ];
        for (error, code) in errors {
            assert_eq!(error_code(&error), code);
        }
    }

    fn spawn(world: &mut World, role: Role, medical: u32) -> EntityId {
        let outcome = world.apply(Command::SpawnSoldier {
            spec: SoldierSpec {
                role,
                inventory: Inventory {
                    medical,
                    ..Inventory::default()
                },
                ..SoldierSpec::default()
            },
        });
        match outcome.events.as_slice() {
            [TimedEvent {
                event: Event::SoldierSpawned { id, .. },
                ..
            }] => *id,
            _ => panic!("unexpected spawn: {outcome:?}"),
        }
    }

    #[test]
    fn gate_c2_real_http_medical_success_and_continuation() {
        let mut world = World::new(11);
        let medic = spawn(&mut world, Role::Medic, 5);
        let patient = spawn(&mut world, Role::Rifle, 0);
        assert_eq!((medic.raw(), patient.raw()), (0, 1));
        let wound_body = r#"{"version":1,"command":"inflict_wound","patient":1,"trauma":10,"bleeding_per_second":20,"shock":30}"#;
        let (response, after_wound) = exchange(req(wound_body), false, world);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"wound_inflicted","id":0,"patient":1,"trauma":10,"bleeding_per_second":20,"shock":30}}],"terminal_error":null,"blocked":null,"digest":"1a644c800f5dfd57"})
        );
        assert_eq!(after_wound.wound(WoundId(0)).unwrap().patient, patient);
        let start_body = r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#;
        let (response, active) = exchange(req(start_body), false, after_wound);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_started","id":0,"medic":0,"patient":1,"wound":0,"kind":"hemostatic","completes_at":10,"consumed":1}}],"terminal_error":null,"blocked":null,"digest":"d3b2b24654f6f170"})
        );
        assert_eq!(active.soldier(medic).unwrap().inventory.medical, 4);
        assert_eq!(active.resource_totals().consumed_medical, 1);
        let interrupt = r#"{"version":1,"command":"interrupt_treatment","treatment":0}"#;
        let (response, interrupted) = exchange(req(interrupt), false, active);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_interrupted","id":0,"reason":"explicit"}}],"terminal_error":null,"blocked":null,"digest":"aa8ad8b69e55cfe2"})
        );
        let before = interrupted.snapshot();
        let digest = interrupted.state_digest();
        let malformed =
            r#"{"version":1,"command":"interrupt_treatment","treatment":0,"patient":null}"#;
        let (response, unchanged) = exchange(req(malformed), false, interrupted);
        assert_eq!(status(&response), "HTTP/1.1 400 Bad Request");
        assert_eq!(
            json_body(&response),
            json!({"error":"malformed_or_unsupported"})
        );
        assert_eq!(unchanged.snapshot(), before);
        assert_eq!(unchanged.state_digest(), digest);
        let advance = r#"{"version":1,"command":"advance_to","target":20}"#;
        let (response, advanced) = exchange(req(advance), false, unchanged);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":20,"events":[{"at":20,"event":{"type":"time_advanced","from":0,"to":20,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"035c8d0e76da8786"})
        );
        assert_eq!(advanced.clock(), 20);
        assert_eq!(advanced.soldier(medic).unwrap().inventory.medical, 4);
        assert_eq!(advanced.resource_totals().consumed_medical, 1);
    }

    #[test]
    fn gate_c2_http_malformed_matrix_is_byte_and_digest_atomic() {
        let cases = [
            (
                "missing version",
                r#"{"command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "null command",
                r#"{"version":1,"command":null,"treatment":0}"#,
            ),
            (
                "duplicate payload",
                r#"{"version":1,"command":"interrupt_treatment","treatment":0,"treatment":0}"#,
            ),
            (
                "inapplicable recognized",
                r#"{"version":1,"command":"interrupt_treatment","treatment":0,"wound":null}"#,
            ),
            (
                "unknown",
                r#"{"version":1,"command":"interrupt_treatment","treatment":0,"surprise":1}"#,
            ),
            (
                "negative",
                r#"{"version":1,"command":"interrupt_treatment","treatment":-1}"#,
            ),
            (
                "fractional",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1.5,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "array",
                r#"{"version":1,"command":"inflict_wound","patient":[],"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "shock wound",
                r#"{"version":1,"command":"request_treatment","patient":0,"kind":"shock","wound":null}"#,
            ),
            (
                "hemostatic null wound",
                r#"{"version":1,"command":"request_treatment","patient":0,"kind":"hemostatic","wound":null}"#,
            ),
            ("missing command", r#"{"version":1,"treatment":0}"#),
            (
                "null version",
                r#"{"version":null,"command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "wrong version type",
                r#"{"version":"1","command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "duplicate version",
                r#"{"version":1,"version":1,"command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "wrong command type",
                r#"{"version":1,"command":false,"treatment":0}"#,
            ),
            (
                "duplicate command",
                r#"{"version":1,"command":"interrupt_treatment","command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "unsupported version",
                r#"{"version":2,"command":"interrupt_treatment","treatment":0}"#,
            ),
            (
                "unknown command",
                r#"{"version":1,"command":"operate","treatment":0}"#,
            ),
            (
                "case command",
                r#"{"version":1,"command":"Inflict_Wound","patient":0,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "missing wound patient",
                r#"{"version":1,"command":"inflict_wound","trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "null wound patient",
                r#"{"version":1,"command":"inflict_wound","patient":null,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "missing trauma",
                r#"{"version":1,"command":"inflict_wound","patient":0,"bleeding_per_second":1,"shock":0}"#,
            ),
            (
                "null trauma",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":null,"bleeding_per_second":1,"shock":0}"#,
            ),
            (
                "missing bleeding",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"shock":0}"#,
            ),
            (
                "null bleeding",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"bleeding_per_second":null,"shock":0}"#,
            ),
            (
                "missing shock",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"bleeding_per_second":0}"#,
            ),
            (
                "null shock",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"bleeding_per_second":0,"shock":null}"#,
            ),
            (
                "direct missing medic",
                r#"{"version":1,"command":"start_treatment","patient":0,"wound":0,"kind":"hemostatic"}"#,
            ),
            (
                "direct null medic",
                r#"{"version":1,"command":"start_treatment","medic":null,"patient":0,"wound":0,"kind":"hemostatic"}"#,
            ),
            (
                "direct missing patient",
                r#"{"version":1,"command":"start_treatment","medic":0,"wound":0,"kind":"hemostatic"}"#,
            ),
            (
                "direct null patient",
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":null,"wound":0,"kind":"hemostatic"}"#,
            ),
            (
                "direct missing kind",
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0}"#,
            ),
            (
                "direct null kind",
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":null}"#,
            ),
            (
                "request missing patient",
                r#"{"version":1,"command":"request_treatment","kind":"shock"}"#,
            ),
            (
                "request null patient",
                r#"{"version":1,"command":"request_treatment","patient":null,"kind":"shock"}"#,
            ),
            (
                "unknown kind",
                r#"{"version":1,"command":"request_treatment","patient":0,"kind":"bandage"}"#,
            ),
            (
                "case kind",
                r#"{"version":1,"command":"request_treatment","patient":0,"kind":"Shock"}"#,
            ),
            (
                "string numeric",
                r#"{"version":1,"command":"inflict_wound","patient":"0","trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "boolean numeric",
                r#"{"version":1,"command":"inflict_wound","patient":false,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "object numeric",
                r#"{"version":1,"command":"inflict_wound","patient":{},"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "entity overflow",
                r#"{"version":1,"command":"inflict_wound","patient":18446744073709551616,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "wound overflow",
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":18446744073709551616,"kind":"hemostatic"}"#,
            ),
            (
                "treatment overflow",
                r#"{"version":1,"command":"interrupt_treatment","treatment":18446744073709551616}"#,
            ),
            (
                "bleeding u16 overflow",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":65536,"shock":0}"#,
            ),
            (
                "shock u16 overflow",
                r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":0,"shock":65536}"#,
            ),
            (
                "duplicate patient",
                r#"{"version":1,"command":"inflict_wound","patient":0,"patient":0,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            ),
            (
                "duplicate wound",
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"wound":0,"kind":"hemostatic"}"#,
            ),
            (
                "malformed json",
                r#"{"version":1,"command":"interrupt_treatment","treatment":0"#,
            ),
        ];
        for (name, body) in cases {
            let world = pinned_http_fixture(77);
            let before = world.snapshot();
            let digest = world.state_digest();
            assert_eq!(digest, 0x2bd7_8a3a_f7ee_2a90, "{name}");
            let (response, world) = exchange(req(body), false, world);
            assert_eq!(status(&response), "HTTP/1.1 400 Bad Request", "{name}");
            assert_eq!(
                json_body(&response),
                json!({"error":"malformed_or_unsupported"}),
                "{name}"
            );
            assert_eq!(world.clock(), 0, "{name}");
            assert_eq!(world.snapshot(), before, "{name}");
            assert_eq!(world.state_digest(), digest, "{name}");
            assert_eq!(world.state_digest(), 0x2bd7_8a3a_f7ee_2a90, "{name}");
        }
    }

    #[test]
    fn gate_c2_semantic_rejection_returns_complete_outcome_and_is_atomic() {
        let mut world = World::new(19);
        let patient = spawn(&mut world, Role::Rifle, 0);
        assert_eq!(patient.raw(), 0);
        let before = world.snapshot();
        let digest = world.state_digest();
        assert_eq!(digest, 0x141a_fc1e_bd5c_94da);
        let body = r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":0,"shock":0}"#;
        let (response, world) = exchange(req(body), false, world);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[],"terminal_error":"invalid_wound","blocked":null,"digest":"141afc1ebd5c94da"})
        );
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.state_digest(), digest);
    }

    #[test]
    fn live_socket_framing_and_typed_outcomes() {
        let (response, world) = exchange(
            b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
            false,
            World::new(0),
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        let body = r#"{"version":1,"command":"create_stockpile","id":7,"ammunition":9,"supplies":3,"target":null,"from":null,"to":null,"cell":null,"hot":null,"at":null}"#;
        let (response, world) = exchange(req(body), false, world);
        let parsed = json_body(&response);
        assert_eq!(
            parsed["events"][0]["event"],
            json!({"type":"stockpile_created","id":7,"initial":{"ammunition":9,"supplies":3}})
        );
        assert_eq!(
            world.stockpile(7).unwrap(),
            Stock {
                ammunition: 9,
                supplies: 3
            }
        );
        let schedule = r#"{"version":1,"command":"schedule_transfer","at":2,"from":7,"to":8,"ammunition":40,"supplies":2,"target":null,"id":null,"cell":null,"hot":null}"#;
        let (response, world) = exchange(req(schedule), false, world);
        assert_eq!(json_body(&response)["events"][0]["event"]["id"], 0);
        let create_destination =
            r#"{"version":1,"command":"create_stockpile","id":8,"ammunition":0,"supplies":0}"#;
        let (_, world) = exchange(req(create_destination), false, world);
        let advance = r#"{"version":1,"command":"advance_to","target":3}"#;
        let (response, world) = exchange(req(advance), false, world);
        let parsed = json_body(&response);
        assert_eq!(parsed["terminal_error"], "insufficient_stock");
        assert_eq!(
            parsed["blocked"],
            json!({"id":0,"at":2,"command":{"type":"transfer","from":7,"to":8,"ammunition":40,"supplies":2}})
        );
        assert_eq!(parsed["clock"], 2);
        assert_eq!(parsed["events"][0]["event"]["type"], "time_advanced");
        assert_eq!(parsed["digest"], format!("{:016x}", world.state_digest()));
        let digest = world.state_digest();
        let malformed =
            b"POST /v1/command HTTP/1.1\r\nContent-Length: 2\r\ncontent-length: 2\r\n\r\n{}"
                .to_vec();
        let (response, world) = exchange(malformed, false, world);
        assert_eq!(status(&response), "HTTP/1.1 400 Bad Request");
        assert_eq!(world.state_digest(), digest);
    }

    #[test]
    fn live_socket_rejects_truncated_oversized_and_bad_framing_atomically() {
        let world = World::new(3);
        let digest = world.state_digest();
        let truncated = b"POST /v1/command HTTP/1.1\r\nContent-Length: 10\r\n\r\n{}".to_vec();
        let (response, world) = exchange(truncated, true, world);
        assert_eq!(status(&response), "HTTP/1.1 400 Bad Request");
        assert_eq!(world.state_digest(), digest);
        let oversized = format!(
            "POST /v1/command HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_REQUEST + 1
        )
        .into_bytes();
        let (response, world) = exchange(oversized, false, world);
        assert_eq!(status(&response), "HTTP/1.1 413 Payload Too Large");
        assert_eq!(world.state_digest(), digest);
        let (response, world) = exchange(b"GET /health HTTP/1.1\n\n".to_vec(), true, world);
        assert_eq!(status(&response), "HTTP/1.1 400 Bad Request");
        assert_eq!(world.state_digest(), digest);
    }

    fn later_health(listener: &TcpListener) -> TcpStream {
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        client
    }

    #[test]
    fn listener_contains_connection_failure_and_serves_later_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client_listener = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let mut first = true;
            let mut world = World::new(0);
            apply_ok(
                &mut world,
                Command::CreateStockpile {
                    id: 1,
                    initial: Stock {
                        ammunition: 10,
                        supplies: 4,
                    },
                },
            );
            apply_ok(
                &mut world,
                Command::CreateStockpile {
                    id: 2,
                    initial: Stock::default(),
                },
            );
            serve_listener(
                &listener,
                &mut world,
                Duration::from_millis(100),
                Some(3),
                |mut stream, world, deadline| {
                    if first {
                        first = false;
                        stream.set_read_timeout(Some(remaining(deadline)?))?;
                        let _committed_response = read_and_handle(&mut stream, world);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "injected response write failure",
                        ));
                    }
                    serve_connection(stream, world, deadline)
                },
            )
            .unwrap();
            world
        });
        let body =
            r#"{"version":1,"command":"transfer","from":1,"to":2,"ammunition":3,"supplies":2}"#;
        let mut failed = TcpStream::connect(client_listener.local_addr().unwrap()).unwrap();
        failed.write_all(&req(body)).unwrap();
        drop(failed);
        let mut healthy = later_health(&client_listener);
        let mut response = Vec::new();
        healthy.read_to_end(&mut response).unwrap();
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        let health = json_body(&response);
        let mut snapshot_client =
            TcpStream::connect(client_listener.local_addr().unwrap()).unwrap();
        snapshot_client
            .write_all(b"GET /snapshot HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut snapshot_response = Vec::new();
        snapshot_client.read_to_end(&mut snapshot_response).unwrap();
        let split = snapshot_response
            .windows(4)
            .position(|x| x == b"\r\n\r\n")
            .unwrap();
        let restored = World::from_snapshot(&snapshot_response[split + 4..]).unwrap();
        let world = server.join().unwrap();
        assert_eq!(
            world.stockpile(1),
            Some(Stock {
                ammunition: 7,
                supplies: 2
            })
        );
        assert_eq!(
            world.stockpile(2),
            Some(Stock {
                ammunition: 3,
                supplies: 2
            })
        );
        assert_eq!(restored.state_digest(), world.state_digest());
        assert_eq!(health["digest"], format!("{:016x}", world.state_digest()));
    }

    fn stalled_peer_does_not_wedge_listener(request: &[u8]) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client_listener = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let mut world = World::new(0);
            serve_listener(
                &listener,
                &mut world,
                Duration::from_millis(40),
                Some(2),
                serve_connection,
            )
            .unwrap();
        });
        let mut stalled = TcpStream::connect(client_listener.local_addr().unwrap()).unwrap();
        stalled.write_all(request).unwrap();
        let mut healthy = later_health(&client_listener);
        healthy
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut response = Vec::new();
        healthy.read_to_end(&mut response).unwrap();
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        server.join().unwrap();
        drop(stalled);
    }

    #[test]
    fn stalled_header_times_out_without_process_restart() {
        stalled_peer_does_not_wedge_listener(b"GET /health HTTP/1.1\r\nHost: localhost\r\n");
    }

    #[test]
    fn stalled_declared_body_times_out_without_half_close() {
        stalled_peer_does_not_wedge_listener(
            b"POST /v1/command HTTP/1.1\r\nContent-Length: 100\r\n\r\n{}",
        );
    }

    #[test]
    fn trickling_header_cannot_extend_absolute_deadline() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client_listener = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let mut world = World::new(0);
            serve_listener(
                &listener,
                &mut world,
                Duration::from_millis(45),
                Some(2),
                serve_connection,
            )
            .unwrap();
        });
        let mut trickle = TcpStream::connect(client_listener.local_addr().unwrap()).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let sender_stop = Arc::clone(&stop);
        let sender = thread::spawn(move || {
            let prefix = b"GET /health HTTP/1.1\r\nX-Trickle: ";
            let mut index = 0;
            while !sender_stop.load(Ordering::SeqCst) {
                let byte = if index < prefix.len() {
                    prefix[index]
                } else {
                    b'x'
                };
                let _ = trickle.write_all(&[byte]);
                index += 1;
                thread::sleep(Duration::from_millis(10));
            }
        });
        thread::sleep(Duration::from_millis(70));
        let mut healthy = later_health(&client_listener);
        healthy
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut response = Vec::new();
        healthy.read_to_end(&mut response).unwrap();
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert!(!stop.load(Ordering::SeqCst));
        stop.store(true, Ordering::SeqCst);
        sender.join().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn trickling_body_cannot_extend_absolute_deadline() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client_listener = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let mut world = World::new(0);
            serve_listener(
                &listener,
                &mut world,
                Duration::from_millis(45),
                Some(2),
                serve_connection,
            )
            .unwrap();
        });
        let mut trickle = TcpStream::connect(client_listener.local_addr().unwrap()).unwrap();
        trickle
            .write_all(b"POST /v1/command HTTP/1.1\r\nContent-Length: 65536\r\n\r\n")
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let sender_stop = Arc::clone(&stop);
        let sender = thread::spawn(move || {
            while !sender_stop.load(Ordering::SeqCst) {
                let _ = trickle.write_all(b"{");
                thread::sleep(Duration::from_millis(10));
            }
        });
        thread::sleep(Duration::from_millis(70));
        let mut healthy = later_health(&client_listener);
        healthy
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut response = Vec::new();
        healthy.read_to_end(&mut response).unwrap();
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert!(!stop.load(Ordering::SeqCst));
        stop.store(true, Ordering::SeqCst);
        sender.join().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn deadline_aware_write_stops_incremental_progress() {
        struct ControlledWriter {
            now: Instant,
            step: Duration,
            chunk: usize,
            written: usize,
            progress: Vec<usize>,
            timeouts: Vec<Duration>,
        }

        impl DeadlineWriter for ControlledWriter {
            fn now(&self) -> Instant {
                self.now
            }

            fn set_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
                self.timeouts.push(timeout);
                Ok(())
            }

            fn write_chunk(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let count = self.chunk.min(bytes.len());
                self.written += count;
                self.progress.push(count);
                self.now += self.step;
                Ok(count)
            }
        }

        let start = Instant::now();
        let mut writer = ControlledWriter {
            now: start,
            step: Duration::from_millis(3),
            chunk: 2,
            written: 0,
            progress: Vec::new(),
            timeouts: Vec::new(),
        };
        let body = [0_u8; 10];
        let error = write_all_deadline_with(&mut writer, &body, start + Duration::from_millis(9))
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(writer.progress, [2, 2, 2]);
        assert!(writer.progress.iter().all(|count| *count > 0));
        assert_eq!(writer.written, 6, "three partial writes made progress");
        assert!(writer.written < body.len());
        assert_eq!(
            writer.timeouts,
            [
                Duration::from_millis(9),
                Duration::from_millis(6),
                Duration::from_millis(3),
            ]
        );
        assert!(writer.timeouts.windows(2).all(|pair| pair[1] < pair[0]));
    }

    #[test]
    fn accept_error_policy_backs_off_and_escapes() {
        assert!(retryable_accept_error(&std::io::Error::from(
            std::io::ErrorKind::Interrupted
        )));
        assert!(!retryable_accept_error(&std::io::Error::from(
            std::io::ErrorKind::InvalidInput
        )));
        let mut consecutive = 0;
        assert_eq!(retry_accept(&mut consecutive), Some(ACCEPT_BACKOFF));
        consecutive = 0; // a successful accept resets the counter
        assert_eq!(retry_accept(&mut consecutive), Some(ACCEPT_BACKOFF));
        assert_eq!(retry_accept(&mut consecutive), Some(ACCEPT_BACKOFF * 2));
        assert_eq!(retry_accept(&mut consecutive), None);
    }
}
