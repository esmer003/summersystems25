// Website Status Checker (simplified per request)
// Features: Periodic monitoring, HTTP header validation, and statistics only
// Concurrency: std::thread + std::sync::mpsc (blocking; no async/Actix/Tokio)
// HTTP client: ureq
// Allowed deps only: ureq, serde (serde optional)
// Build:
//   cargo new sitewatch && cd sitewatch
//   # Replace Cargo.toml with the one below, and put this file as src/main.rs
// Run examples:
//   cargo run -- --workers 50 --timeout-ms 5000 --retries 1 \
//     https://example.org https://httpbin.org/status/503
//   cargo run -- --period 15 https://example.org https://httpbin.org/delay/2

use std::io; // used for ENTER-to-stop in periodic mode
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};
use std::{env, fs};

// --- Minimal stand-in for `chrono::{DateTime, Utc}` to satisfy the struct signature ---
#[allow(non_camel_case_types)]
mod chrono_shim {
    use std::marker::PhantomData;

    #[derive(Clone, Copy, Debug)]
    pub struct Utc;

    #[derive(Clone, Copy, Debug)]
    pub struct DateTime<T> {
        pub(crate) inner: std::time::SystemTime,
        pub(crate) _marker: PhantomData<T>,
    }

    impl<T> DateTime<T> {
        pub fn now() -> Self {
            Self { inner: std::time::SystemTime::now(), _marker: PhantomData }
        }
        pub fn as_system_time(&self) -> std::time::SystemTime { self.inner }
    }

    impl<T> From<std::time::SystemTime> for DateTime<T> {
        fn from(st: std::time::SystemTime) -> Self {
            Self { inner: st, _marker: PhantomData }
        }
    }
}
use chrono_shim::{DateTime, Utc};

// -------------------- Config & CLI --------------------
#[derive(Debug, Clone)]
struct Config {
    workers: usize,
    timeout: Duration,
    retries: u32,
    period_secs: u64, // 0 means single run
    header_checks: Vec<(String, String)>, // exact equals checks
    urls: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workers: 50,
            timeout: Duration::from_millis(5000),
            retries: 0,
            period_secs: 0,
            header_checks: Vec::new(),
            urls: Vec::new(),
        }
    }
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config::default();
    let mut args = env::args().skip(1); // skip binary name

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--workers" => {
                let n = args.next().ok_or("--workers requires a value")?;
                cfg.workers = n.parse().map_err(|_| "invalid --workers value")?;
            }
            "--timeout-ms" => {
                let n = args.next().ok_or("--timeout-ms requires a value")?;
                let ms: u64 = n.parse().map_err(|_| "invalid --timeout-ms value")?;
                cfg.timeout = Duration::from_millis(ms);
            }
            "--retries" => {
                let n = args.next().ok_or("--retries requires a value")?;
                cfg.retries = n.parse().map_err(|_| "invalid --retries value")?;
            }
            "--period" => {
                let n = args.next().ok_or("--period requires seconds")?;
                cfg.period_secs = n.parse().map_err(|_| "invalid --period value")?;
            }
            "--header" => {
                let kv = args.next().ok_or("--header requires KEY=VALUE")?;
                let (k, v) = parse_header_kv(&kv).map_err(|e| format!("--header: {}", e))?;
                cfg.header_checks.push((k, v));
            }
            "--file" => {
                let path = args.next().ok_or("--file requires a path")?;
                let content = fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {}", path, e))?;
                for line in content.lines() {
                    let url = line.trim();
                    if !url.is_empty() && !url.starts_with('#') {
                        cfg.urls.push(url.to_string());
                    }
                }
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("unknown flag: {}", arg));
                } else {
                    cfg.urls.push(arg);
                }
            }
        }
    }

    if cfg.urls.is_empty() {
        return Err("no URLs provided. Pass them as args or with --file path".into());
    }

    // Avoid spawning more workers than tasks (but keep at least 1)
    cfg.workers = cfg.workers.max(1).min(cfg.urls.len().max(1));
    Ok(cfg)
}

fn parse_header_kv(s: &str) -> Result<(String, String), &'static str> {
    let mut split = s.splitn(2, '=');
    let k = split.next().ok_or("missing key")?.trim();
    let v = split.next().ok_or("missing value")?.trim();
    if k.is_empty() { return Err("empty key"); }
    Ok((k.to_string(), v.to_string()))
}

// -------------------- Core Types --------------------
#[derive(Debug, Clone)]
struct WebsiteStatus {
    url: String,
    status: Result<u16, String>,
    response_time: Duration,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct Stats {
    samples: u64,
    ok: u64,
    total_response: Duration,
}

impl Stats {
    fn new() -> Self { Self { samples: 0, ok: 0, total_response: Duration::from_millis(0) } }
    fn record(&mut self, s: &WebsiteStatus) {
        self.samples += 1;
        if let Ok(code) = s.status { if (200..=399).contains(&code) { self.ok += 1; } }
        self.total_response += s.response_time;
    }
    fn avg_ms(&self) -> u128 {
        if self.samples == 0 { 0 } else { (self.total_response.as_millis()) / (self.samples as u128) }
    }
    fn uptime_pct(&self) -> f64 {
        if self.samples == 0 { 0.0 } else { (self.ok as f64) * 100.0 / (self.samples as f64) }
    }
}

// -------------------- Worker Pool --------------------
#[derive(Debug)]
enum Job {
    Check(String),
}

fn spawn_workers(
    n: usize,
    job_rx: Arc<Mutex<mpsc::Receiver<Job>>>,
    result_tx: mpsc::Sender<WebsiteStatus>,
    timeout: Duration,
    retries: u32,
    header_checks: Vec<(String, String)>,
    shutdown: Arc<AtomicBool>,
) -> Vec<thread::JoinHandle<()>> {
    let mut handles = Vec::with_capacity(n);

    for _ in 0..n {
        let job_rx = job_rx.clone();
        let result_tx = result_tx.clone();
        let header_checks = header_checks.clone();
        let shutdown = shutdown.clone();

        // Build one Agent per worker (blocking client)
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout_read(timeout)
            .timeout_write(timeout)
            .build();

        let handle = thread::spawn(move || {
            loop {
                if shutdown.load(Ordering::Relaxed) { break; }
                let job_opt = {
                    let rx = job_rx.lock().unwrap();
                    rx.recv().ok()
                };
                match job_opt {
                    Some(Job::Check(url)) => {
                        let status = check_once_with_retries(&agent, &url, retries, &header_checks);
                        let _ = result_tx.send(status);
                    }
                    None => break, // channel closed
                }
            }
        });
        handles.push(handle);
    }

    handles
}

fn check_once_with_retries(
    agent: &ureq::Agent,
    url: &str,
    retries: u32,
    header_checks: &[(String, String)],
) -> WebsiteStatus {
    let mut attempt = 0;
    let start_all = Instant::now();

    loop {
        let start = Instant::now();
        let ts: DateTime<Utc> = DateTime::now();
        match agent.get(url).call() {
            Ok(resp) => {
                let code = resp.status();
                // Header validation (exact matches)
                for (k, expected) in header_checks.iter() {
                    match resp.header(k) {
                        Some(v) if v == expected => {},
                        Some(v) => {
                            return WebsiteStatus {
                                url: url.to_string(),
                                status: Err(format!("header {} mismatch: got '{}', expected '{}'", k, v, expected)),
                                response_time: start.elapsed(),
                                timestamp: ts,
                            }
                        }
                        None => {
                            return WebsiteStatus {
                                url: url.to_string(),
                                status: Err(format!("missing header {}", k)),
                                response_time: start.elapsed(),
                                timestamp: ts,
                            }
                        }
                    }
                }

                return WebsiteStatus {
                    url: url.to_string(),
                    status: Ok(code as u16),
                    response_time: start.elapsed(),
                    timestamp: ts,
                };
            }
            Err(ureq::Error::Status(code, _resp)) => {
                // HTTP status >= 400 (still a response)
                return WebsiteStatus {
                    url: url.to_string(),
                    status: Ok(code as u16),
                    response_time: start.elapsed(),
                    timestamp: DateTime::now(),
                };
            }
            Err(e) => {
                attempt += 1;
                if attempt > retries {
                    return WebsiteStatus {
                        url: url.to_string(),
                        status: Err(format!("transport error: {}", e)),
                        response_time: start_all.elapsed(),
                        timestamp: DateTime::now(),
                    };
                }
                // small fixed backoff to avoid hammering
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

// -------------------- Runner --------------------
fn run_once(cfg: &Config) -> Vec<WebsiteStatus> {
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (result_tx, result_rx) = mpsc::channel::<WebsiteStatus>();
    let shutdown = Arc::new(AtomicBool::new(false));

    let job_rx_arc = Arc::new(Mutex::new(job_rx));

    let workers = spawn_workers(
        cfg.workers,
        job_rx_arc,
        result_tx,
        cfg.timeout,
        cfg.retries,
        cfg.header_checks.clone(),
        shutdown.clone(),
    );

    // Enqueue jobs
    for url in &cfg.urls {
        job_tx.send(Job::Check(url.clone())).expect("send job");
    }

    // Close job channel so workers exit after queue drains
    drop(job_tx);

    // Collect results
    let mut results = Vec::with_capacity(cfg.urls.len());
    for _ in 0..cfg.urls.len() {
        match result_rx.recv() {
            Ok(r) => results.push(r),
            Err(_) => break,
        }
    }

    // Signal shutdown & join workers
    shutdown.store(true, Ordering::Relaxed);
    for h in workers { let _ = h.join(); }

    results
}

fn print_results(results: &[WebsiteStatus]) {
    println!("\nResults ({} checks):", results.len());
    println!("{:<5} | {:<8} | {:<7} | {:<13} | {}", "#", "Status", "ms", "ts(ms)", "URL");
    println!("{}", "-".repeat(100));
    for (i, r) in results.iter().enumerate() {
        let code_str = match r.status {
            Ok(c) => c.to_string(),
            Err(_) => "ERR".to_string(),
        };
        let ts_ms = r.timestamp.as_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        println!("{:<5} | {:<8} | {:<7} | {:<13} | {}", i + 1, code_str, r.response_time.as_millis(), ts_ms, r.url);
        if let Err(ref e) = r.status { println!("        ↳ error: {}", e); }
    }
}

fn print_round_stats(results: &[WebsiteStatus]) {
    let total = results.len() as f64;
    let successes = results.iter().filter(|r| matches!(r.status, Ok(c) if (200..=399).contains(&c))).count();
    let total_duration: Duration = results.iter().map(|r| r.response_time).sum();
    let avg_ms = if results.is_empty() { 0 } else { (total_duration.as_millis() / (results.len() as u128)) as u128 };
    let uptime = if total == 0.0 { 0.0 } else { (successes as f64) * 100.0 / total };
    println!("\nRound stats: avg={}ms, uptime={:.2}% ({}/{})", avg_ms, uptime, successes, results.len());
}

fn run_periodic(cfg: Config) {
    assert!(cfg.period_secs > 0);
    let shutdown = Arc::new(AtomicBool::new(false));

    // Stdin watcher for graceful shutdown
    {
        let sd = shutdown.clone();
        thread::spawn(move || {
            let mut _dummy = String::new();
            let _ = io::stdin().read_line(&mut _dummy); // press ENTER to stop
            sd.store(true, Ordering::Relaxed);
        });
    }

    // Aggregated stats per URL
    use std::collections::HashMap;
    let mut agg: HashMap<String, Stats> = HashMap::new();

    println!("Periodic monitoring every {}s. Press ENTER to stop...", cfg.period_secs);

    while !shutdown.load(Ordering::Relaxed) {
        let results = run_once(&cfg);
        print_results(&results);
        print_round_stats(&results);

        for r in &results {
            agg.entry(r.url.clone()).or_insert_with(Stats::new).record(r);
        }

        // Sleep for the period, but wake early if shutdown
        let period = Duration::from_secs(cfg.period_secs);
        let start = Instant::now();
        while start.elapsed() < period {
            if shutdown.load(Ordering::Relaxed) { break; }
            thread::sleep(Duration::from_millis(100));
        }
    }

    // Print aggregated stats
    println!("\nAggregate statistics:");
    println!("{:<7} | {:<7} | {:<7} | {}", "samples", "uptime%", "avg ms", "URL");
    println!("{}", "-".repeat(80));
    let mut keys: Vec<_> = agg.keys().cloned().collect();
    keys.sort();
    for url in keys {
        let s = &agg[&url];
        println!("{:<7} | {:<7.2} | {:<7} | {}", s.samples, s.uptime_pct(), s.avg_ms(), url);
    }
}

fn main() {
    match parse_args() {
        Ok(cfg) => {
            if cfg.period_secs == 0 {
                let results = run_once(&cfg);
                print_results(&results);
                print_round_stats(&results);
            } else {
                run_periodic(cfg);
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("\nUsage: sitewatch [FLAGS] <url> [<url> ...]\n");
            eprintln!("Flags:");
            eprintln!("  --workers <N>        Number of worker threads (default 50)");
            eprintln!("  --timeout-ms <MS>    Request timeout in milliseconds (default 5000)");
            eprintln!("  --retries <N>        Max retries per website on transport errors (default 0)");
            eprintln!("  --period <SECS>      Periodic monitoring interval in seconds (0 = single run)");
            eprintln!("  --header K=V         Require exact HTTP header K=V (repeatable)");
            eprintln!("  --file <PATH>        Read URLs (one per line) from PATH");
            eprintln!("\nExamples:");
            eprintln!("  sitewatch --workers 50 --timeout-ms 5000 https://example.org https://httpbin.org/status/500");
            eprintln!("  sitewatch --period 10 --retries 1 --header 'Content-Type=text/plain' --file urls.txt");
        }
    }
}

// -------------------- Tests --------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};

    fn spawn_simple_http_server(port: u16) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind test server");
            for stream in listener.incoming() {
                let mut stream = match stream { Ok(s) => s, Err(_) => continue };
                handle_conn(&mut stream);
            }
        })
    }

    fn handle_conn(stream: &mut TcpStream) {
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        // naive path sniffing
        let req = String::from_utf8_lossy(&buf);
        let path = req.split_whitespace().nth(1).unwrap_or("/");
        match path {
            "/ok" => respond(stream, 200, "OK", "text/plain"),
            "/slow" => { thread::sleep(Duration::from_millis(300)); respond(stream, 200, "SLOW", "text/plain") }
            "/err" => respond(stream, 503, "ERR", "text/plain"),
            _ => respond(stream, 404, "NOPE", "text/plain"),
        }
    }

    fn respond(stream: &mut TcpStream, code: u16, body: &str, ctype: &str) {
        let status_line = match code { 200 => "HTTP/1.1 200 OK", 404 => "HTTP/1.1 404 Not Found", 503 => "HTTP/1.1 503 Service Unavailable", _ => "HTTP/1.1 500 Internal Server Error" };
        let resp = format!(
            "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status_line, ctype, body.len(), body
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }

    #[test]
    fn test_parse_header_kv() {
        assert_eq!(parse_header_kv("A=B").unwrap(), ("A".to_string(), "B".to_string()));
        assert!(parse_header_kv("A=").is_ok());
        assert!(parse_header_kv("=B").is_err());
    }

    #[test]
    fn test_run_once_ok_and_err() {
        let port = 34567;
        let _server = spawn_simple_http_server(port);
        // give server a moment to bind
        thread::sleep(Duration::from_millis(50));

        let cfg = Config {
            workers: 4,
            timeout: Duration::from_millis(2000),
            retries: 0,
            period_secs: 0,
            header_checks: vec![("Content-Type".into(), "text/plain".into())],
            urls: vec![
                format!("http://127.0.0.1:{}/ok", port),
                format!("http://127.0.0.1:{}/err", port),
            ],
        };

        let res = run_once(&cfg);
        assert_eq!(res.len(), 2);
        let ok = res.iter().find(|r| r.url.ends_with("/ok")).unwrap();
        assert!(matches!(ok.status, Ok(c) if c == 200));
        let err = res.iter().find(|r| r.url.ends_with("/err")).unwrap();
        assert!(matches!(err.status, Ok(c) if c == 503));
    }

    #[test]
    fn test_header_check() {
        let port = 34568;
        let _server = spawn_simple_http_server(port);
        thread::sleep(Duration::from_millis(50));
        let cfg = Config {
            workers: 1,
            timeout: Duration::from_millis(2000),
            retries: 0,
            period_secs: 0,
            header_checks: vec![("Content-Type".into(), "text/plain".into())],
            urls: vec![format!("http://127.0.0.1:{}/ok", port)],
        };
        let res = run_once(&cfg);
        let r = &res[0];
        assert!(matches!(r.status, Ok(200)));
    }

    #[test]
    fn test_timeout_and_retry() {
        let port = 34569;
        let _server = spawn_simple_http_server(port);
        thread::sleep(Duration::from_millis(50));
        let cfg = Config {
            workers: 2,
            timeout: Duration::from_millis(50), // likely to timeout on /slow
            retries: 1,
            period_secs: 0,
            header_checks: vec![],
            urls: vec![format!("http://127.0.0.1:{}/slow", port)],
        };
        let res = run_once(&cfg);
        let r = &res[0];
        assert!(matches!(r.status, Err(_)));
    }
}