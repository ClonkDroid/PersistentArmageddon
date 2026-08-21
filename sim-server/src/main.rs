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
    use sim_core::{
        Activity, ApplyOutcome, CasualtyState, EntityId, InterruptionReason, Inventory, LifeState,
        LivingState, Needs, Position, ResourceTotals, Role, Soldier, Treatment, TreatmentStatus,
        Wound, BLOOD_MAX, HEMOSTATIC_COST, HEMOSTATIC_DURATION, SHOCK_TREATMENT_COST,
        SHOCK_TREATMENT_DURATION,
    };
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
        assert_eq!(
            outcome,
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::StockpileCreated {
                        id: 9,
                        initial: Stock {
                            ammunition: 17,
                            supplies: 6,
                        },
                    },
                }],
                error: None,
                blocked: None,
            }
        );
        world
    }

    fn assert_pinned_http_fixture(world: &World) {
        assert_eq!(world.clock(), 0);
        assert_eq!(world.soldier_count(), 0);
        for raw in [0, 1, 77, u64::MAX] {
            let id = sim_core::EntityId::from_raw(raw);
            assert_eq!(world.soldier(id), None);
            assert_eq!(world.casualty_state(id), None);
            assert_eq!(world.wounds_of(id), Vec::<sim_core::Wound>::new());
        }
        for id in [WoundId(0), WoundId(1), WoundId(u64::MAX)] {
            assert_eq!(world.wound(id), None);
        }
        for id in [TreatmentId(0), TreatmentId(1), TreatmentId(u64::MAX)] {
            assert_eq!(world.treatment(id), None);
        }
        assert_eq!(
            world.stockpile(9),
            Some(Stock {
                ammunition: 17,
                supplies: 6,
            })
        );
        assert_eq!(world.stockpile(8), None);
        assert_eq!(world.squad(0), None);
        assert_eq!(world.hot_cell(0), None);
        assert_eq!(world.hot_cell_count(), 0);
        assert_eq!(
            world.resource_totals(),
            sim_core::ResourceTotals {
                ammunition: 17,
                stockpile_supplies: 6,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            }
        );
        assert_eq!(world.state_digest(), 0x2bd7_8a3a_f7ee_2a90);
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
        struct LegacyWorldFixture {
            clock: u64,
            soldier_count: usize,
            absent_soldiers: [EntityId; 2],
            empty_wounds_of: EntityId,
            absent_wounds: [WoundId; 2],
            casualty: (EntityId, Option<CasualtyState>),
            absent_treatments: [TreatmentId; 2],
            stockpile: (u32, Option<Stock>),
            unrelated_stockpile: u32,
            absent_squad: u32,
            absent_hot_cell: u32,
            hot_cell_count: usize,
            totals: ResourceTotals,
            digest: u64,
        }

        impl LegacyWorldFixture {
            fn assert_world(&self, world: &World, name: &str) {
                assert_eq!(world.clock(), self.clock, "{name}");
                assert_eq!(world.soldier_count(), self.soldier_count, "{name}");
                for id in self.absent_soldiers {
                    assert_eq!(world.soldier(id), None, "{name}");
                }
                assert_eq!(world.wounds_of(self.empty_wounds_of), vec![], "{name}");
                for id in self.absent_wounds {
                    assert_eq!(world.wound(id), None, "{name}");
                }
                assert_eq!(
                    world.casualty_state(self.casualty.0),
                    self.casualty.1,
                    "{name}"
                );
                for id in self.absent_treatments {
                    assert_eq!(world.treatment(id), None, "{name}");
                }
                assert_eq!(
                    world.stockpile(self.stockpile.0),
                    self.stockpile.1,
                    "{name}"
                );
                assert_eq!(world.stockpile(self.unrelated_stockpile), None, "{name}");
                assert_eq!(world.squad(self.absent_squad), None, "{name}");
                assert_eq!(world.hot_cell(self.absent_hot_cell), None, "{name}");
                assert_eq!(world.hot_cell_count(), self.hot_cell_count, "{name}");
                assert_eq!(world.resource_totals(), self.totals, "{name}");
                assert_eq!(world.state_digest(), self.digest, "{name}");
            }
        }

        struct LegacySuccess {
            name: &'static str,
            seed: u64,
            initial: LegacyWorldFixture,
            body: &'static str,
            response: Value,
            final_state: LegacyWorldFixture,
        }
        let legacy = [
            LegacySuccess {
                name: "null-heavy create_stockpile",
                seed: 31,
                initial: LegacyWorldFixture {
                    clock: 0,
                    soldier_count: 0,
                    absent_soldiers: [EntityId::from_parts(0, 0), EntityId::from_parts(7, 0)],
                    empty_wounds_of: EntityId::from_parts(0, 0),
                    absent_wounds: [WoundId(0), WoundId(7)],
                    casualty: (EntityId::from_parts(0, 0), None),
                    absent_treatments: [TreatmentId(0), TreatmentId(7)],
                    stockpile: (7, None),
                    unrelated_stockpile: 8,
                    absent_squad: 7,
                    absent_hot_cell: 7,
                    hot_cell_count: 0,
                    totals: ResourceTotals {
                        ammunition: 0,
                        stockpile_supplies: 0,
                        carried_food: 0,
                        carried_water: 0,
                        carried_medical: 0,
                        sourced_food: 0,
                        sourced_water: 0,
                        consumed_food: 0,
                        consumed_water: 0,
                        lost_food: 0,
                        lost_water: 0,
                        sourced_medical: 0,
                        consumed_medical: 0,
                        lost_medical: 0,
                    },
                    digest: 0x0c06_8a8d_648b_9f5d,
                },
                body: r#"{"version":1,"command":"create_stockpile","id":7,"ammunition":9,"supplies":3,"target":null,"from":null,"to":null,"cell":null,"hot":null,"at":null,"activity":null,"patient":null,"medic":null,"wound":null,"trauma":null,"bleeding_per_second":null,"shock":null,"kind":null,"treatment":null}"#,
                response: json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"stockpile_created","id":7,"initial":{"ammunition":9,"supplies":3}}}],"terminal_error":null,"blocked":null,"digest":"c333945e80f9b8d1"}),
                final_state: LegacyWorldFixture {
                    clock: 0,
                    soldier_count: 0,
                    absent_soldiers: [EntityId::from_parts(0, 0), EntityId::from_parts(7, 0)],
                    empty_wounds_of: EntityId::from_parts(0, 0),
                    absent_wounds: [WoundId(0), WoundId(7)],
                    casualty: (EntityId::from_parts(0, 0), None),
                    absent_treatments: [TreatmentId(0), TreatmentId(7)],
                    stockpile: (
                        7,
                        Some(Stock {
                            ammunition: 9,
                            supplies: 3,
                        }),
                    ),
                    unrelated_stockpile: 8,
                    absent_squad: 7,
                    absent_hot_cell: 7,
                    hot_cell_count: 0,
                    totals: ResourceTotals {
                        ammunition: 9,
                        stockpile_supplies: 3,
                        carried_food: 0,
                        carried_water: 0,
                        carried_medical: 0,
                        sourced_food: 0,
                        sourced_water: 0,
                        consumed_food: 0,
                        consumed_water: 0,
                        lost_food: 0,
                        lost_water: 0,
                        sourced_medical: 0,
                        consumed_medical: 0,
                        lost_medical: 0,
                    },
                    digest: 0xc333_945e_80f9_b8d1,
                },
            },
            LegacySuccess {
                name: "null-heavy advance_to",
                seed: 31,
                initial: LegacyWorldFixture {
                    clock: 0,
                    soldier_count: 0,
                    absent_soldiers: [EntityId::from_parts(0, 0), EntityId::from_parts(7, 0)],
                    empty_wounds_of: EntityId::from_parts(0, 0),
                    absent_wounds: [WoundId(0), WoundId(7)],
                    casualty: (EntityId::from_parts(0, 0), None),
                    absent_treatments: [TreatmentId(0), TreatmentId(7)],
                    stockpile: (7, None),
                    unrelated_stockpile: 8,
                    absent_squad: 7,
                    absent_hot_cell: 7,
                    hot_cell_count: 0,
                    totals: ResourceTotals {
                        ammunition: 0,
                        stockpile_supplies: 0,
                        carried_food: 0,
                        carried_water: 0,
                        carried_medical: 0,
                        sourced_food: 0,
                        sourced_water: 0,
                        consumed_food: 0,
                        consumed_water: 0,
                        lost_food: 0,
                        lost_water: 0,
                        sourced_medical: 0,
                        consumed_medical: 0,
                        lost_medical: 0,
                    },
                    digest: 0x0c06_8a8d_648b_9f5d,
                },
                body: r#"{"version":1,"command":"advance_to","target":1,"id":null,"from":null,"to":null,"ammunition":null,"supplies":null,"cell":null,"hot":null,"at":null,"activity":null,"patient":null,"medic":null,"wound":null,"trauma":null,"bleeding_per_second":null,"shock":null,"kind":null,"treatment":null}"#,
                response: json!({"version":1,"clock":1,"events":[{"at":1,"event":{"type":"time_advanced","from":0,"to":1,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"4215b2950c54e5bc"}),
                final_state: LegacyWorldFixture {
                    clock: 1,
                    soldier_count: 0,
                    absent_soldiers: [EntityId::from_parts(0, 0), EntityId::from_parts(7, 0)],
                    empty_wounds_of: EntityId::from_parts(0, 0),
                    absent_wounds: [WoundId(0), WoundId(7)],
                    casualty: (EntityId::from_parts(0, 0), None),
                    absent_treatments: [TreatmentId(0), TreatmentId(7)],
                    stockpile: (7, None),
                    unrelated_stockpile: 8,
                    absent_squad: 7,
                    absent_hot_cell: 7,
                    hot_cell_count: 0,
                    totals: ResourceTotals {
                        ammunition: 0,
                        stockpile_supplies: 0,
                        carried_food: 0,
                        carried_water: 0,
                        carried_medical: 0,
                        sourced_food: 0,
                        sourced_water: 0,
                        consumed_food: 0,
                        consumed_water: 0,
                        lost_food: 0,
                        lost_water: 0,
                        sourced_medical: 0,
                        consumed_medical: 0,
                        lost_medical: 0,
                    },
                    digest: 0x4215_b295_0c54_e5bc,
                },
            },
        ];
        for fixture in legacy {
            let initial = World::new(fixture.seed);
            fixture.initial.assert_world(&initial, fixture.name);
            let (response, world) = exchange(req(fixture.body), false, initial);
            assert_eq!(status(&response), "HTTP/1.1 200 OK", "{}", fixture.name);
            assert_eq!(json_body(&response), fixture.response, "{}", fixture.name);
            fixture.final_state.assert_world(&world, fixture.name);
            let bytes = world.snapshot();
            let restored = World::from_snapshot(&bytes).unwrap();
            fixture.final_state.assert_world(&restored, fixture.name);
            assert_eq!(restored.snapshot(), bytes, "{}", fixture.name);
            assert_eq!(
                restored.state_digest(),
                world.state_digest(),
                "{}",
                fixture.name
            );
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

    fn spawn_literal(world: &mut World, id: EntityId, role: Role, medical: u32) {
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
        assert_eq!(outcome.clock, 0);
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.blocked, None);
        assert_eq!(
            outcome.events,
            vec![TimedEvent {
                at: 0,
                event: Event::SoldierSpawned {
                    id,
                    loadout: sim_core::Loadout {
                        ammunition: 0,
                        food: 0,
                        water: 0,
                        medical,
                    },
                },
            }]
        );
        let soldier = world.soldier(id).unwrap();
        assert_eq!(soldier.id, id);
        assert_eq!(soldier.role, role);
        assert_eq!(soldier.health, 1000);
        assert_eq!(
            soldier.inventory,
            Inventory {
                food: 0,
                water: 0,
                medical
            }
        );
    }

    #[derive(Clone)]
    struct PublicMedicalFixture {
        name: &'static str,
        clock: u64,
        soldier_count: usize,
        soldiers: Vec<Soldier>,
        absent_soldiers: Vec<EntityId>,
        wounds: Vec<Wound>,
        wounds_of: Vec<(EntityId, Vec<Wound>)>,
        absent_wounds: Vec<WoundId>,
        casualties: Vec<(EntityId, Option<CasualtyState>)>,
        treatments: Vec<Treatment>,
        absent_treatments: Vec<TreatmentId>,
        totals: ResourceTotals,
        absent_stockpiles: Vec<u32>,
        absent_squads: Vec<u32>,
        absent_hot_cells: Vec<u32>,
        hot_cell_count: usize,
        digest: u64,
    }

    impl PublicMedicalFixture {
        fn assert_world(&self, world: &World) {
            assert_eq!(world.clock(), self.clock, "{} clock", self.name);
            assert_eq!(
                world.soldier_count(),
                self.soldier_count,
                "{} count",
                self.name
            );
            for expected in &self.soldiers {
                assert_eq!(
                    world.soldier(expected.id),
                    Some(*expected),
                    "{} soldier {:?}",
                    self.name,
                    expected.id
                );
            }
            for id in &self.absent_soldiers {
                assert_eq!(
                    world.soldier(*id),
                    None,
                    "{} absent soldier {id:?}",
                    self.name
                );
            }
            for expected in &self.wounds {
                assert_eq!(
                    world.wound(expected.id),
                    Some(*expected),
                    "{} wound {:?}",
                    self.name,
                    expected.id
                );
            }
            for (patient, expected) in &self.wounds_of {
                assert_eq!(
                    world.wounds_of(*patient),
                    *expected,
                    "{} wounds_of {patient:?}",
                    self.name
                );
            }
            for id in &self.absent_wounds {
                assert_eq!(world.wound(*id), None, "{} absent wound {id:?}", self.name);
            }
            for (id, expected) in &self.casualties {
                assert_eq!(
                    world.casualty_state(*id),
                    *expected,
                    "{} casualty {id:?}",
                    self.name
                );
            }
            for expected in &self.treatments {
                assert_eq!(
                    world.treatment(expected.id),
                    Some(*expected),
                    "{} treatment {:?}",
                    self.name,
                    expected.id
                );
            }
            for id in &self.absent_treatments {
                assert_eq!(
                    world.treatment(*id),
                    None,
                    "{} absent treatment {id:?}",
                    self.name
                );
            }
            assert_eq!(
                world.resource_totals(),
                self.totals,
                "{} resources",
                self.name
            );
            for id in &self.absent_stockpiles {
                assert_eq!(world.stockpile(*id), None, "{} stockpile {id}", self.name);
            }
            for id in &self.absent_squads {
                assert_eq!(world.squad(*id), None, "{} squad {id}", self.name);
            }
            for id in &self.absent_hot_cells {
                assert_eq!(world.hot_cell(*id), None, "{} hot cell {id}", self.name);
            }
            assert_eq!(
                world.hot_cell_count(),
                self.hot_cell_count,
                "{} hot cell count",
                self.name
            );
            assert_eq!(world.state_digest(), self.digest, "{} digest", self.name);
        }
    }

    fn verified_restore(world: &World, fixture: &PublicMedicalFixture) -> World {
        fixture.assert_world(world);
        let bytes = world.snapshot();
        let restored = World::from_snapshot(&bytes).unwrap();
        fixture.assert_world(&restored);
        assert_eq!(restored.snapshot(), bytes, "{} bytes", fixture.name);
        assert_eq!(
            restored.state_digest(),
            world.state_digest(),
            "{} parity",
            fixture.name
        );
        restored
    }

    fn direct_setup_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);
        let _living = LivingState {
            hunger: 0,
            thirst: 0,
            fatigue: 0,
            sleep_debt: 0,
            morale: 1000,
            health: 1000,
            activity: Activity::Idle,
            life: LifeState::Alive,
            materialized_at: 0,
        };
        let _needs = Needs {
            fatigue: 0,
            hunger: 0,
            thirst: 0,
            sleep_debt: 0,
        };
        PublicMedicalFixture {
            name: "direct setup",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 5,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![],
            wounds_of: vec![(medic, vec![]), (patient, vec![])],
            absent_wounds: vec![WoundId(0), WoundId(1), WoundId(99)],
            casualties: vec![(medic, None), (patient, None)],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 5,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x7d80_eee1_2482_f06b,
        }
    }

    fn shock_setup_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);
        let _living = LivingState {
            hunger: 0,
            thirst: 0,
            fatigue: 0,
            sleep_debt: 0,
            morale: 1000,
            health: 1000,
            activity: Activity::Idle,
            life: LifeState::Alive,
            materialized_at: 0,
        };
        let _needs = Needs {
            fatigue: 0,
            hunger: 0,
            thirst: 0,
            sleep_debt: 0,
        };
        PublicMedicalFixture {
            name: "Shock setup",
            clock: 0,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![],
            wounds_of: vec![(m0, vec![]), (m1, vec![]), (patient, vec![])],
            absent_wounds: vec![WoundId(0), WoundId(1), WoundId(99)],
            casualties: vec![(m0, None), (m1, None), (patient, None)],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 8,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xd762_f1d6_6467_7e05,
        }
    }

    fn healing_setup_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);
        let _living = LivingState {
            hunger: 0,
            thirst: 0,
            fatigue: 0,
            sleep_debt: 0,
            morale: 1000,
            health: 1000,
            activity: Activity::Idle,
            life: LifeState::Alive,
            materialized_at: 0,
        };
        let _needs = Needs {
            fatigue: 0,
            hunger: 0,
            thirst: 0,
            sleep_debt: 0,
        };
        PublicMedicalFixture {
            name: "healing setup",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 3,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![],
            wounds_of: vec![(medic, vec![]), (patient, vec![])],
            absent_wounds: vec![WoundId(0), WoundId(1), WoundId(99)],
            casualties: vec![(medic, None), (patient, None)],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 3,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xa531_059e_3401_271d,
        }
    }

    fn direct_second_wound_probe_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct second wound probe",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 5,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 989,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 989,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                    controlled: false,
                    healed: false,
                },
                Wound {
                    id: WoundId(1),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 1,
                        bleeding_per_second: 0,
                        shock: 0,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![
                        Wound {
                            id: WoundId(0),
                            patient,
                            created_at: 0,
                            spec: WoundSpec {
                                trauma: 10,
                                bleeding_per_second: 20,
                                shock: 30,
                            },
                            controlled: false,
                            healed: false,
                        },
                        Wound {
                            id: WoundId(1),
                            patient,
                            created_at: 0,
                            spec: WoundSpec {
                                trauma: 1,
                                bleeding_per_second: 0,
                                shock: 0,
                            },
                            controlled: false,
                            healed: false,
                        },
                    ],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 5,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xcbc8_0aac_57eb_9e72,
        }
    }

    fn direct_busy_probe_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);
        let endpoint = EntityId::from_parts(2, 0);

        PublicMedicalFixture {
            name: "direct busy probe",
            clock: 0,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: endpoint,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 999,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 999,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                    controlled: false,
                    healed: false,
                },
                Wound {
                    id: WoundId(1),
                    patient: endpoint,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 1,
                        bleeding_per_second: 1,
                        shock: 1,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (
                    endpoint,
                    vec![Wound {
                        id: WoundId(1),
                        patient: endpoint,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 1,
                            bleeding_per_second: 1,
                            shock: 1,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
                (
                    endpoint,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 1,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x0445_45b6_f217_e91e,
        }
    }

    fn direct_release_probe_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct release probe",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 3,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![
                Treatment {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    wound: Some(WoundId(0)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 0,
                    completes_at: 10,
                    consumed: 1,
                    status: TreatmentStatus::Interrupted {
                        at: 0,
                        reason: InterruptionReason::Explicit,
                    },
                },
                Treatment {
                    id: TreatmentId(1),
                    medic,
                    patient,
                    wound: Some(WoundId(0)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 0,
                    completes_at: 10,
                    consumed: 1,
                    status: TreatmentStatus::Active,
                },
            ],
            absent_treatments: vec![TreatmentId(2), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 3,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x8173_0898_6e9d_651c,
        }
    }

    fn shock_clock14_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);

        PublicMedicalFixture {
            name: "Shock clock 14",
            clock: 14,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 14,
                        hunger: 14,
                        thirst: 28,
                        sleep_debt: 14,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 14,
                        thirst: 28,
                        fatigue: 14,
                        sleep_debt: 14,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 14,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 14,
                        hunger: 14,
                        thirst: 28,
                        sleep_debt: 14,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 14,
                        thirst: 28,
                        fatigue: 14,
                        sleep_debt: 14,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 14,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 14,
                        hunger: 14,
                        thirst: 28,
                        sleep_debt: 14,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 14,
                        thirst: 28,
                        fatigue: 14,
                        sleep_debt: 14,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 14,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 14,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic: m0,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                started_at: 0,
                completes_at: 15,
                consumed: 2,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 6,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x1e51_e702_bade_c6d9,
        }
    }

    fn shock_clock15_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);

        PublicMedicalFixture {
            name: "Shock clock 15",
            clock: 15,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 15,
                        hunger: 15,
                        thirst: 30,
                        sleep_debt: 15,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 15,
                        thirst: 30,
                        fatigue: 15,
                        sleep_debt: 15,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 15,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 15,
                        hunger: 15,
                        thirst: 30,
                        sleep_debt: 15,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 15,
                        thirst: 30,
                        fatigue: 15,
                        sleep_debt: 15,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 15,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 15,
                        hunger: 15,
                        thirst: 30,
                        sleep_debt: 15,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 15,
                        thirst: 30,
                        fatigue: 15,
                        sleep_debt: 15,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 15,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 100,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 15,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic: m0,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                started_at: 0,
                completes_at: 15,
                consumed: 2,
                status: TreatmentStatus::Completed { at: 15 },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 6,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x24f6_1536_26fe_8e8e,
        }
    }

    fn shock_selection_wound_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);
        let patient2 = EntityId::from_parts(3, 0);

        PublicMedicalFixture {
            name: "Shock second patient post-wound",
            clock: 0,
            soldier_count: 4,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient2,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(4, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 400,
                    },
                    controlled: false,
                    healed: false,
                },
                Wound {
                    id: WoundId(1),
                    patient: patient2,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 400,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (
                    patient2,
                    vec![Wound {
                        id: WoundId(1),
                        patient: patient2,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
                (
                    patient2,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic: m0,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                started_at: 0,
                completes_at: 15,
                consumed: 2,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 6,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x614e_8f27_88f6_8084,
        }
    }

    fn shock_selection_probe_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);
        let patient2 = EntityId::from_parts(3, 0);

        PublicMedicalFixture {
            name: "Shock second selection",
            clock: 0,
            soldier_count: 4,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient2,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(4, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 400,
                    },
                    controlled: false,
                    healed: false,
                },
                Wound {
                    id: WoundId(1),
                    patient: patient2,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 400,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (
                    patient2,
                    vec![Wound {
                        id: WoundId(1),
                        patient: patient2,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
                (
                    patient2,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![
                Treatment {
                    id: TreatmentId(0),
                    medic: m0,
                    patient,
                    wound: None,
                    kind: TreatmentKind::Shock,
                    started_at: 0,
                    completes_at: 15,
                    consumed: 2,
                    status: TreatmentStatus::Active,
                },
                Treatment {
                    id: TreatmentId(1),
                    medic: m1,
                    patient: patient2,
                    wound: None,
                    kind: TreatmentKind::Shock,
                    started_at: 0,
                    completes_at: 15,
                    consumed: 2,
                    status: TreatmentStatus::Active,
                },
            ],
            absent_treatments: vec![TreatmentId(2), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 4,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x8ba6_8273_9c77_a3f3,
        }
    }

    fn healing_post_wound_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing post-wound",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 3,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 3,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xe409_bfcc_d97b_10f5,
        }
    }

    fn shock_post_wound_fixture() -> PublicMedicalFixture {
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);

        PublicMedicalFixture {
            name: "Shock post-wound",
            clock: 0,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 8,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x546a_00d5_95ec_0cfd,
        }
    }

    fn direct_post_wound_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct post-wound",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 5,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 5,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x1a64_4c80_0f5d_fd57,
        }
    }

    fn direct_active_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct active",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xd3b2_b246_54f6_f170,
        }
    }

    fn direct_interrupted_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct interrupted",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Interrupted {
                    at: 0,
                    reason: InterruptionReason::Explicit,
                },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xaa8a_d8b6_9e55_cfe2,
        }
    }

    fn direct_clock20_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "direct clock 20",
            clock: 20,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 4600,
                        shock: 70,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 20,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Interrupted {
                    at: 0,
                    reason: InterruptionReason::Explicit,
                },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x035c_8d0e_76da_8786,
        }
    }

    fn healing_active_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing active",
            clock: 0,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 30,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xfe25_5699_6b16_d7b6,
        }
    }

    fn healing_clock9_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing boundary minus one",
            clock: 9,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 9,
                        hunger: 9,
                        thirst: 18,
                        sleep_debt: 9,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 9,
                        thirst: 18,
                        fatigue: 9,
                        sleep_debt: 9,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 9,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 9,
                        hunger: 9,
                        thirst: 18,
                        sleep_debt: 9,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 9,
                        thirst: 18,
                        fatigue: 9,
                        sleep_debt: 9,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 9,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 4820,
                        shock: 48,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 9,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x9937_4e12_7a15_569d,
        }
    }

    fn healing_completed_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing completed",
            clock: 10,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 10,
                        hunger: 10,
                        thirst: 20,
                        sleep_debt: 10,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 10,
                        thirst: 20,
                        fatigue: 10,
                        sleep_debt: 10,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 10,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 990,
                    needs: Needs {
                        fatigue: 10,
                        hunger: 10,
                        thirst: 20,
                        sleep_debt: 10,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 10,
                        thirst: 20,
                        fatigue: 10,
                        sleep_debt: 10,
                        morale: 1000,
                        health: 990,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 10,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: true,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: true,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 4800,
                        shock: 50,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: true,
                        recovery_next_at: Some(15),
                        materialized_at: 10,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Completed { at: 10 },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xf10d_c636_c78b_e1da,
        }
    }

    fn healing_clock15_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing tick",
            clock: 15,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 15,
                        hunger: 15,
                        thirst: 30,
                        sleep_debt: 15,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 15,
                        thirst: 30,
                        fatigue: 15,
                        sleep_debt: 15,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 15,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 15,
                        hunger: 15,
                        thirst: 30,
                        sleep_debt: 15,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 15,
                        thirst: 30,
                        fatigue: 15,
                        sleep_debt: 15,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 15,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: true,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: true,
                        healed: false,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 4900,
                        shock: 0,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: true,
                        recovery_next_at: Some(20),
                        materialized_at: 15,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Completed { at: 10 },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x29e9_214b_52e8_e3e4,
        }
    }

    fn healing_terminal_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing terminal",
            clock: 20,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: true,
                healed: true,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: true,
                        healed: true,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 0,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 20,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Completed { at: 10 },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xbce6_a737_b43e_2693,
        }
    }

    fn healing_completed_probe_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing completed allocation probe",
            clock: 10,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 10,
                        hunger: 10,
                        thirst: 20,
                        sleep_debt: 10,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 1,
                    },
                    living: LivingState {
                        hunger: 10,
                        thirst: 20,
                        fatigue: 10,
                        sleep_debt: 10,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 10,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 989,
                    needs: Needs {
                        fatigue: 10,
                        hunger: 10,
                        thirst: 20,
                        sleep_debt: 10,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 10,
                        thirst: 20,
                        fatigue: 10,
                        sleep_debt: 10,
                        morale: 1000,
                        health: 989,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 10,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                    controlled: true,
                    healed: false,
                },
                Wound {
                    id: WoundId(1),
                    patient,
                    created_at: 10,
                    spec: WoundSpec {
                        trauma: 1,
                        bleeding_per_second: 1,
                        shock: 1,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![
                        Wound {
                            id: WoundId(0),
                            patient,
                            created_at: 0,
                            spec: WoundSpec {
                                trauma: 10,
                                bleeding_per_second: 20,
                                shock: 30,
                            },
                            controlled: true,
                            healed: false,
                        },
                        Wound {
                            id: WoundId(1),
                            patient,
                            created_at: 10,
                            spec: WoundSpec {
                                trauma: 1,
                                bleeding_per_second: 1,
                                shock: 1,
                            },
                            controlled: false,
                            healed: false,
                        },
                    ],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 4800,
                        shock: 51,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 10,
                    }),
                ),
            ],
            treatments: vec![
                Treatment {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    wound: Some(WoundId(0)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 0,
                    completes_at: 10,
                    consumed: 1,
                    status: TreatmentStatus::Completed { at: 10 },
                },
                Treatment {
                    id: TreatmentId(1),
                    medic,
                    patient,
                    wound: Some(WoundId(1)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 10,
                    completes_at: 20,
                    consumed: 1,
                    status: TreatmentStatus::Active,
                },
            ],
            absent_treatments: vec![TreatmentId(2), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 1,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x83ea_2a6d_9931_db00,
        }
    }

    fn healing_final_probe_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing final allocation probe",
            clock: 20,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 1,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 999,
                    needs: Needs {
                        fatigue: 20,
                        hunger: 20,
                        thirst: 40,
                        sleep_debt: 20,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 20,
                        thirst: 40,
                        fatigue: 20,
                        sleep_debt: 20,
                        morale: 1000,
                        health: 999,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 20,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![
                Wound {
                    id: WoundId(0),
                    patient,
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 10,
                        bleeding_per_second: 20,
                        shock: 30,
                    },
                    controlled: true,
                    healed: true,
                },
                Wound {
                    id: WoundId(1),
                    patient,
                    created_at: 20,
                    spec: WoundSpec {
                        trauma: 1,
                        bleeding_per_second: 1,
                        shock: 1,
                    },
                    controlled: false,
                    healed: false,
                },
            ],
            wounds_of: vec![
                (
                    patient,
                    vec![
                        Wound {
                            id: WoundId(0),
                            patient,
                            created_at: 0,
                            spec: WoundSpec {
                                trauma: 10,
                                bleeding_per_second: 20,
                                shock: 30,
                            },
                            controlled: true,
                            healed: true,
                        },
                        Wound {
                            id: WoundId(1),
                            patient,
                            created_at: 20,
                            spec: WoundSpec {
                                trauma: 1,
                                bleeding_per_second: 1,
                                shock: 1,
                            },
                            controlled: false,
                            healed: false,
                        },
                    ],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(2), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 1,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 20,
                    }),
                ),
            ],
            treatments: vec![
                Treatment {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    wound: Some(WoundId(0)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 0,
                    completes_at: 10,
                    consumed: 1,
                    status: TreatmentStatus::Completed { at: 10 },
                },
                Treatment {
                    id: TreatmentId(1),
                    medic,
                    patient,
                    wound: Some(WoundId(1)),
                    kind: TreatmentKind::Hemostatic,
                    started_at: 20,
                    completes_at: 30,
                    consumed: 1,
                    status: TreatmentStatus::Active,
                },
            ],
            absent_treatments: vec![TreatmentId(2), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 1,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x90e9_fd5a_5da7_fa0e,
        }
    }

    fn healing_clock21_fixture() -> PublicMedicalFixture {
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);

        PublicMedicalFixture {
            name: "healing clock 21",
            clock: 21,
            soldier_count: 2,
            soldiers: vec![
                Soldier {
                    id: medic,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 21,
                        hunger: 21,
                        thirst: 42,
                        sleep_debt: 21,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 21,
                        thirst: 42,
                        fatigue: 21,
                        sleep_debt: 21,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 21,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 21,
                        hunger: 21,
                        thirst: 42,
                        sleep_debt: 21,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 21,
                        thirst: 42,
                        fatigue: 21,
                        sleep_debt: 21,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 21,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(2, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30,
                },
                controlled: true,
                healed: true,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30,
                        },
                        controlled: true,
                        healed: true,
                    }],
                ),
                (medic, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (medic, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 0,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 21,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Completed { at: 10 },
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0x94c4_9cc6_10b4_3c74,
        }
    }

    #[test]
    fn gate_c2_f3_wound_hemostatic_interruption_and_late_advance() {
        let mut world = World::new(11);
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);
        spawn_literal(&mut world, medic, Role::Medic, 5);
        spawn_literal(&mut world, patient, Role::Rifle, 0);
        let world = verified_restore(&world, &direct_setup_fixture());
        let wound_body = r#"{"version":1,"command":"inflict_wound","patient":1,"trauma":10,"bleeding_per_second":20,"shock":30}"#;
        let (response, after_wound) = exchange(req(wound_body), false, world);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"wound_inflicted","id":0,"patient":1,"trauma":10,"bleeding_per_second":20,"shock":30}}],"terminal_error":null,"blocked":null,"digest":"1a644c800f5dfd57"})
        );
        assert_eq!(after_wound.wound(WoundId(0)).unwrap().patient, patient);
        assert_eq!(
            after_wound.wounds_of(patient),
            vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30
                },
                controlled: false,
                healed: false
            }]
        );
        assert_eq!(
            after_wound.casualty_state(patient),
            Some(CasualtyState {
                blood: BLOOD_MAX,
                shock: 30,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 0
            })
        );
        assert_eq!(
            after_wound.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 5,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 0,
                lost_medical: 0,
            }
        );
        let wound_fixture = direct_post_wound_fixture();
        let after_wound = verified_restore(&after_wound, &wound_fixture);
        let mut wound_allocator_probe = verified_restore(&after_wound, &wound_fixture);
        assert_eq!(
            wound_allocator_probe.apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 0,
                    shock: 0
                }
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(1),
                        patient,
                        wound: WoundSpec {
                            trauma: 1,
                            bleeding_per_second: 0,
                            shock: 0
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(wound_allocator_probe.wound(WoundId(2)), None);
        assert_eq!(wound_allocator_probe.wounds_of(patient).len(), 2);
        verified_restore(&wound_allocator_probe, &direct_second_wound_probe_fixture());
        let start_body = r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#;
        let (response, active) = exchange(req(start_body), false, after_wound);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_started","id":0,"medic":0,"patient":1,"wound":0,"kind":"hemostatic","completes_at":10,"consumed":1}}],"terminal_error":null,"blocked":null,"digest":"d3b2b24654f6f170"})
        );
        assert_eq!(active.soldier(medic).unwrap().inventory.medical, 4);
        assert_eq!(active.resource_totals().consumed_medical, 1);
        assert_eq!(
            active.treatment(TreatmentId(0)),
            Some(Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: HEMOSTATIC_DURATION,
                consumed: HEMOSTATIC_COST,
                status: TreatmentStatus::Active
            })
        );
        assert_eq!(
            active.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let active_fixture = direct_active_fixture();
        let active = verified_restore(&active, &active_fixture);
        let mut busy_probe = verified_restore(&active, &active_fixture);
        let endpoint = EntityId::from_parts(2, 0);
        spawn_literal(&mut busy_probe, endpoint, Role::Rifle, 0);
        assert_eq!(
            busy_probe.apply(Command::InflictWound {
                patient: endpoint,
                wound: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 1,
                    shock: 1
                }
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(1),
                        patient: endpoint,
                        wound: WoundSpec {
                            trauma: 1,
                            bleeding_per_second: 1,
                            shock: 1
                        },
                    },
                }],
                error: None,
                blocked: None,
            }
        );
        let busy_fixture = direct_busy_probe_fixture();
        let mut busy_probe = verified_restore(&busy_probe, &busy_fixture);
        let busy_before = busy_probe.snapshot();
        assert_eq!(
            busy_probe.apply(Command::RequestTreatment {
                patient: endpoint,
                wound: Some(WoundId(1)),
                kind: TreatmentKind::Hemostatic
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![],
                error: Some(SimError::NoEligibleMedic),
                blocked: None
            }
        );
        assert_eq!(busy_probe.snapshot(), busy_before);
        verified_restore(&busy_probe, &busy_fixture);
        let interrupt = r#"{"version":1,"command":"interrupt_treatment","treatment":0}"#;
        let (response, interrupted) = exchange(req(interrupt), false, active);
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_interrupted","id":0,"reason":"explicit"}}],"terminal_error":null,"blocked":null,"digest":"aa8ad8b69e55cfe2"})
        );
        assert_eq!(
            interrupted.treatment(TreatmentId(0)),
            Some(Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: HEMOSTATIC_DURATION,
                consumed: HEMOSTATIC_COST,
                status: TreatmentStatus::Interrupted {
                    at: 0,
                    reason: InterruptionReason::Explicit
                }
            })
        );
        assert_eq!(
            interrupted.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let interrupted_fixture_value = direct_interrupted_fixture();
        let interrupted = verified_restore(&interrupted, &interrupted_fixture_value);
        let mut release_probe = verified_restore(&interrupted, &interrupted_fixture_value);
        assert_eq!(
            release_probe.apply(Command::StartTreatment {
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::TreatmentStarted {
                        id: TreatmentId(1),
                        medic,
                        patient,
                        wound: Some(WoundId(0)),
                        kind: TreatmentKind::Hemostatic,
                        completes_at: 10,
                        consumed: 1
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(release_probe.treatment(TreatmentId(2)), None);
        assert_eq!(release_probe.soldier(medic).unwrap().inventory.medical, 3);
        verified_restore(&release_probe, &direct_release_probe_fixture());
        let before = interrupted.snapshot();
        assert_eq!(interrupted.state_digest(), 0xaa8a_d8b6_9e55_cfe2);
        let malformed =
            r#"{"version":1,"command":"interrupt_treatment","treatment":0,"patient":null}"#;
        let (response, unchanged) = exchange(req(malformed), false, interrupted);
        assert_eq!(status(&response), "HTTP/1.1 400 Bad Request");
        assert_eq!(
            json_body(&response),
            json!({"error":"malformed_or_unsupported"})
        );
        assert_eq!(unchanged.snapshot(), before);
        assert_eq!(unchanged.state_digest(), 0xaa8a_d8b6_9e55_cfe2);
        interrupted_fixture_value.assert_world(&unchanged);
        let unchanged = verified_restore(&unchanged, &interrupted_fixture_value);
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
        assert_eq!(
            advanced.treatment(TreatmentId(0)).unwrap().status,
            TreatmentStatus::Interrupted {
                at: 0,
                reason: InterruptionReason::Explicit
            }
        );
        assert!(!advanced.wound(WoundId(0)).unwrap().controlled);
        assert_eq!(
            advanced.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 4,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let advanced_fixture = direct_clock20_fixture();
        verified_restore(&advanced, &advanced_fixture);
    }

    #[test]
    fn gate_c2_f3_requested_shock_selects_lowest_eligible_medic() {
        let mut world = World::new(23);
        let m0 = EntityId::from_parts(0, 0);
        let m1 = EntityId::from_parts(1, 0);
        let patient = EntityId::from_parts(2, 0);
        spawn_literal(&mut world, m0, Role::Medic, 4);
        spawn_literal(&mut world, m1, Role::Medic, 4);
        spawn_literal(&mut world, patient, Role::Rifle, 0);
        let mut world = verified_restore(&world, &shock_setup_fixture());
        let setup = world.apply(Command::InflictWound {
            patient,
            wound: WoundSpec {
                trauma: 0,
                bleeding_per_second: 0,
                shock: 400,
            },
        });
        assert_eq!(
            setup,
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(0),
                        patient,
                        wound: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(
            world.casualty_state(patient),
            Some(CasualtyState {
                blood: BLOOD_MAX,
                shock: 400,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 0
            })
        );
        assert_eq!(
            world.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 8,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 0,
                lost_medical: 0,
            }
        );
        let world = verified_restore(&world, &shock_post_wound_fixture());

        let (response, active) = exchange(
            req(r#"{"version":1,"command":"request_treatment","patient":2,"kind":"shock"}"#),
            false,
            world,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        let expected = json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_started","id":0,"medic":0,"patient":2,"wound":null,"kind":"shock","completes_at":15,"consumed":2}}],"terminal_error":null,"blocked":null,"digest":"d29ac157757f3347"});
        assert_eq!(json_body(&response), expected);
        assert_eq!(active.soldier(m0).unwrap().inventory.medical, 2);
        assert_eq!(active.soldier(m1).unwrap().inventory.medical, 4);
        assert_eq!(
            active.treatment(TreatmentId(0)),
            Some(Treatment {
                id: TreatmentId(0),
                medic: m0,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                started_at: 0,
                completes_at: SHOCK_TREATMENT_DURATION,
                consumed: SHOCK_TREATMENT_COST,
                status: TreatmentStatus::Active
            })
        );
        assert_eq!(
            active.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 6,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 2,
                lost_medical: 0,
            }
        );
        assert_eq!(
            active.wounds_of(patient),
            vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400
                },
                controlled: false,
                healed: false
            }]
        );
        assert_eq!(
            active.casualty_state(patient),
            Some(CasualtyState {
                blood: BLOOD_MAX,
                shock: 400,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 0
            })
        );
        let shock_fixture = PublicMedicalFixture {
            name: "requested Shock active",
            clock: 0,
            soldier_count: 3,
            soldiers: vec![
                Soldier {
                    id: m0,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 2,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: m1,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Medic,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 4,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
                Soldier {
                    id: patient,
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0,
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    needs: Needs {
                        fatigue: 0,
                        hunger: 0,
                        thirst: 0,
                        sleep_debt: 0,
                    },
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0,
                    },
                    living: LivingState {
                        hunger: 0,
                        thirst: 0,
                        fatigue: 0,
                        sleep_debt: 0,
                        morale: 1000,
                        health: 1000,
                        activity: Activity::Idle,
                        life: LifeState::Alive,
                        materialized_at: 0,
                    },
                },
            ],
            absent_soldiers: vec![EntityId::from_parts(3, 0), EntityId::from_parts(0, 1)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![
                (
                    patient,
                    vec![Wound {
                        id: WoundId(0),
                        patient,
                        created_at: 0,
                        spec: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400,
                        },
                        controlled: false,
                        healed: false,
                    }],
                ),
                (m0, vec![]),
                (m1, vec![]),
            ],
            absent_wounds: vec![WoundId(1), WoundId(99)],
            casualties: vec![
                (m0, None),
                (m1, None),
                (
                    patient,
                    Some(CasualtyState {
                        blood: 5000,
                        shock: 400,
                        shock_remainder: 0,
                        incapacitated: false,
                        recovering: false,
                        recovery_next_at: None,
                        materialized_at: 0,
                    }),
                ),
            ],
            treatments: vec![Treatment {
                id: TreatmentId(0),
                medic: m0,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                started_at: 0,
                completes_at: 15,
                consumed: 2,
                status: TreatmentStatus::Active,
            }],
            absent_treatments: vec![TreatmentId(1), TreatmentId(99)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 6,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 8,
                consumed_medical: 2,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: 0xd29a_c157_757f_3347,
        };
        let active = verified_restore(&active, &shock_fixture);

        let mut selection_probe = verified_restore(&active, &shock_fixture);
        let patient2 = EntityId::from_parts(3, 0);
        spawn_literal(&mut selection_probe, patient2, Role::Rifle, 0);
        assert_eq!(
            selection_probe.apply(Command::InflictWound {
                patient: patient2,
                wound: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 400
                }
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(1),
                        patient: patient2,
                        wound: WoundSpec {
                            trauma: 0,
                            bleeding_per_second: 0,
                            shock: 400
                        },
                    },
                }],
                error: None,
                blocked: None,
            }
        );
        shock_selection_wound_fixture().assert_world(&selection_probe);
        let selection_probe = verified_restore(&selection_probe, &shock_selection_wound_fixture());
        let mut selection_probe = selection_probe;
        assert_eq!(
            selection_probe.apply(Command::RequestTreatment {
                patient: patient2,
                wound: None,
                kind: TreatmentKind::Shock
            }),
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::TreatmentStarted {
                        id: TreatmentId(1),
                        medic: m1,
                        patient: patient2,
                        wound: None,
                        kind: TreatmentKind::Shock,
                        completes_at: 15,
                        consumed: 2
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(selection_probe.treatment(TreatmentId(2)), None);

        assert_eq!(selection_probe.soldier(m0).unwrap().inventory.medical, 2);
        assert_eq!(selection_probe.soldier(m1).unwrap().inventory.medical, 2);
        verified_restore(&selection_probe, &shock_selection_probe_fixture());

        let mut due_probe = verified_restore(&active, &shock_fixture);
        assert_eq!(
            due_probe.apply(Command::AdvanceTo { target: 14 }),
            sim_core::ApplyOutcome {
                clock: 14,
                events: vec![TimedEvent {
                    at: 14,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 14,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(
            due_probe.treatment(TreatmentId(0)).unwrap().status,
            TreatmentStatus::Active
        );
        let mut due_probe = verified_restore(&due_probe, &shock_clock14_fixture());
        let completion = due_probe.apply(Command::AdvanceTo { target: 15 });
        assert_eq!(completion.clock, 15);

        assert_eq!(completion.error, None);
        assert_eq!(completion.blocked, None);
        assert_eq!(
            completion.events,
            vec![
                TimedEvent {
                    at: 15,
                    event: Event::TreatmentCompleted {
                        id: TreatmentId(0),
                        medic: m0,
                        patient,
                        kind: TreatmentKind::Shock
                    }
                },
                TimedEvent {
                    at: 15,
                    event: Event::TimeAdvanced {
                        from: 14,
                        to: 15,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                }
            ]
        );
        assert_eq!(
            due_probe.casualty_state(patient),
            Some(CasualtyState {
                blood: 5000,
                shock: 100,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 15
            })
        );
        verified_restore(&due_probe, &shock_clock15_fixture());
    }

    #[test]
    fn gate_c2_f3_hemostatic_completion_recovery_and_healing() {
        let mut world = World::new(29);
        let medic = EntityId::from_parts(0, 0);
        let patient = EntityId::from_parts(1, 0);
        spawn_literal(&mut world, medic, Role::Medic, 3);
        spawn_literal(&mut world, patient, Role::Rifle, 0);
        let mut world = verified_restore(&world, &healing_setup_fixture());
        let setup = world.apply(Command::InflictWound {
            patient,
            wound: WoundSpec {
                trauma: 10,
                bleeding_per_second: 20,
                shock: 30,
            },
        });
        assert_eq!(
            setup,
            sim_core::ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(0),
                        patient,
                        wound: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        let world = verified_restore(&world, &healing_post_wound_fixture());

        let (response, active) = exchange(
            req(
                r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#,
            ),
            false,
            world,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":0,"events":[{"at":0,"event":{"type":"treatment_started","id":0,"medic":0,"patient":1,"wound":0,"kind":"hemostatic","completes_at":10,"consumed":1}}],"terminal_error":null,"blocked":null,"digest":"fe2556996b16d7b6"})
        );
        assert_eq!(
            active.treatment(TreatmentId(0)),
            Some(Treatment {
                id: TreatmentId(0),
                medic,
                patient,
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic,
                started_at: 0,
                completes_at: 10,
                consumed: 1,
                status: TreatmentStatus::Active
            })
        );
        assert_eq!(
            active.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let active_fixture = healing_active_fixture();
        let active = verified_restore(&active, &active_fixture);

        let (response, before) = exchange(
            req(r#"{"version":1,"command":"advance_to","target":9}"#),
            false,
            active,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":9,"events":[{"at":9,"event":{"type":"time_advanced","from":0,"to":9,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"99374e127a15569d"})
        );
        assert_eq!(
            before.treatment(TreatmentId(0)).unwrap().status,
            TreatmentStatus::Active
        );
        assert!(!before.wound(WoundId(0)).unwrap().controlled);
        assert_eq!(
            before.casualty_state(patient),
            Some(CasualtyState {
                blood: 4820,
                shock: 48,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 9
            })
        );
        assert_eq!(
            before.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let before_fixture = healing_clock9_fixture();
        let before = verified_restore(&before, &before_fixture);

        let (response, completed) = exchange(
            req(r#"{"version":1,"command":"advance_to","target":10}"#),
            false,
            before,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":10,"events":[{"at":10,"event":{"type":"treatment_completed","id":0,"medic":0,"patient":1,"kind":"hemostatic"}},{"at":10,"event":{"type":"recovery_changed","id":1,"before":false,"after":true,"next_at":15}},{"at":10,"event":{"type":"time_advanced","from":9,"to":10,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"f10dc636c78be1da"})
        );
        assert_eq!(
            completed.treatment(TreatmentId(0)).unwrap().status,
            TreatmentStatus::Completed { at: 10 }
        );
        assert_eq!(
            completed.wound(WoundId(0)).unwrap(),
            Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30
                },
                controlled: true,
                healed: false
            }
        );
        assert_eq!(
            completed.casualty_state(patient),
            Some(CasualtyState {
                blood: 4800,
                shock: 50,
                shock_remainder: 0,
                incapacitated: false,
                recovering: true,
                recovery_next_at: Some(15),
                materialized_at: 10
            })
        );
        assert_eq!(completed.soldier(patient).unwrap().health, 990);
        assert_eq!(
            completed.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let completed_fixture = healing_completed_fixture();
        let completed = verified_restore(&completed, &completed_fixture);
        let mut completed_release_probe = verified_restore(&completed, &completed_fixture);
        assert_eq!(
            completed_release_probe.apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 1,
                    shock: 1
                }
            }),
            sim_core::ApplyOutcome {
                clock: 10,
                events: vec![
                    TimedEvent {
                        at: 10,
                        event: Event::WoundInflicted {
                            id: WoundId(1),
                            patient,
                            wound: WoundSpec {
                                trauma: 1,
                                bleeding_per_second: 1,
                                shock: 1
                            }
                        }
                    },
                    TimedEvent {
                        at: 10,
                        event: Event::RecoveryChanged {
                            id: patient,
                            before: true,
                            after: false,
                            next_at: None
                        }
                    }
                ],
                error: None,
                blocked: None
            }
        );
        assert_eq!(
            completed_release_probe.apply(Command::StartTreatment {
                medic,
                patient,
                wound: Some(WoundId(1)),
                kind: TreatmentKind::Hemostatic
            }),
            sim_core::ApplyOutcome {
                clock: 10,
                events: vec![TimedEvent {
                    at: 10,
                    event: Event::TreatmentStarted {
                        id: TreatmentId(1),
                        medic,
                        patient,
                        wound: Some(WoundId(1)),
                        kind: TreatmentKind::Hemostatic,
                        completes_at: 20,
                        consumed: 1
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(completed_release_probe.wound(WoundId(2)), None);
        assert_eq!(completed_release_probe.treatment(TreatmentId(2)), None);
        verified_restore(&completed_release_probe, &healing_completed_probe_fixture());

        let (response, tick) = exchange(
            req(r#"{"version":1,"command":"advance_to","target":15}"#),
            false,
            completed,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":15,"events":[{"at":15,"event":{"type":"recovery_ticked","id":1,"blood_before":4800,"blood_after":4900,"shock_before":50,"shock_after":0,"health_before":990,"health_after":1000}},{"at":15,"event":{"type":"time_advanced","from":10,"to":15,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"29e9214b52e8e3e4"})
        );
        assert_eq!(
            tick.casualty_state(patient),
            Some(CasualtyState {
                blood: 4900,
                shock: 0,
                shock_remainder: 0,
                incapacitated: false,
                recovering: true,
                recovery_next_at: Some(20),
                materialized_at: 15
            })
        );
        assert_eq!(tick.soldier(patient).unwrap().health, 1000);
        assert_eq!(
            tick.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let tick_fixture = healing_clock15_fixture();
        let tick = verified_restore(&tick, &tick_fixture);

        let (response, healed) = exchange(
            req(r#"{"version":1,"command":"advance_to","target":20}"#),
            false,
            tick,
        );
        assert_eq!(status(&response), "HTTP/1.1 200 OK");
        assert_eq!(
            json_body(&response),
            json!({"version":1,"clock":20,"events":[{"at":20,"event":{"type":"recovery_ticked","id":1,"blood_before":4900,"blood_after":5000,"shock_before":0,"shock_after":0,"health_before":1000,"health_after":1000}},{"at":20,"event":{"type":"wound_healed","id":0,"patient":1}},{"at":20,"event":{"type":"recovery_changed","id":1,"before":true,"after":false,"next_at":null}},{"at":20,"event":{"type":"time_advanced","from":15,"to":20,"hot_cells_stepped":0,"fixed_steps_per_hot_cell":0}}],"terminal_error":null,"blocked":null,"digest":"bce6a737b43e2693"})
        );
        assert_eq!(
            healed.casualty_state(patient),
            Some(CasualtyState {
                blood: BLOOD_MAX,
                shock: 0,
                shock_remainder: 0,
                incapacitated: false,
                recovering: false,
                recovery_next_at: None,
                materialized_at: 20
            })
        );
        assert_eq!(
            healed.wound(WoundId(0)).unwrap(),
            Wound {
                id: WoundId(0),
                patient,
                created_at: 0,
                spec: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30
                },
                controlled: true,
                healed: true
            }
        );
        assert_eq!(
            healed.treatment(TreatmentId(0)).unwrap().status,
            TreatmentStatus::Completed { at: 10 }
        );
        assert_eq!(healed.soldier(patient).unwrap().health, 1000);
        assert_eq!(
            healed.resource_totals(),
            ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 2,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 3,
                consumed_medical: 1,
                lost_medical: 0,
            }
        );
        let healed_fixture = healing_terminal_fixture();
        let healed = verified_restore(&healed, &healed_fixture);
        let mut final_release_probe = verified_restore(&healed, &healed_fixture);
        assert_eq!(
            final_release_probe.apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma: 1,
                    bleeding_per_second: 1,
                    shock: 1
                }
            }),
            sim_core::ApplyOutcome {
                clock: 20,
                events: vec![TimedEvent {
                    at: 20,
                    event: Event::WoundInflicted {
                        id: WoundId(1),
                        patient,
                        wound: WoundSpec {
                            trauma: 1,
                            bleeding_per_second: 1,
                            shock: 1
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(
            final_release_probe.apply(Command::StartTreatment {
                medic,
                patient,
                wound: Some(WoundId(1)),
                kind: TreatmentKind::Hemostatic
            }),
            sim_core::ApplyOutcome {
                clock: 20,
                events: vec![TimedEvent {
                    at: 20,
                    event: Event::TreatmentStarted {
                        id: TreatmentId(1),
                        medic,
                        patient,
                        wound: Some(WoundId(1)),
                        kind: TreatmentKind::Hemostatic,
                        completes_at: 30,
                        consumed: 1
                    }
                }],
                error: None,
                blocked: None
            }
        );
        assert_eq!(final_release_probe.wound(WoundId(2)), None);
        assert_eq!(final_release_probe.treatment(TreatmentId(2)), None);
        verified_restore(&final_release_probe, &healing_final_probe_fixture());

        let mut terminal_probe = verified_restore(&healed, &healed_fixture);
        assert_eq!(
            terminal_probe.apply(Command::AdvanceTo { target: 21 }),
            sim_core::ApplyOutcome {
                clock: 21,
                events: vec![TimedEvent {
                    at: 21,
                    event: Event::TimeAdvanced {
                        from: 20,
                        to: 21,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0,
                    },
                }],
                error: None,
                blocked: None,
            }
        );
        verified_restore(&terminal_probe, &healing_clock21_fixture());
    }

    #[test]
    fn gate_c2_http_malformed_matrix_is_byte_and_digest_atomic() {
        #[derive(Clone, Copy)]
        struct Shape {
            name: &'static str,
            base: &'static str,
            allowed: &'static [&'static str],
            required: &'static [&'static str],
            numeric: &'static [&'static str],
        }
        const SHAPES: &[Shape] = &[
            Shape {
                name: "inflict_wound",
                base: r#"{"version":1,"command":"inflict_wound","patient":7,"trauma":10,"bleeding_per_second":20,"shock":30}"#,
                allowed: &[
                    "version",
                    "command",
                    "patient",
                    "trauma",
                    "bleeding_per_second",
                    "shock",
                ],
                required: &["patient", "trauma", "bleeding_per_second", "shock"],
                numeric: &["patient", "trauma", "bleeding_per_second", "shock"],
            },
            Shape {
                name: "start_hemostatic",
                base: r#"{"version":1,"command":"start_treatment","medic":2,"patient":7,"wound":4,"kind":"hemostatic"}"#,
                allowed: &["version", "command", "medic", "patient", "wound", "kind"],
                required: &["medic", "patient", "wound", "kind"],
                numeric: &["medic", "patient", "wound"],
            },
            Shape {
                name: "start_shock",
                base: r#"{"version":1,"command":"start_treatment","medic":2,"patient":7,"kind":"shock"}"#,
                allowed: &["version", "command", "medic", "patient", "kind"],
                required: &["medic", "patient", "kind"],
                numeric: &["medic", "patient"],
            },
            Shape {
                name: "request_hemostatic",
                base: r#"{"version":1,"command":"request_treatment","patient":7,"wound":4,"kind":"hemostatic"}"#,
                allowed: &["version", "command", "patient", "wound", "kind"],
                required: &["patient", "wound", "kind"],
                numeric: &["patient", "wound"],
            },
            Shape {
                name: "request_shock",
                base: r#"{"version":1,"command":"request_treatment","patient":7,"kind":"shock"}"#,
                allowed: &["version", "command", "patient", "kind"],
                required: &["patient", "kind"],
                numeric: &["patient"],
            },
            Shape {
                name: "interrupt",
                base: r#"{"version":1,"command":"interrupt_treatment","treatment":4}"#,
                allowed: &["version", "command", "treatment"],
                required: &["treatment"],
                numeric: &["treatment"],
            },
        ];
        const FIELDS: &[&str] = &[
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
        fn historical(field: &str) -> Value {
            match field {
                "command" => json!("advance_to"),
                "hot" => json!(true),
                "activity" => json!("idle"),
                "kind" => json!("shock"),
                _ => json!(1),
            }
        }
        fn changed(base: &str, field: &str, value: Option<Value>) -> String {
            let mut object = serde_json::from_str::<Value>(base)
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
            match value {
                Some(value) => {
                    object.insert(field.to_owned(), value);
                }
                None => {
                    object.remove(field);
                }
            }
            Value::Object(object).to_string()
        }
        fn extra(base: &str, field: &str, value: Value) -> String {
            changed(base, field, Some(value))
        }
        fn changed_raw_number(base: &str, field: &str, number: &str) -> String {
            let ordinary = changed(base, field, Some(json!(1)));
            ordinary.replacen(
                &format!(r#""{field}":1"#),
                &format!(r#""{field}":{number}"#),
                1,
            )
        }
        fn duplicate(base: &str, field: &str) -> String {
            let value = serde_json::from_str::<Value>(base).unwrap()[field].clone();
            format!(r#"{{"{field}":{},{}"#, value, &base[1..])
        }

        let mut rows: Vec<(String, String)> = Vec::new();
        let mut required_rows = Vec::new();
        let mut inapplicable_rows = Vec::new();
        let mut wrong_type_rows = Vec::new();
        let mut overflow_id_rows = Vec::new();
        let mut duplicate_rows = Vec::new();

        assert_eq!(SHAPES.iter().map(|s| s.required.len()).sum::<usize>(), 17);
        assert_eq!(SHAPES.iter().map(|s| s.numeric.len()).sum::<usize>(), 13);
        for shape in SHAPES {
            for field in shape.required {
                required_rows.push((
                    format!("required/{}/{field}/missing", shape.name),
                    changed(shape.base, field, None),
                ));
                required_rows.push((
                    format!("required/{}/{field}/null", shape.name),
                    changed(shape.base, field, Some(Value::Null)),
                ));
            }
            let inapplicable: Vec<_> = FIELDS
                .iter()
                .filter(|field| !shape.allowed.contains(field))
                .collect();
            let expected = match shape.name {
                "inflict_wound" | "start_hemostatic" => 14,
                "start_shock" | "request_hemostatic" => 15,
                "request_shock" => 16,
                "interrupt" => 17,
                _ => unreachable!(),
            };
            assert_eq!(inapplicable.len(), expected, "{}", shape.name);
            for field in inapplicable {
                inapplicable_rows.push((
                    format!("inapplicable/{}/{field}/value", shape.name),
                    extra(shape.base, field, historical(field)),
                ));
                inapplicable_rows.push((
                    format!("inapplicable/{}/{field}/null", shape.name),
                    extra(shape.base, field, Value::Null),
                ));
            }
            rows.push((
                format!("unknown-field/{}", shape.name),
                extra(shape.base, "truly_unknown", json!(1)),
            ));
            for field in shape.numeric {
                for (form, value) in [
                    ("negative", json!(-1)),
                    ("fractional", json!(1.5)),
                    ("string", json!("1")),
                    ("boolean", json!(true)),
                    ("array", json!([1])),
                    ("object", json!({"v":1})),
                ] {
                    wrong_type_rows.push((
                        format!("numeric/{}/{field}/{form}", shape.name),
                        changed(shape.base, field, Some(value)),
                    ));
                }
                if matches!(*field, "patient" | "medic" | "wound" | "treatment") {
                    overflow_id_rows.push((
                        format!("u64-overflow/{}/{field}", shape.name),
                        changed_raw_number(shape.base, field, "18446744073709551616"),
                    ));
                }
            }
            for field in shape.allowed {
                duplicate_rows.push((
                    format!("duplicate/{}/{field}", shape.name),
                    duplicate(shape.base, field),
                ));
            }
        }
        assert_eq!(required_rows.len(), 34);
        assert_eq!(inapplicable_rows.len(), 182);
        assert_eq!(wrong_type_rows.len(), 78);
        assert_eq!(overflow_id_rows.len(), 10);
        assert_eq!(duplicate_rows.len(), 29);
        rows.extend(required_rows);
        rows.extend(inapplicable_rows);
        rows.extend(wrong_type_rows);
        rows.extend(overflow_id_rows);
        rows.extend(duplicate_rows);

        for shape in &SHAPES[1..5] {
            for (form, value) in [
                ("unknown", json!("bandage")),
                ("case", json!("Shock")),
                ("number", json!(1)),
                ("boolean", json!(true)),
                ("array", json!([])),
                ("object", json!({})),
            ] {
                rows.push((
                    format!("kind/{}/{form}", shape.name),
                    changed(shape.base, "kind", Some(value)),
                ));
            }
        }
        for (form, value) in [
            ("missing", None),
            ("null", Some(Value::Null)),
            ("wrong-string", Some(json!("1"))),
            ("wrong-bool", Some(json!(true))),
            ("wrong-array", Some(json!([]))),
            ("wrong-object", Some(json!({}))),
            ("unsupported", Some(json!(2))),
            ("zero", Some(json!(0))),
            ("overflow", Some(json!(2))),
        ] {
            rows.push((
                format!("version/{form}"),
                if form == "overflow" {
                    changed_raw_number(SHAPES[5].base, "version", "18446744073709551616")
                } else {
                    changed(SHAPES[5].base, "version", value)
                },
            ));
        }
        for (form, value) in [
            ("missing", None),
            ("null", Some(Value::Null)),
            ("numeric", Some(json!(1))),
            ("boolean", Some(json!(true))),
            ("array", Some(json!([]))),
            ("object", Some(json!({}))),
            ("unknown", Some(json!("operate"))),
            ("case", Some(json!("Interrupt_Treatment"))),
        ] {
            rows.push((
                format!("command/{form}"),
                changed(SHAPES[5].base, "command", value),
            ));
        }
        for field in ["trauma", "bleeding_per_second", "shock"] {
            rows.push((
                format!("u16-overflow/{field}"),
                changed(SHAPES[0].base, field, Some(json!(65536))),
            ));
        }
        rows.extend([
            ("json/empty".into(), "".into()),
            ("json/null".into(), "null".into()),
            ("json/array".into(), "[]".into()),
            ("json/string".into(), r#""x""#.into()),
            (
                "json/truncated".into(),
                r#"{"version":1,"command":"interrupt_treatment","treatment":4"#.into(),
            ),
            (
                "json/malformed".into(),
                r#"{"version":1,,"command":"interrupt_treatment"}"#.into(),
            ),
            (
                "json/trailing".into(),
                format!("{} trailing", SHAPES[5].base),
            ),
        ]);

        let names: std::collections::BTreeSet<_> = rows.iter().map(|(name, _)| name).collect();
        assert_eq!(names.len(), rows.len(), "matrix row names must be unique");
        for (name, body) in rows {
            let source = pinned_http_fixture(77);
            let source_bytes = source.snapshot();
            assert_pinned_http_fixture(&source);
            let restored = World::from_snapshot(&source_bytes).unwrap();
            assert_pinned_http_fixture(&restored);
            assert_eq!(
                restored.snapshot(),
                source_bytes,
                "pre-request restore bytes: {name}"
            );
            assert_eq!(
                restored.state_digest(),
                0x2bd7_8a3a_f7ee_2a90,
                "pre-request digest: {name}"
            );
            let (response, rejected) = exchange(req(&body), false, restored);
            assert_eq!(status(&response), "HTTP/1.1 400 Bad Request", "{name}");
            assert_eq!(
                raw_body(&response),
                br#"{"error":"malformed_or_unsupported"}"#,
                "{name}"
            );
            assert_eq!(rejected.clock(), 0, "{name}");
            assert_pinned_http_fixture(&rejected);
            assert_eq!(rejected.snapshot(), source_bytes, "rejection bytes: {name}");
            assert_eq!(
                rejected.state_digest(),
                0x2bd7_8a3a_f7ee_2a90,
                "rejection digest: {name}"
            );
            let twice = World::from_snapshot(&rejected.snapshot()).unwrap();
            assert_pinned_http_fixture(&twice);
            assert_eq!(
                twice.snapshot(),
                source_bytes,
                "second restore bytes: {name}"
            );
            assert_eq!(
                twice.state_digest(),
                0x2bd7_8a3a_f7ee_2a90,
                "second restore digest: {name}"
            );
        }
    }

    fn f5_living_source() -> World {
        let mut world = World::new(19);
        assert_eq!(
            world.apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0
                    },
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierSpawned {
                        id: EntityId::from_parts(0, 0),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        },
                    },
                }],
                error: None,
                blocked: None,
            }
        );
        world
    }

    fn f5_stale_source() -> World {
        let mut world = World::new(23);
        assert_eq!(
            world.apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0
                    },
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierSpawned {
                        id: EntityId::from_parts(0, 0),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        }
                    }
                }],
                error: None,
                blocked: None,
            }
        );
        assert_eq!(
            world.apply(Command::DespawnSoldier {
                id: EntityId::from_parts(0, 0)
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierRemoved {
                        id: EntityId::from_parts(0, 0),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        }
                    }
                }],
                error: None,
                blocked: None,
            }
        );
        assert_eq!(
            world.apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0
                    },
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierSpawned {
                        id: EntityId::from_parts(0, 1),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        }
                    }
                }],
                error: None,
                blocked: None,
            }
        );
        world
    }

    fn f5_dead_source() -> World {
        let mut world = World::new(19);
        assert_eq!(
            world.apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0
                    },
                    squad: None,
                    role: Role::Rifle,
                    rank: 0,
                    health: 1000,
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical: 0
                    },
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierSpawned {
                        id: EntityId::from_parts(0, 0),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        }
                    }
                }],
                error: None,
                blocked: None,
            }
        );
        assert_eq!(
            world.apply(Command::InflictWound {
                patient: EntityId::from_parts(0, 0),
                wound: WoundSpec {
                    trauma: 1000,
                    bleeding_per_second: 0,
                    shock: 0
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![
                    TimedEvent {
                        at: 0,
                        event: Event::WoundInflicted {
                            id: WoundId(0),
                            patient: EntityId::from_parts(0, 0),
                            wound: WoundSpec {
                                trauma: 1000,
                                bleeding_per_second: 0,
                                shock: 0
                            }
                        }
                    },
                    TimedEvent {
                        at: 0,
                        event: Event::SoldierDied {
                            id: EntityId::from_parts(0, 0),
                            cause: DeathCause::ImmediateTrauma,
                            health_before: 1000
                        }
                    },
                ],
                error: None,
                blocked: None,
            }
        );
        world
    }

    fn f5_living_pre_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 living source/pre",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(1, 0)],
            wounds: vec![],
            wounds_of: vec![(EntityId::from_parts(0, 0), vec![])],
            absent_wounds: vec![WoundId(0), WoundId(1)],
            casualties: vec![
                (EntityId::from_parts(0, 0), None),
                (EntityId::from_parts(1, 0), None),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 9],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0x141a_fc1e_bd5c_94da,
        }
    }

    fn f5_stale_pre_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 stale-generation source/pre",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 1),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(0, 0), EntityId::from_parts(1, 0)],
            wounds: vec![],
            wounds_of: vec![(EntityId::from_parts(0, 1), vec![])],
            absent_wounds: vec![WoundId(0), WoundId(1)],
            casualties: vec![
                (EntityId::from_parts(0, 1), None),
                (EntityId::from_parts(0, 0), None),
            ],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0xd662_1b34_87b2_ff72,
        }
    }

    fn f5_dead_pre_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 retained-dead source/pre",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 0,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 0,
                    activity: Activity::Idle,
                    life: LifeState::Dead {
                        at: 0,
                        cause: DeathCause::ImmediateTrauma,
                    },
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(1, 0)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient: EntityId::from_parts(0, 0),
                created_at: 0,
                spec: WoundSpec {
                    trauma: 1000,
                    bleeding_per_second: 0,
                    shock: 0,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![(
                EntityId::from_parts(0, 0),
                vec![Wound {
                    id: WoundId(0),
                    patient: EntityId::from_parts(0, 0),
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 1000,
                        bleeding_per_second: 0,
                        shock: 0,
                    },
                    controlled: false,
                    healed: false,
                }],
            )],
            absent_wounds: vec![WoundId(1)],
            casualties: vec![(
                EntityId::from_parts(0, 0),
                Some(CasualtyState {
                    blood: 5000,
                    shock: 0,
                    shock_remainder: 0,
                    incapacitated: false,
                    recovering: false,
                    recovery_next_at: None,
                    materialized_at: 0,
                }),
            )],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0x11d1_c6a4_424f_e908,
        }
    }

    fn f5_trauma_1000_post_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 trauma-1000 post",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 0,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 0,
                    activity: Activity::Idle,
                    life: LifeState::Dead {
                        at: 0,
                        cause: DeathCause::ImmediateTrauma,
                    },
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(1, 0)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient: EntityId::from_parts(0, 0),
                created_at: 0,
                spec: WoundSpec {
                    trauma: 1000,
                    bleeding_per_second: 0,
                    shock: 0,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![(
                EntityId::from_parts(0, 0),
                vec![Wound {
                    id: WoundId(0),
                    patient: EntityId::from_parts(0, 0),
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 1000,
                        bleeding_per_second: 0,
                        shock: 0,
                    },
                    controlled: false,
                    healed: false,
                }],
            )],
            absent_wounds: vec![WoundId(1)],
            casualties: vec![(
                EntityId::from_parts(0, 0),
                Some(CasualtyState {
                    blood: 5000,
                    shock: 0,
                    shock_remainder: 0,
                    incapacitated: false,
                    recovering: false,
                    recovery_next_at: None,
                    materialized_at: 0,
                }),
            )],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0x11d1_c6a4_424f_e908,
        }
    }

    fn f5_bleeding_1000_post_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 bleeding-1000 post",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(1, 0)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient: EntityId::from_parts(0, 0),
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 1000,
                    shock: 0,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![(
                EntityId::from_parts(0, 0),
                vec![Wound {
                    id: WoundId(0),
                    patient: EntityId::from_parts(0, 0),
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 1000,
                        shock: 0,
                    },
                    controlled: false,
                    healed: false,
                }],
            )],
            absent_wounds: vec![WoundId(1)],
            casualties: vec![(
                EntityId::from_parts(0, 0),
                Some(CasualtyState {
                    blood: 5000,
                    shock: 0,
                    shock_remainder: 0,
                    incapacitated: false,
                    recovering: false,
                    recovery_next_at: None,
                    materialized_at: 0,
                }),
            )],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0x807d_822b_5d03_55d4,
        }
    }

    fn f5_shock_1000_post_fixture() -> PublicMedicalFixture {
        PublicMedicalFixture {
            name: "f5 shock-1000 post",
            clock: 0,
            soldier_count: 1,
            soldiers: vec![Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 0,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 0,
                    activity: Activity::Idle,
                    life: LifeState::Dead {
                        at: 0,
                        cause: DeathCause::TraumaticShock,
                    },
                    materialized_at: 0,
                },
            }],
            absent_soldiers: vec![EntityId::from_parts(1, 0)],
            wounds: vec![Wound {
                id: WoundId(0),
                patient: EntityId::from_parts(0, 0),
                created_at: 0,
                spec: WoundSpec {
                    trauma: 0,
                    bleeding_per_second: 0,
                    shock: 1000,
                },
                controlled: false,
                healed: false,
            }],
            wounds_of: vec![(
                EntityId::from_parts(0, 0),
                vec![Wound {
                    id: WoundId(0),
                    patient: EntityId::from_parts(0, 0),
                    created_at: 0,
                    spec: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 1000,
                    },
                    controlled: false,
                    healed: false,
                }],
            )],
            absent_wounds: vec![WoundId(1)],
            casualties: vec![(
                EntityId::from_parts(0, 0),
                Some(CasualtyState {
                    blood: 5000,
                    shock: 1000,
                    shock_remainder: 0,
                    incapacitated: true,
                    recovering: false,
                    recovery_next_at: None,
                    materialized_at: 0,
                }),
            )],
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 0,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 0,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0],
            absent_squads: vec![0],
            absent_hot_cells: vec![0],
            hot_cell_count: 0,
            digest: 0x184d_2863_764e_d3a8,
        }
    }

    struct F5Row {
        name: &'static str,
        source: fn() -> World,
        pre: fn() -> PublicMedicalFixture,
        request: &'static str,
        expected_envelope: &'static str,
        post: fn() -> PublicMedicalFixture,
        rejection: bool,
    }

    const F5_REJECTION_ROWS: [F5Row; 8] = [
        F5Row {
            name: "zero_effect",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_wound","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "trauma_1001",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1001,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_wound","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "bleeding_1001",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":1001,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_wound","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "shock_1001",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":0,"shock":1001}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_wound","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "absent_patient",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":1,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_entity","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "stale_generation_patient",
            source: f5_stale_source,
            pre: f5_stale_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_entity","blocked":null,"digest":"d6621b3487b2ff72"}"#,
            post: f5_stale_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "dead_patient",
            source: f5_dead_source,
            pre: f5_dead_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"dead_entity","blocked":null,"digest":"11d1c6a4424fe908"}"#,
            post: f5_dead_pre_fixture,
            rejection: true,
        },
        F5Row {
            name: "wire_valid_max_patient",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":18446744073709551615,"trauma":1,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[],"terminal_error":"invalid_entity","blocked":null,"digest":"141afc1ebd5c94da"}"#,
            post: f5_living_pre_fixture,
            rejection: true,
        },
    ];

    const F5_ACCEPTED_ROWS: [F5Row; 3] = [
        F5Row {
            name: "trauma_1000",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":1000,"bleeding_per_second":0,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[{"at":0,"event":{"type":"wound_inflicted","id":0,"patient":0,"trauma":1000,"bleeding_per_second":0,"shock":0}},{"at":0,"event":{"type":"soldier_died","id":0,"cause":"immediate_trauma","health_before":1000}}],"terminal_error":null,"blocked":null,"digest":"11d1c6a4424fe908"}"#,
            post: f5_trauma_1000_post_fixture,
            rejection: false,
        },
        F5Row {
            name: "bleeding_1000",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":1000,"shock":0}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[{"at":0,"event":{"type":"wound_inflicted","id":0,"patient":0,"trauma":0,"bleeding_per_second":1000,"shock":0}}],"terminal_error":null,"blocked":null,"digest":"807d822b5d0355d4"}"#,
            post: f5_bleeding_1000_post_fixture,
            rejection: false,
        },
        F5Row {
            name: "shock_1000",
            source: f5_living_source,
            pre: f5_living_pre_fixture,
            request: r#"{"version":1,"command":"inflict_wound","patient":0,"trauma":0,"bleeding_per_second":0,"shock":1000}"#,
            expected_envelope: r#"{"version":1,"clock":0,"events":[{"at":0,"event":{"type":"wound_inflicted","id":0,"patient":0,"trauma":0,"bleeding_per_second":0,"shock":1000}},{"at":0,"event":{"type":"soldier_died","id":0,"cause":"traumatic_shock","health_before":1000}}],"terminal_error":null,"blocked":null,"digest":"184d2863764ed3a8"}"#,
            post: f5_shock_1000_post_fixture,
            rejection: false,
        },
    ];

    fn f5_execute_row(row: &F5Row) {
        let source = (row.source)();
        let pre = (row.pre)();
        pre.assert_world(&source);
        let pre_bytes = source.snapshot();
        let restored = verified_restore(&source, &pre);
        let (response, result) = exchange(req(row.request), false, restored);
        assert_eq!(status(&response), "HTTP/1.1 200 OK", "{}", row.name);
        let expected: Value = serde_json::from_str(row.expected_envelope).unwrap();
        assert_eq!(json_body(&response), expected, "{}", row.name);
        let post = (row.post)();
        post.assert_world(&result);
        let post_bytes = result.snapshot();
        let restored_post = verified_restore(&result, &post);
        assert_eq!(
            restored_post.snapshot(),
            post_bytes,
            "{} post bytes",
            row.name
        );
        if row.rejection {
            assert_eq!(post.clock, pre.clock, "{} literal clock", row.name);
            assert_eq!(post.digest, pre.digest, "{} literal digest", row.name);
            assert_eq!(post_bytes, pre_bytes, "{} rejection bytes", row.name);
        }
    }

    #[test]
    fn gate_c2_f5_literal_rows_execute_and_count_authority() {
        assert_eq!(F5_REJECTION_ROWS.len(), 8);
        assert_eq!(F5_ACCEPTED_ROWS.len(), 3);
        let mut names = std::collections::BTreeSet::new();
        for row in F5_REJECTION_ROWS.iter().chain(F5_ACCEPTED_ROWS.iter()) {
            assert!(
                names.insert(row.name),
                "duplicate executed F5 row {}",
                row.name
            );
            f5_execute_row(row);
        }
        assert_eq!(names.len(), 11);
    }

    fn f6_spawn(world: &mut World, id: EntityId, role: Role, medical: u32) {
        assert_eq!(
            world.apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: 0,
                    position: Position {
                        x_mm: 0,
                        y_mm: 0,
                        cell: 0
                    },
                    squad: None,
                    role,
                    rank: 0,
                    health: 1000,
                    ammunition: 0,
                    inventory: Inventory {
                        food: 0,
                        water: 0,
                        medical
                    },
                },
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierSpawned {
                        id,
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical
                        }
                    }
                }],
                error: None,
                blocked: None,
            }
        );
    }

    fn f6_absent_source() -> World {
        let mut world = World::new(29);
        f6_spawn(&mut world, EntityId::from_parts(0, 0), Role::Medic, 3);
        f6_spawn(&mut world, EntityId::from_parts(1, 0), Role::Rifle, 0);
        assert_eq!(
            world.apply(Command::InflictWound {
                patient: EntityId::from_parts(1, 0),
                wound: WoundSpec {
                    trauma: 10,
                    bleeding_per_second: 20,
                    shock: 30
                }
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(0),
                        patient: EntityId::from_parts(1, 0),
                        wound: WoundSpec {
                            trauma: 10,
                            bleeding_per_second: 20,
                            shock: 30
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        world
    }

    fn f6_controlled_source() -> World {
        let mut world = f6_absent_source();
        assert_eq!(
            world.apply(Command::StartTreatment {
                medic: EntityId::from_parts(0, 0),
                patient: EntityId::from_parts(1, 0),
                wound: Some(WoundId(0)),
                kind: TreatmentKind::Hemostatic
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::TreatmentStarted {
                        id: TreatmentId(0),
                        medic: EntityId::from_parts(0, 0),
                        patient: EntityId::from_parts(1, 0),
                        wound: Some(WoundId(0)),
                        kind: TreatmentKind::Hemostatic,
                        completes_at: 10,
                        consumed: 1
                    }
                }],
                error: None,
                blocked: None
            }
        );
        let out = world.apply(Command::AdvanceTo { target: 10 });
        assert_eq!(out.error, None);
        assert_eq!(out.blocked, None);
        world
    }

    fn f6_healed_source() -> World {
        let mut world = f6_controlled_source();
        let out = world.apply(Command::AdvanceTo { target: 20 });
        assert_eq!(out.error, None);
        assert_eq!(out.blocked, None);
        world
    }

    fn f6_shock_no_casualty_source() -> World {
        let mut world = World::new(29);
        f6_spawn(&mut world, EntityId::from_parts(0, 0), Role::Medic, 3);
        f6_spawn(&mut world, EntityId::from_parts(1, 0), Role::Rifle, 0);
        world
    }

    fn f6_zero_shock_source() -> World {
        let mut world = World::new(61);
        f6_spawn(&mut world, EntityId::from_parts(0, 0), Role::Medic, 5);
        f6_spawn(&mut world, EntityId::from_parts(1, 0), Role::Rifle, 0);
        assert_eq!(
            world
                .apply(Command::InflictWound {
                    patient: EntityId::from_parts(1, 0),
                    wound: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 20,
                        shock: 0
                    }
                })
                .error,
            None
        );
        world
    }

    fn f6_other_patient_source() -> World {
        let mut world = World::new(62);
        f6_spawn(&mut world, EntityId::from_parts(0, 0), Role::Medic, 5);
        f6_spawn(&mut world, EntityId::from_parts(1, 0), Role::Rifle, 0);
        f6_spawn(&mut world, EntityId::from_parts(2, 0), Role::Rifle, 0);
        assert_eq!(
            world
                .apply(Command::InflictWound {
                    patient: EntityId::from_parts(2, 0),
                    wound: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 20,
                        shock: 0
                    }
                })
                .error,
            None
        );
        world
    }

    fn f6_removed_owner_source() -> World {
        let mut world = World::new(63);
        f6_spawn(&mut world, EntityId::from_parts(0, 0), Role::Medic, 5);
        f6_spawn(&mut world, EntityId::from_parts(1, 0), Role::Rifle, 0);
        f6_spawn(&mut world, EntityId::from_parts(2, 0), Role::Rifle, 0);
        assert_eq!(
            world
                .apply(Command::InflictWound {
                    patient: EntityId::from_parts(2, 0),
                    wound: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 20,
                        shock: 0
                    }
                })
                .error,
            None
        );
        assert_eq!(
            world.apply(Command::DespawnSoldier {
                id: EntityId::from_parts(2, 0)
            }),
            ApplyOutcome {
                clock: 0,
                events: vec![TimedEvent {
                    at: 0,
                    event: Event::SoldierRemoved {
                        id: EntityId::from_parts(2, 0),
                        loadout: sim_core::Loadout {
                            ammunition: 0,
                            food: 0,
                            water: 0,
                            medical: 0
                        }
                    }
                }],
                error: None,
                blocked: None
            }
        );
        world
    }

    fn f6_absent_fixture() -> PublicMedicalFixture {
        healing_post_wound_fixture()
    }
    fn f6_controlled_fixture() -> PublicMedicalFixture {
        healing_completed_fixture()
    }
    fn f6_healed_fixture() -> PublicMedicalFixture {
        healing_terminal_fixture()
    }
    fn f6_shock_no_casualty_fixture() -> PublicMedicalFixture {
        healing_setup_fixture()
    }

    fn f6_simple_fixture(mode: u8) -> PublicMedicalFixture {
        let other = mode == 1;
        let removed = mode == 2;
        let seed_digest = match mode {
            0 => 0x85ba_4f91_4644_5383,
            1 => 0x669c_9532_1422_5f15,
            _ => 0xb9e2_60b0_dfd9_fdef,
        };
        let mut soldiers = vec![
            Soldier {
                id: EntityId::from_parts(0, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Medic,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 5,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            },
            Soldier {
                id: EntityId::from_parts(1, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            },
        ];
        if other {
            soldiers.push(Soldier {
                id: EntityId::from_parts(2, 0),
                faction: 0,
                position: Position {
                    x_mm: 0,
                    y_mm: 0,
                    cell: 0,
                },
                squad: None,
                role: Role::Rifle,
                rank: 0,
                health: 1000,
                needs: Needs {
                    fatigue: 0,
                    hunger: 0,
                    thirst: 0,
                    sleep_debt: 0,
                },
                ammunition: 0,
                inventory: Inventory {
                    food: 0,
                    water: 0,
                    medical: 0,
                },
                living: LivingState {
                    hunger: 0,
                    thirst: 0,
                    fatigue: 0,
                    sleep_debt: 0,
                    morale: 1000,
                    health: 1000,
                    activity: Activity::Idle,
                    life: LifeState::Alive,
                    materialized_at: 0,
                },
            });
        }
        let wound = Wound {
            id: WoundId(0),
            patient: EntityId::from_parts(if other { 2 } else { 1 }, 0),
            created_at: 0,
            spec: WoundSpec {
                trauma: 0,
                bleeding_per_second: 20,
                shock: 0,
            },
            controlled: false,
            healed: false,
        };
        PublicMedicalFixture {
            name: "f6 simple",
            clock: 0,
            soldier_count: if other { 3 } else { 2 },
            soldiers,
            absent_soldiers: vec![EntityId::from_parts(if removed { 2 } else { 3 }, 0)],
            wounds: if removed { vec![] } else { vec![wound] },
            wounds_of: if removed {
                vec![
                    (EntityId::from_parts(0, 0), vec![]),
                    (EntityId::from_parts(1, 0), vec![]),
                ]
            } else if other {
                vec![
                    (EntityId::from_parts(0, 0), vec![]),
                    (EntityId::from_parts(1, 0), vec![]),
                    (EntityId::from_parts(2, 0), vec![wound]),
                ]
            } else {
                vec![
                    (EntityId::from_parts(0, 0), vec![]),
                    (EntityId::from_parts(1, 0), vec![wound]),
                ]
            },
            absent_wounds: if removed {
                vec![WoundId(0), WoundId(1), WoundId(u64::MAX)]
            } else {
                vec![WoundId(1), WoundId(u64::MAX)]
            },
            casualties: if removed {
                vec![
                    (EntityId::from_parts(0, 0), None),
                    (EntityId::from_parts(1, 0), None),
                    (EntityId::from_parts(2, 0), None),
                ]
            } else if other {
                vec![
                    (EntityId::from_parts(0, 0), None),
                    (EntityId::from_parts(1, 0), None),
                    (
                        EntityId::from_parts(2, 0),
                        Some(CasualtyState {
                            blood: 5000,
                            shock: 0,
                            shock_remainder: 0,
                            incapacitated: false,
                            recovering: false,
                            recovery_next_at: None,
                            materialized_at: 0,
                        }),
                    ),
                ]
            } else {
                vec![
                    (EntityId::from_parts(0, 0), None),
                    (
                        EntityId::from_parts(1, 0),
                        Some(CasualtyState {
                            blood: 5000,
                            shock: 0,
                            shock_remainder: 0,
                            incapacitated: false,
                            recovering: false,
                            recovery_next_at: None,
                            materialized_at: 0,
                        }),
                    ),
                ]
            },
            treatments: vec![],
            absent_treatments: vec![TreatmentId(0), TreatmentId(1)],
            totals: ResourceTotals {
                ammunition: 0,
                stockpile_supplies: 0,
                carried_food: 0,
                carried_water: 0,
                carried_medical: 5,
                sourced_food: 0,
                sourced_water: 0,
                consumed_food: 0,
                consumed_water: 0,
                lost_food: 0,
                lost_water: 0,
                sourced_medical: 5,
                consumed_medical: 0,
                lost_medical: 0,
            },
            absent_stockpiles: vec![0, 7],
            absent_squads: vec![0, 7],
            absent_hot_cells: vec![0, 7],
            hot_cell_count: 0,
            digest: seed_digest,
        }
    }
    fn f6_zero_shock_fixture() -> PublicMedicalFixture {
        f6_simple_fixture(0)
    }
    fn f6_other_patient_fixture() -> PublicMedicalFixture {
        f6_simple_fixture(1)
    }
    fn f6_removed_owner_fixture() -> PublicMedicalFixture {
        f6_simple_fixture(2)
    }

    struct F6Row {
        name: &'static str,
        source: fn() -> World,
        fixture: fn() -> PublicMedicalFixture,
        request: &'static str,
        expected: &'static str,
    }
    const F6_ROWS: [F6Row; 16] = [
        F6Row {
            name: "direct_hemostatic_absent_wound",
            source: f6_absent_source,
            fixture: f6_absent_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":1,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"e409bfccd97b10f5","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_absent_wound",
            source: f6_absent_source,
            fixture: f6_absent_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":1,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"e409bfccd97b10f5","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_hemostatic_wire_valid_max_wound",
            source: f6_absent_source,
            fixture: f6_absent_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":18446744073709551615,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"e409bfccd97b10f5","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_wire_valid_max_wound",
            source: f6_absent_source,
            fixture: f6_absent_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":18446744073709551615,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"e409bfccd97b10f5","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_hemostatic_other_patient_wound",
            source: f6_other_patient_source,
            fixture: f6_other_patient_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"669c953214225f15","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_other_patient_wound",
            source: f6_other_patient_source,
            fixture: f6_other_patient_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"669c953214225f15","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_hemostatic_controlled_wound",
            source: f6_controlled_source,
            fixture: f6_controlled_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":10,"digest":"f10dc636c78be1da","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_controlled_wound",
            source: f6_controlled_source,
            fixture: f6_controlled_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":10,"digest":"f10dc636c78be1da","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_hemostatic_healed_wound",
            source: f6_healed_source,
            fixture: f6_healed_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":20,"digest":"bce6a737b43e2693","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_healed_wound",
            source: f6_healed_source,
            fixture: f6_healed_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":20,"digest":"bce6a737b43e2693","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_hemostatic_removed_owner_wound",
            source: f6_removed_owner_source,
            fixture: f6_removed_owner_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"b9e260b0dfd9fdef","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_hemostatic_removed_owner_wound",
            source: f6_removed_owner_source,
            fixture: f6_removed_owner_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"wound":0,"kind":"hemostatic"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"b9e260b0dfd9fdef","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_shock_no_casualty",
            source: f6_shock_no_casualty_source,
            fixture: f6_shock_no_casualty_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"kind":"shock"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"a531059e3401271d","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_shock_no_casualty",
            source: f6_shock_no_casualty_source,
            fixture: f6_shock_no_casualty_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"kind":"shock"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"a531059e3401271d","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "direct_shock_zero_shock_casualty",
            source: f6_zero_shock_source,
            fixture: f6_zero_shock_fixture,
            request: r#"{"version":1,"command":"start_treatment","medic":0,"patient":1,"kind":"shock"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"85ba4f9146445383","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
        F6Row {
            name: "requested_shock_zero_shock_casualty",
            source: f6_zero_shock_source,
            fixture: f6_zero_shock_fixture,
            request: r#"{"version":1,"command":"request_treatment","patient":1,"kind":"shock"}"#,
            expected: r#"{"blocked":null,"clock":0,"digest":"85ba4f9146445383","events":[],"terminal_error":"invalid_treatment","version":1}"#,
        },
    ];

    #[test]
    fn gate_c2_f6_treatment_target_semantic_matrix() {
        assert_eq!(F6_ROWS.len(), 16);
        let mut names = std::collections::BTreeSet::new();
        for row in &F6_ROWS {
            assert!(names.insert(row.name));
            let source = (row.source)();
            let fixture = (row.fixture)();
            fixture.assert_world(&source);
            let pre = source.snapshot();
            let restored = verified_restore(&source, &fixture);
            let (response, rejected) = exchange(req(row.request), false, restored);
            assert_eq!(status(&response), "HTTP/1.1 200 OK", "{}", row.name);
            assert_eq!(raw_body(&response), row.expected.as_bytes(), "{}", row.name);
            fixture.assert_world(&rejected);
            assert_eq!(rejected.snapshot(), pre, "{}", row.name);
            let again = verified_restore(&rejected, &fixture);
            assert_eq!(again.snapshot(), pre, "{}", row.name);
        }
        assert_eq!(names.len(), 16);
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
