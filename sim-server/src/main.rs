use serde::Deserialize;
use serde_json::{json, Value};
use sim_core::{
    BlockedCommand, Command, Event, ScheduledCommand, SimError, SoldierSpec, Stock, TimedEvent,
    World,
};
use std::env;
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u32,
    command: String,
    target: Option<u64>,
    id: Option<u64>,
    from: Option<u32>,
    to: Option<u32>,
    ammunition: Option<u64>,
    supplies: Option<u64>,
    cell: Option<u32>,
    hot: Option<bool>,
    at: Option<u64>,
}
impl Wire {
    fn into_command(self) -> Result<Command, ()> {
        if self.version != 1 {
            return Err(());
        }
        match self.command.as_str() {
            "advance_to" => Ok(Command::AdvanceTo {
                target: self.target.ok_or(())?,
            }),
            "create_stockpile" => Ok(Command::CreateStockpile {
                id: u32::try_from(self.id.ok_or(())?).map_err(|_| ())?,
                initial: Stock {
                    ammunition: self.ammunition.ok_or(())?,
                    supplies: self.supplies.ok_or(())?,
                },
            }),
            "transfer" => Ok(Command::Transfer {
                from: self.from.ok_or(())?,
                to: self.to.ok_or(())?,
                ammunition: self.ammunition.ok_or(())?,
                supplies: self.supplies.ok_or(())?,
            }),
            "set_region_hot" => Ok(Command::SetRegionHot {
                cell: self.cell.ok_or(())?,
                hot: self.hot.ok_or(())?,
            }),
            "schedule_hot" => Ok(Command::Schedule {
                at: self.at.ok_or(())?,
                command: ScheduledCommand::SetRegionHot {
                    cell: self.cell.ok_or(())?,
                    hot: self.hot.ok_or(())?,
                },
            }),
            "schedule_transfer" => Ok(Command::Schedule {
                at: self.at.ok_or(())?,
                command: ScheduledCommand::Transfer {
                    from: self.from.ok_or(())?,
                    to: self.to.ok_or(())?,
                    ammunition: self.ammunition.ok_or(())?,
                    supplies: self.supplies.ok_or(())?,
                },
            }),
            "cancel_scheduled" => Ok(Command::CancelScheduled {
                id: self.id.ok_or(())?,
            }),
            _ => Err(()),
        }
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
    };
    json!({"at":x.at,"event":payload})
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
    for i in 0..count {
        apply_ok(
            &mut w,
            Command::SpawnSoldier {
                spec: SoldierSpec {
                    faction: (i % 2) as u16,
                    ..SoldierSpec::default()
                },
            },
        )
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
    let advance = Instant::now();
    let dense_out = w.apply(Command::AdvanceTo { target: 20 });
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
    let sparse_out = w.apply(Command::AdvanceTo { target: 60 });
    let advance_time = advance.elapsed();
    assert!(dense_out.error.is_none() && sparse_out.error.is_none());
    let needs = Instant::now();
    let checksum = w.needs_checksum();
    let needs_time = needs.elapsed();
    let combined = advance_time + needs_time;
    let rate = 60.0 / combined.as_secs_f64();
    let st = Instant::now();
    let snapshot = w.snapshot();
    let snapshot_time = st.elapsed();
    let dt = Instant::now();
    let digest = w.state_digest();
    let digest_time = dt.elapsed();
    let (rss, peak) = memory_kib();
    println!("soldiers={count}\ninitialization_seconds={:.6}\ndense_scheduler_commands={dense}\nadvance_simulated_seconds=60\nadvance_events={}\nhot_cell_steps_included=true\nfull_needs_pass_included=true\ncombined_advance_wall_seconds={:.6}\nsimulated_seconds_per_wall_second={rate:.3}\ndesign_goal_simulated_seconds_per_wall_second=1.000\ndesign_goal_met={}\nneeds_checksum={checksum:016x}\nsnapshot_seconds={:.6}\nsnapshot_bytes={}\ndigest_seconds={:.6}\ndigest={digest:016x}\ncurrent_rss_kib={rss}\npeak_rss_kib={peak}",init.as_secs_f64(),dense_out.events.len()+sparse_out.events.len(),combined.as_secs_f64(),rate>=1.0,snapshot_time.as_secs_f64(),snapshot.len(),digest_time.as_secs_f64());
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
