use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::accounts::{self, Account, AccountAuth};
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::secrets::SecretStore;

use super::ports::{self, BridgePorts};

const READY_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const SIDECAR_NAME: &str = "auramail-proton-bridge";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtonAccessLimited {
    pub username: String,
    pub message: String,
    pub appeal_url: String,
}

pub trait ProtonEvents: Send + Sync {
    fn access_limited(&self, status: ProtonAccessLimited);
}

#[derive(Default)]
struct State {
    child: Option<Child>,
    ports: Option<BridgePorts>,
    access_limited: Option<ProtonAccessLimited>,
    emitted: HashSet<(String, String)>,
    generation: u64,
}

struct Shared {
    events: Arc<dyn ProtonEvents>,
    state: Mutex<State>,
}

impl Shared {
    fn publish_access_limited(&self, status: ProtonAccessLimited, generation: u64) {
        let recipient = {
            let mut state = lock(&self.state);
            if state.generation != generation {
                return;
            }
            state.access_limited = Some(status.clone());
            let key = (status.username.clone(), status.message.clone());
            state.emitted.insert(key).then(|| Arc::clone(&self.events))
        };
        if let Some(events) = recipient {
            events.access_limited(status);
        }
    }
}

pub struct ProtonBridge {
    pub(crate) db: Arc<Database>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) config_dir: PathBuf,
    pub(crate) sidecar: PathBuf,
    shared: Arc<Shared>,
    start_lock: Mutex<()>,
}

impl ProtonBridge {
    pub fn new(
        events: Arc<dyn ProtonEvents>,
        db: Arc<Database>,
        secrets: Arc<dyn SecretStore>,
        config_dir: PathBuf,
        sidecar: PathBuf,
    ) -> ProtonBridge {
        ProtonBridge {
            db,
            secrets,
            config_dir,
            sidecar,
            shared: Arc::new(Shared {
                events,
                state: Mutex::new(State::default()),
            }),
            start_lock: Mutex::new(()),
        }
    }

    pub fn start_if_needed(&self) -> AppResult<Option<BridgePorts>> {
        let _guard = lock(&self.start_lock);
        self.start_inner()
    }

    pub fn stop(&self) {
        let _guard = lock(&self.start_lock);
        self.stop_inner();
    }

    pub fn restart(&self) -> AppResult<()> {
        let _guard = lock(&self.start_lock);
        self.stop_inner();
        self.start_inner().map(|_| ())
    }

    pub fn is_running(&self) -> bool {
        let mut state = lock(&self.shared.state);
        let running = match state.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if !running && state.child.is_some() {
            clear_stopped_child(&mut state);
        }
        running
    }

    pub fn access_limited(&self) -> Option<ProtonAccessLimited> {
        lock(&self.shared.state).access_limited.clone()
    }

    fn start_inner(&self) -> AppResult<Option<BridgePorts>> {
        let proton_accounts = self.proton_accounts()?;
        if proton_accounts.is_empty() {
            self.stop_inner();
            return Ok(None);
        }
        if let Some(ports) = self.running_ports() {
            return Ok(Some(ports));
        }
        if auth_lock_held(&self.config_dir)? {
            return Err(AppError::Message("Proton sign-in in progress".into()));
        }

        let requested = ports::select(&self.db)?;
        let (mut child, ready, stderr) = self.spawn(requested)?;
        match ready.recv_timeout(READY_TIMEOUT) {
            Ok(ReaderEvent::Ready(reported)) => {
                let effective = BridgePorts {
                    imap: if reported.imap == 0 {
                        requested.imap
                    } else {
                        reported.imap
                    },
                    smtp: if reported.smtp == 0 {
                        requested.smtp
                    } else {
                        reported.smtp
                    },
                };
                if let Err(error) = ports::update_account_rows(&self.db, effective) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
                let mut state = lock(&self.shared.state);
                state.child = Some(child);
                state.ports = Some(effective);
                Ok(Some(effective))
            }
            Ok(ReaderEvent::Stopped) | Err(RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(start_failure(&stderr.wait_text()))
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(AppError::Message(
                    "Proton bridge did not start in time.".into(),
                ))
            }
        }
    }

    fn stop_inner(&self) {
        let child = {
            let mut state = lock(&self.shared.state);
            state.ports = None;
            state.access_limited = None;
            state.emitted.clear();
            state.generation = state.generation.wrapping_add(1);
            state.child.take()
        };
        let Some(mut child) = child else {
            return;
        };
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(b"{\"op\":\"shutdown\"}\n");
            let _ = stdin.flush();
        }
        child.stdin.take();
        wait_for_exit(&mut child);
    }

    fn running_ports(&self) -> Option<BridgePorts> {
        let mut state = lock(&self.shared.state);
        let running = match state.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if running {
            state.ports
        } else {
            if state.child.is_some() {
                clear_stopped_child(&mut state);
            }
            None
        }
    }

    fn proton_accounts(&self) -> AppResult<Vec<Account>> {
        Ok(accounts::list(&self.db)?
            .into_iter()
            .filter(|account| account.auth_type == AccountAuth::Proton)
            .collect())
    }

    fn spawn(
        &self,
        requested: BridgePorts,
    ) -> AppResult<(Child, Receiver<ReaderEvent>, StderrCapture)> {
        let mut child = Command::new(&self.sidecar)
            .arg("serve")
            .arg("--imap-port")
            .arg(requested.imap.to_string())
            .arg("--smtp-port")
            .arg(requested.smtp.to_string())
            .env("XDG_CONFIG_HOME", &self.config_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| {
                AppError::Message("Proton support is unavailable in this build.".into())
            })?;

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message("Proton bridge failed to start.".into()));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message("Proton bridge failed to start.".into()));
            }
        };

        let stderr = StderrCapture::spawn(stderr);
        let generation = {
            let mut state = lock(&self.shared.state);
            state.generation = state.generation.wrapping_add(1);
            state.access_limited = None;
            state.emitted.clear();
            state.generation
        };
        let ready = read_stdout(Arc::clone(&self.shared), stdout, generation);
        Ok((child, ready, stderr))
    }

    pub(crate) fn forget_account(&self, username: &str) -> AppResult<()> {
        let dir = self.config_dir.join("hydroxide");
        ensure_private_dir(&dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(dir.join("auth.lock"))?;
        lock.try_lock_exclusive().map_err(|error| {
            if error.kind() == ErrorKind::WouldBlock {
                AppError::Message("Proton sign-in in progress".into())
            } else {
                AppError::Io(error)
            }
        })?;

        self.stop();
        let result = remove_auth_entry(&dir.join("auth.json"), username);
        let _ = FileExt::unlock(&lock);
        result
    }
}

impl Drop for ProtonBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

enum ReaderEvent {
    Ready(BridgePorts),
    Stopped,
}

#[derive(Deserialize)]
struct BridgeEvent {
    event: String,
    #[serde(default)]
    imap_port: u16,
    #[serde(default)]
    smtp_port: u16,
    #[serde(default)]
    username: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    appeal_url: String,
}

fn read_stdout(shared: Arc<Shared>, stdout: ChildStdout, generation: u64) -> Receiver<ReaderEvent> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut ready_sent = false;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            let Ok(event) = serde_json::from_str::<BridgeEvent>(&line) else {
                continue;
            };
            match event.event.as_str() {
                "ready" if !ready_sent => {
                    ready_sent = true;
                    let _ = sender.send(ReaderEvent::Ready(BridgePorts {
                        imap: event.imap_port,
                        smtp: event.smtp_port,
                    }));
                }
                "access_limited" => shared.publish_access_limited(
                    ProtonAccessLimited {
                        username: event.username,
                        message: event.message,
                        appeal_url: event.appeal_url,
                    },
                    generation,
                ),
                _ => {}
            }
        }
        let _ = sender.send(ReaderEvent::Stopped);
    });
    receiver
}

struct StderrCapture {
    text: Arc<Mutex<String>>,
    done: Receiver<()>,
}

impl StderrCapture {
    fn spawn(stderr: ChildStderr) -> Self {
        let text = Arc::new(Mutex::new(String::new()));
        let target = Arc::clone(&text);
        let (sender, done) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else {
                    break;
                };
                let mut buffer = lock(&target);
                if buffer.len() < 4096 {
                    buffer.push_str(&line);
                    buffer.push('\n');
                }
            }
            let _ = sender.send(());
        });
        Self { text, done }
    }

    fn wait_text(self) -> String {
        let _ = self.done.recv_timeout(Duration::from_secs(1));
        lock(&self.text).clone()
    }
}

fn start_failure(stderr: &str) -> AppError {
    let stderr = stderr.to_ascii_lowercase();
    if stderr.contains("another bridge process")
        || stderr.contains("could not lock")
        || stderr.contains("bridge.lock")
    {
        AppError::Message("Another AuraMail instance is already running the Proton bridge.".into())
    } else {
        AppError::Message("Proton bridge failed to start.".into())
    }
}

fn auth_lock_held(config_dir: &Path) -> AppResult<bool> {
    let dir = config_dir.join("hydroxide");
    ensure_private_dir(&dir)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.join("auth.lock"))?;
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            Ok(false)
        }
        Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(AppError::Io(error)),
    }
}

fn remove_auth_entry(path: &Path, username: &str) -> AppResult<()> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::Io(error)),
    };
    let mut entries: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&contents)
            .map_err(|_| AppError::Message("could not read the Proton account store".into()))?;
    if entries.remove(username).is_none() {
        return Ok(());
    }
    let data = serde_json::to_vec(&entries)?;
    let temporary = path.with_extension(format!("json.tmp.{}", std::process::id()));
    fs::write(&temporary, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

fn ensure_private_dir(dir: &Path) -> AppResult<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn wait_for_exit(child: &mut Child) {
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn sidecar_path() -> AppResult<PathBuf> {
    if let Some(value) = std::env::var_os("AURAMAIL_PROTON_BRIDGE") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    let name = if cfg!(windows) {
        "auramail-proton-bridge.exe"
    } else {
        SIDECAR_NAME
    };
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let sibling = directory.join(name);
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }
    let development = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proton-bridge")
        .join("dist")
        .join(name);
    if development.is_file() {
        return Ok(development);
    }
    Err(AppError::Message(
        "Proton support is unavailable in this build.".into(),
    ))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn clear_stopped_child(state: &mut State) {
    state.child = None;
    state.ports = None;
    state.access_limited = None;
    state.emitted.clear();
    state.generation = state.generation.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use fs2::FileExt;
    use tempfile::TempDir;

    use super::*;
    use crate::accounts::{self, AccountAuth, ProtonAccountInput, SignatureFields};
    use crate::db::Database;
    use crate::proton::ports::{self, BridgePorts};
    use crate::secrets::{MemoryStore, SecretStore};

    #[derive(Default)]
    struct FakeEvents {
        statuses: Mutex<Vec<ProtonAccessLimited>>,
    }

    impl ProtonEvents for FakeEvents {
        fn access_limited(&self, status: ProtonAccessLimited) {
            self.statuses.lock().unwrap().push(status);
        }
    }

    impl FakeEvents {
        fn statuses(&self) -> Vec<ProtonAccessLimited> {
            self.statuses.lock().unwrap().clone()
        }

        fn wait_for(&self, count: usize) -> Vec<ProtonAccessLimited> {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let statuses = self.statuses();
                if statuses.len() >= count || Instant::now() >= deadline {
                    return statuses;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    struct Fixture {
        dir: TempDir,
        db: Arc<Database>,
        secrets: Arc<dyn SecretStore>,
        events: Arc<FakeEvents>,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let db = Arc::new(Database::open_memory().unwrap());
            let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
            let events = Arc::new(FakeEvents::default());
            Self {
                dir,
                db,
                secrets,
                events,
            }
        }

        fn config_dir(&self) -> PathBuf {
            self.dir.path().join("proton-bridge")
        }

        fn bridge(&self) -> ProtonBridge {
            self.bridge_with_sidecar(fake_sidecar())
        }

        fn bridge_with_sidecar(&self, sidecar: PathBuf) -> ProtonBridge {
            ProtonBridge::new(
                Arc::clone(&self.events) as Arc<dyn ProtonEvents>,
                Arc::clone(&self.db),
                Arc::clone(&self.secrets),
                self.config_dir(),
                sidecar,
            )
        }

        fn add_proton(&self, email: &str, imap_port: u16, smtp_port: u16) {
            accounts::create_proton(
                &self.db,
                self.secrets.as_ref(),
                ProtonAccountInput {
                    display_name: "Proton".into(),
                    email: email.into(),
                    username: email.into(),
                    bridge_password: "bridge-password".into(),
                    imap_host: "127.0.0.1".into(),
                    imap_port,
                    smtp_host: "127.0.0.1".into(),
                    smtp_port,
                    color: "lavender".into(),
                    signature: String::new(),
                    signature_fields: SignatureFields::default(),
                    space_id: 1,
                },
            )
            .unwrap();
        }

        fn generation(&self) -> Option<String> {
            fs::read_to_string(self.config_dir().join("sidecar-generation")).ok()
        }
    }

    fn fake_sidecar() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".local-tests/proton-bridge/fake-sidecar.sh")
    }

    fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(path.exists(), "expected {}", path.display());
    }

    #[test]
    fn no_proton_accounts_is_a_noop() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        assert_eq!(bridge.start_if_needed().unwrap(), None);
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
    }

    #[test]
    fn start_publishes_ports_and_is_idempotent() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let bridge = fixture.bridge();

        let ports = bridge.start_if_needed().unwrap().unwrap();
        assert_ne!(ports.imap, 0);
        assert_ne!(ports.smtp, 0);
        assert_ne!(ports.imap, ports.smtp);
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        assert_eq!(bridge.start_if_needed().unwrap(), Some(ports));
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        bridge.stop();
        assert!(!bridge.is_running());
    }

    #[test]
    fn concurrent_start_spawns_only_one_child() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let bridge = Arc::new(fixture.bridge());

        let handles = (0..4)
            .map(|_| {
                let bridge = Arc::clone(&bridge);
                thread::spawn(move || bridge.start_if_needed().unwrap().unwrap())
            })
            .collect::<Vec<_>>();
        let ports = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert!(ports.iter().all(|candidate| *candidate == ports[0]));
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        bridge.stop();
    }

    #[test]
    fn reuses_persisted_ports_when_they_are_free() {
        let fixture = Fixture::new();
        let persisted = ports::free_pair().unwrap();
        fixture.add_proton("me@proton.me", persisted.imap, persisted.smtp);
        let bridge = fixture.bridge();

        assert_eq!(
            bridge.start_if_needed().unwrap(),
            Some(BridgePorts {
                imap: persisted.imap,
                smtp: persisted.smtp,
            })
        );
        bridge.stop();
    }

    #[test]
    fn updates_every_proton_row_to_the_chosen_ports() {
        let fixture = Fixture::new();
        let first = ports::free_pair().unwrap();
        let second = ports::free_pair().unwrap();
        fixture.add_proton("one@proton.me", first.imap, first.smtp);
        fixture.add_proton("two@proton.me", second.imap, second.smtp);
        let bridge = fixture.bridge();

        let chosen = bridge.start_if_needed().unwrap().unwrap();
        let proton_accounts = accounts::list(&fixture.db)
            .unwrap()
            .into_iter()
            .filter(|account| account.auth_type == AccountAuth::Proton)
            .collect::<Vec<_>>();

        assert_eq!(proton_accounts.len(), 2);
        for account in proton_accounts {
            assert_eq!(account.imap_port, chosen.imap);
            assert_eq!(account.smtp_port, chosen.smtp);
        }
        bridge.stop();
    }

    #[test]
    fn auth_lock_blocks_start_while_signing_in() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let hydroxide = fixture.config_dir().join("hydroxide");
        fs::create_dir_all(&hydroxide).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(hydroxide.join("auth.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();

        let bridge = fixture.bridge();
        let error = bridge.start_if_needed().unwrap_err();

        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
        FileExt::unlock(&lock).unwrap();
    }

    #[test]
    fn reports_sidecar_lock_contention_without_leaving_a_child() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let hydroxide = fixture.config_dir().join("hydroxide");
        fs::create_dir_all(&hydroxide).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(hydroxide.join("bridge.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();

        let bridge = fixture.bridge();
        let error = bridge.start_if_needed().unwrap_err();

        assert!(
            error
                .to_string()
                .contains("already running the Proton bridge"),
            "{error}"
        );
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
        FileExt::unlock(&lock).unwrap();
    }

    #[test]
    fn stop_sends_shutdown_and_clears_the_handle() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();

        bridge.stop();

        assert!(!bridge.is_running());
        wait_for_file(&fixture.config_dir().join("stopped"));
    }

    #[test]
    fn restart_runs_a_new_sidecar_generation() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        bridge.restart().unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("2"));
        bridge.stop();
    }

    #[test]
    fn access_limited_is_stored_and_emitted_once() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let config_dir = fixture.config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("access-limited"), b"").unwrap();
        let bridge = fixture.bridge();

        bridge.start_if_needed().unwrap().unwrap();
        fixture.events.wait_for(1);
        thread::sleep(Duration::from_millis(300));
        let statuses = fixture.events.statuses();

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].username, "me@proton.me");
        assert!(statuses[0].message.contains("limited"));
        assert_eq!(
            statuses[0].appeal_url,
            "https://proton.me/support/appeal-abuse"
        );
        assert_eq!(bridge.access_limited(), Some(statuses[0].clone()));
        bridge.stop();
    }

    #[test]
    fn missing_sidecar_reports_an_unavailable_build() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let bridge = fixture.bridge_with_sidecar(fixture.dir.path().join("missing-sidecar"));

        let error = bridge.start_if_needed().unwrap_err();

        assert_eq!(
            error.to_string(),
            "Proton support is unavailable in this build."
        );
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
    }

    #[test]
    fn sidecar_exit_before_ready_reports_a_clear_error() {
        let fixture = Fixture::new();
        fixture.add_proton("me@proton.me", 1143, 1025);
        let config_dir = fixture.config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("fail-start"), b"").unwrap();
        let bridge = fixture.bridge();

        let error = bridge.start_if_needed().unwrap_err();

        assert_eq!(error.to_string(), "Proton bridge failed to start.");
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
    }

    #[test]
    fn forget_account_removes_only_the_named_auth_entry() {
        let fixture = Fixture::new();
        let hydroxide = fixture.config_dir().join("hydroxide");
        fs::create_dir_all(&hydroxide).unwrap();
        fs::write(
            hydroxide.join("auth.json"),
            r#"{"me@proton.me":"first","other@proton.me":"second"}"#,
        )
        .unwrap();
        let bridge = fixture.bridge();

        bridge.forget_account("me@proton.me").unwrap();

        let stored: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(hydroxide.join("auth.json")).unwrap())
                .unwrap();
        assert_eq!(stored.get("me@proton.me"), None);
        assert_eq!(stored["other@proton.me"], "second");
    }
}
