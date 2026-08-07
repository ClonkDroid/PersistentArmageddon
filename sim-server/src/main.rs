use sim_core::{Command, SoldierSpec, Stock, World};
use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().any(|a| a == "--benchmark") {
        return benchmark();
    }
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    let world = World::new(1);
    eprintln!("sim-server listening on http://127.0.0.1:8080");
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut request = [0; 1024];
        let n = stream.read(&mut request)?;
        let path = String::from_utf8_lossy(&request[..n]);
        let (status, content_type, body) = if path.starts_with("GET /health ") {
            (
                "200 OK",
                "application/json",
                format!(
                    "{{\"status\":\"ok\",\"clock\":{},\"soldiers\":{},\"digest\":\"{:016x}\"}}",
                    world.clock(),
                    world.soldier_count(),
                    world.state_digest()
                )
                .into_bytes(),
            )
        } else if path.starts_with("GET /snapshot ") {
            ("200 OK", "application/octet-stream", world.snapshot())
        } else {
            ("404 Not Found", "text/plain", b"not found\n".to_vec())
        };
        write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len())?;
        stream.write_all(&body)?;
    }
    Ok(())
}

fn benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = env::var("PA_SOLDIERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_410_000);
    let start = Instant::now();
    let mut w = World::new(0x5eed);
    for i in 0..count {
        w.spawn(SoldierSpec {
            faction: (i % 2) as u16,
            ..SoldierSpec::default()
        })?;
    }
    let init = start.elapsed();
    w.set_stockpile(
        1,
        Stock {
            ammunition: count as u64 * 30,
            supplies: count as u64 * 2,
        },
    );
    w.set_stockpile(2, Stock::default());
    w.schedule(
        30,
        Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 1000,
            supplies: 500,
        },
    );
    let advance = Instant::now();
    w.advance_to(60)?;
    let elapsed = advance.elapsed();
    let rss = resident_memory_kib();
    println!("soldiers={count}\ninit_seconds={:.6}\nadvance_seconds={:.6}\nsim_seconds=60\nthroughput_sim_seconds_per_wall_second={:.2}\napprox_rss_kib={}\ndigest={:016x}",init.as_secs_f64(),elapsed.as_secs_f64(),60.0/elapsed.as_secs_f64(),rss,w.state_digest());
    Ok(())
}
fn resident_memory_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}
