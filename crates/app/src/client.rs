//! Talking to a session daemon: connecting, starting one, and framing messages.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use herdstead_core::paths::Paths;
use herdstead_core::protocol::{decode_line, encode_line, Event, Request, Snapshot};

/// A framed connection to a session daemon.
pub struct Connection {
    pub stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Connection {
    pub fn connect(paths: &Paths, session: &str) -> Result<Self> {
        let path = paths.socket(session);
        let stream = UnixStream::connect(&path)
            .with_context(|| format!("connecting to {}", path.display()))?;
        let reader = BufReader::new(stream.try_clone().context("cloning the socket")?);
        Ok(Self { stream, reader })
    }

    pub fn send(&mut self, request: &Request) -> Result<()> {
        let bytes = encode_line(request).context("encoding a request")?;
        self.stream.write_all(&bytes).context("writing a request")?;
        self.stream.flush().context("flushing a request")
    }

    /// Reads the next event, skipping blank keep-alive lines.
    pub fn next_event(&mut self) -> Result<Event> {
        loop {
            let mut line = String::new();
            let read = self
                .reader
                .read_line(&mut line)
                .context("reading an event")?;
            if read == 0 {
                bail!("the session daemon closed the connection");
            }
            if line.trim().is_empty() {
                continue;
            }
            return decode_line(line.trim_end()).context("decoding an event");
        }
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) {
        self.stream.set_read_timeout(timeout).ok();
    }

    /// Asks the daemon to stop and waits for its socket to disappear.
    pub fn kill(paths: &Paths, session: &str) -> Result<()> {
        let mut connection = Self::connect(paths, session)?;
        connection.send(&Request::Kill)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if !paths.socket(session).exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        bail!("the session did not stop within 3s")
    }

    /// Connects and returns the first snapshot, for `ls`.
    pub fn snapshot_once(paths: &Paths, session: &str) -> Result<Snapshot> {
        let mut connection = Self::connect(paths, session)?;
        connection.set_read_timeout(Some(Duration::from_millis(600)));
        connection.send(&Request::Inspect {
            session: session.to_string(),
        })?;
        match connection.next_event()? {
            Event::Snapshot { snapshot } => Ok(*snapshot),
            Event::Detached { reason } => bail!("detached: {reason}"),
            Event::Error { message } => bail!("{message}"),
        }
    }
}

/// Connects to a session, starting its daemon when it is not running yet.
pub fn connect_or_spawn(paths: &Paths, session: &str, cwd: &Path) -> Result<Connection> {
    if let Ok(connection) = Connection::connect(paths, session) {
        return Ok(connection);
    }
    spawn_server(paths, session, cwd)?;
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last = None;
    while Instant::now() < deadline {
        match Connection::connect(paths, session) {
            Ok(connection) => return Ok(connection),
            Err(err) => last = Some(err),
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(last.unwrap_or_else(|| anyhow!("the session daemon did not start")))
}

/// Starts the daemon in its own session, with output going to a log file.
pub fn spawn_server(paths: &Paths, session: &str, cwd: &Path) -> Result<()> {
    paths.ensure()?;
    let exe = std::env::current_exe().context("finding the herdstead binary")?;
    let log_path = paths.log_file(session);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;
    let mut command = Command::new(exe);
    command
        .arg("--server")
        .arg("--session")
        .arg(session)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .env("HERDSTEAD_SERVER", "1");
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            // A new session keeps the panes alive when the terminal closes.
            if libc_setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().context("starting the session daemon")?;
    Ok(())
}

#[cfg(unix)]
fn libc_setsid() -> i32 {
    extern "C" {
        fn setsid() -> i32;
    }
    unsafe { setsid() }
}
