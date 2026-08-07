use serde::Deserialize;
use sim_core::{Command, Event, ScheduledCommand, SoldierSpec, Stock, TimedEvent, World};
use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Instant;

const MAX_REQUEST: usize = 64 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().any(|a| a == "--benchmark") {
        return benchmark();
    }
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    let mut world = World::new(1);
    eprintln!("sim-server listening on http://127.0.0.1:8080");
    for stream in listener.incoming() {
        let mut stream = stream?;
        let (status, kind, body) = read_and_handle(&mut stream, &mut world);
        write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len())?;
        stream.write_all(&body)?;
    }
    Ok(())
}

fn read_and_handle(r: &mut impl Read, w: &mut World) -> (&'static str, &'static str, Vec<u8>) {
    let mut b = Vec::new();
    if r.take((MAX_REQUEST + 1) as u64)
        .read_to_end(&mut b)
        .is_err()
        || b.len() > MAX_REQUEST
    {
        return (
            "413 Payload Too Large",
            "application/json",
            b"{\"error\":\"request_too_large\"}".to_vec(),
        );
    }
    let Some(split) = b.windows(4).position(|x| x == b"\r\n\r\n") else {
        return bad();
    };
    let head = String::from_utf8_lossy(&b[..split]);
    let body = &b[split + 4..];
    if head.starts_with("GET /health ") {
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
    if head.starts_with("GET /snapshot ") {
        return ("200 OK", "application/octet-stream", w.snapshot());
    }
    if !head.starts_with("POST /v1/command ") {
        return ("404 Not Found", "text/plain", b"not found\n".to_vec());
    }
    let declared = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Content-Length: ")
                .or_else(|| l.strip_prefix("content-length: "))
        })
        .and_then(|x| x.parse::<usize>().ok());
    if declared != Some(body.len()) {
        return bad();
    }
    let wire: Wire = match serde_json::from_slice(body) {
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
            o.error.as_ref().map(|e| format!("{e:?}")),
            w.state_digest(),
        )
        .into_bytes(),
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
fn outcome_json(clock: u64, events: &[TimedEvent], error: Option<String>, digest: u64) -> String {
    let events = events
        .iter()
        .map(|x| format!("{{\"at\":{},\"event\":\"{}\"}}", x.at, event_name(x.event)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"version\":1,\"clock\":{clock},\"events\":[{events}],\"terminal_error\":{},\"digest\":\"{digest:016x}\"}}",error.map_or_else(||"null".into(),|e|format!("\"{e}\"")))
}
fn event_name(e: Event) -> &'static str {
    match e {
        Event::SoldierSpawned { .. } => "soldier_spawned",
        Event::SoldierRemoved { .. } => "soldier_removed",
        Event::SquadCreated { .. } => "squad_created",
        Event::OfficerAssigned { .. } => "officer_assigned",
        Event::StockpileCreated { .. } => "stockpile_created",
        Event::TransferCompleted { .. } => "transfer_completed",
        Event::RegionFidelityChanged { .. } => "region_fidelity_changed",
        Event::Scheduled { .. } => "scheduled",
        Event::ScheduleCancelled { .. } => "schedule_cancelled",
        Event::RandomGenerated { .. } => "random_generated",
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
    use std::io::Cursor;
    fn req(body: &str) -> Vec<u8> {
        format!(
            "POST /v1/command HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }
    #[test]
    fn command_changes_once_and_malformed_is_atomic() {
        let mut w = World::new(0);
        let body = r#"{"version":1,"command":"create_stockpile","id":7,"ammunition":9,"supplies":3,"target":null,"from":null,"to":null,"cell":null,"hot":null,"at":null}"#;
        let (a, _, response) = read_and_handle(&mut Cursor::new(req(body)), &mut w);
        assert_eq!(a, "200 OK");
        assert!(String::from_utf8(response)
            .unwrap()
            .contains("stockpile_created"));
        assert_eq!(w.stockpile(7).unwrap().ammunition, 9);
        let digest = w.state_digest();
        let (a, _, _) = read_and_handle(&mut Cursor::new(req("{}")), &mut w);
        assert_eq!(a, "400 Bad Request");
        assert_eq!(w.state_digest(), digest)
    }
}
