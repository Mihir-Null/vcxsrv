//! One serialized lifecycle worker, bounded typed ingress and secret-free snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostChange {
    None,
    ConnectPassively,
    InvalidateGuest,
}
pub struct HostPoller {
    next: std::time::Instant,
    delay: Duration,
    last: Option<bool>,
}
impl HostPoller {
    pub fn new_at(now: std::time::Instant) -> Self {
        Self {
            next: now + Duration::from_secs(5),
            delay: Duration::from_secs(5),
            last: None,
        }
    }
    pub fn due_at(&self, now: std::time::Instant) -> bool {
        now >= self.next
    }
    pub fn observe_at(
        &mut self,
        running: bool,
        state: &State,
        now: std::time::Instant,
    ) -> HostChange {
        self.delay = if self.last == Some(running) {
            (self.delay * 2).min(Duration::from_secs(30))
        } else {
            Duration::from_secs(5)
        };
        self.last = Some(running);
        self.next = now + self.delay;
        match (state, running) {
            (State::WaitingForWsl, true) => HostChange::ConnectPassively,
            (State::Ready, false) => HostChange::InvalidateGuest,
            _ => HostChange::None,
        }
    }
    fn defer_at(&mut self, now: std::time::Instant) {
        self.delay = (self.delay * 2).min(Duration::from_secs(30));
        self.next = now + self.delay;
    }
}

use crate::{
    authority::{Authority, AuthorityStore},
    cli::Options,
    config::Config,
    controller::{Action, Controller, Effect, Event, Invalidation, Operation, Outcome, State},
    diagnostics::Failure,
    helper::{HelperClient, HelperError},
    instance::InstanceGuard,
    owner::{ServerBinding, ServerOwner},
    process::{ProcessError, ProcessRunner},
    storage::{ProtectedDirectory, StorageError},
    wsl::{decode_running_distros, WslClient},
};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Debug, Clone, Copy)]
pub enum Request {
    Action(Action),
    Exit,
}
pub struct ActionQueue {
    sender: SyncSender<Request>,
    busy: AtomicBool,
    closing: AtomicBool,
    cancel: Arc<AtomicBool>,
}
impl ActionQueue {
    pub fn new() -> (Self, Receiver<Request>) {
        let (sender, receiver) = mpsc::sync_channel(1);
        (
            Self {
                sender,
                busy: AtomicBool::new(false),
                closing: AtomicBool::new(false),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            receiver,
        )
    }
    pub fn submit(&self, action: Action) -> bool {
        if self.closing.load(Ordering::Acquire) || self.busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        if self.sender.try_send(Request::Action(action)).is_err() {
            self.busy.store(false, Ordering::Release);
            return false;
        }
        true
    }
    pub fn begin_observation(&self) -> bool {
        !self.closing.load(Ordering::Acquire)
            && self
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
    pub fn complete(&self) {
        self.busy.store(false, Ordering::Release);
    }
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
    pub fn cancel(&self) -> &AtomicBool {
        &self.cancel
    }
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.cancel.store(true, Ordering::Release);
        let _ = self.sender.try_send(Request::Exit);
    }
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub version: u32,
    pub state: String,
    pub ready: bool,
    pub busy: bool,
    pub companion_present: bool,
    pub ownership_present: bool,
    pub authority_valid: bool,
    pub error: Option<String>,
    pub last_authenticated_state: Option<bool>,
}
impl Snapshot {
    fn from_controller(controller: &Controller) -> Self {
        let (state, error) = match controller.state() {
            State::WaitingForWsl => ("WaitingForWsl", None),
            State::Connecting => ("Connecting", None),
            State::Ready => ("Ready", None),
            State::Error(e) => ("Error", Some(e.clone())),
        };
        Self {
            version: 1,
            state: state.into(),
            ready: matches!(controller.state(), State::Ready),
            busy: false,
            companion_present: false,
            ownership_present: false,
            authority_valid: false,
            error,
            last_authenticated_state: None,
        }
    }
    pub fn status(&self) -> String {
        self.error.clone().unwrap_or_else(|| {
            if self.ready {
                "Ready — last authenticated connection".into()
            } else {
                self.state.clone()
            }
        })
    }
}
fn config_for(
    options: &Options,
    state: Option<&ProtectedDirectory>,
    create: bool,
) -> Result<Config, &'static str> {
    let exe = std::env::current_exe().map_err(|_| "Manager executable path unavailable")?;
    let defaults = Config::defaults_for(&exe).map_err(|_| "Invalid companion location")?;
    let path = options.config_path()?;
    let parent = path.parent().ok_or("Invalid config path")?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid config filename")?;
    let external;
    let directory = match state {
        Some(d) if d.path() == parent => d,
        _ => {
            external=match ProtectedDirectory::inspect(parent) {Ok(d)=>d,Err(StorageError::Missing) if !create && options.config.is_none()=>return Ok(defaults),Err(_)=>return Err("Config directory must have current-user-only protected security and no reparse points")};
            &external
        }
    };
    match directory.read(name, 65536) {
        Ok(file) => Config::from_json(
            std::str::from_utf8(file.bytes()).map_err(|_| "Config must be UTF-8")?,
            &exe,
        )
        .map_err(|_| {
            "Invalid config.json; verify schema, distro, user, display and absolute companion path"
        }),
        Err(StorageError::Missing) if create => {
            let bytes =
                serde_json::to_vec_pretty(&defaults).map_err(|_| "Config serialization failed")?;
            drop(
                directory
                    .create_new(name, &bytes)
                    .map_err(|_| "Could not create private config.json")?,
            );
            Ok(defaults)
        }
        Err(StorageError::Missing) if options.config.is_none() => Ok(defaults),
        Err(_) => Err("Config failed private-file validation; existing ACLs are never repaired"),
    }
}
pub fn doctor(options: &Options) -> Result<Snapshot, &'static str> {
    let controller = Controller::new();
    let mut snapshot = Snapshot::from_controller(&controller);
    let path = options.state_path()?;
    let directory = match ProtectedDirectory::inspect(&path) {
        Ok(d) => Some(d),
        Err(StorageError::Missing) => None,
        Err(_) => return Err("State directory failed private-storage validation"),
    };
    let config = config_for(options, directory.as_ref(), false)?;
    snapshot.companion_present = config.server_path.is_file();
    if let Some(d) = directory {
        snapshot.ownership_present = match d.read("ownership.json", 16384) {
            Ok(_) => true,
            Err(StorageError::Missing) => false,
            Err(_) => return Err("Ownership metadata failed validation"),
        };
        snapshot.authority_valid = AuthorityStore::new(&d).load(config.display).is_ok();
    }
    if !snapshot.companion_present {
        snapshot.error = Some(Failure::MissingServer.message().into());
    }
    // Read-only inspection cannot establish a live authenticated state.
    Ok(snapshot)
}
pub fn recover_stopped(options: &Options) -> Result<crate::recovery::RecoveryResult, &'static str> {
    let guard = Arc::new(
        InstanceGuard::acquire("manager")
            .map_err(|_| "Close the running manager before using recover-stopped")?,
    );
    let directory = match ProtectedDirectory::open_existing(&options.state_path()?, guard) {
        Ok(d) => d,
        Err(StorageError::Missing) => return Ok(crate::recovery::RecoveryResult::NoRecord),
        Err(_) => return Err("State directory failed private-storage validation"),
    };
    let config = config_for(options, Some(&directory), false)?;
    crate::recovery::retire_stopped_record(
        &directory,
        &config,
        std::time::Instant::now() + Duration::from_secs(15),
    )
    .map_err(recovery_message)
}
fn recovery_message(error: crate::recovery::RecoveryError) -> &'static str {
    use crate::recovery::RecoveryError;
    match error {RecoveryError::StillAlive=>"Recorded server is still alive; recover it normally or use separately confirmed Stop server",RecoveryError::Authority=>"Existing authority is missing, unsafe, replaced or malformed; recovery refused",RecoveryError::Deadline=>"Stopped-server recovery timed out; metadata preserved",_=>"Stopped-server scope or process evidence could not be verified; metadata preserved"}
}
struct Runtime {
    config: Config,
    directory: ProtectedDirectory,
    authority: Option<Authority>,
    owner: Option<ServerOwner>,
    helper: HelperClient,
    controller: Controller,
}
impl Runtime {
    fn new(options: &Options) -> Result<Self, &'static str> {
        let guard = Arc::new(InstanceGuard::acquire("manager").map_err(|_| {
            "Another manager is running, or the per-user instance object is unsafe"
        })?);
        let directory=ProtectedDirectory::open_or_create(&options.state_path()?,guard).map_err(|_|"State directory must have current-user-only protected security and no reparse points; use a fresh --state-dir")?;
        let config = config_for(options, Some(&directory), true)?;
        let helper = HelperClient::new(config.clone())
            .map_err(|_| "Trusted Windows WSL executable unavailable")?;
        Ok(Self {
            config,
            directory,
            authority: None,
            owner: None,
            helper,
            controller: Controller::new(),
        })
    }
    fn running(&self, effect: &Effect, cancel: &AtomicBool) -> Result<bool, Failure> {
        let wsl = WslClient::new(ProcessRunner::default()).map_err(|_| Failure::MissingHelper)?;
        let output = ProcessRunner::default()
            .run_cancellable(&wsl.running_spec(effect.remaining()?), None, cancel)
            .map_err(command_failure)?;
        let names = decode_running_distros(&output.stdout).map_err(|_| Failure::Protocol)?;
        Ok(names.iter().any(|name| name == &self.config.distro))
    }
    fn ensure(&mut self) -> Result<Outcome, Failure> {
        if let Some(owner) = &self.owner {
            return if owner.is_alive().map_err(|_| Failure::OwnershipMismatch)? {
                Ok(Outcome::ServerOwned)
            } else {
                Err(Failure::OwnershipMismatch)
            };
        }
        if !self.config.server_path.is_file() {
            return Err(Failure::MissingServer);
        }
        let authority = AuthorityStore::new(&self.directory)
            .load_or_create(self.config.display)
            .map_err(|_| Failure::UnsafeStorage)?;
        let binding = ServerBinding::new(
            &self.directory,
            self.config.server_path.clone(),
            self.config.distro.clone(),
            self.config.user.clone(),
            self.config.display,
            "authority",
        )
        .map_err(|_| Failure::OwnershipMismatch)?;
        let owner = match ServerOwner::recover(&self.directory, &binding)
            .map_err(|_| Failure::OwnershipMismatch)?
        {
            Some(owner) => owner,
            None => ServerOwner::spawn(
                &self.directory,
                &binding,
                &[
                    format!(":{}", self.config.display).into(),
                    "-wslmanaged".into(),
                    "-auth".into(),
                    self.directory.path().join("authority").into_os_string(),
                    "-wsldistro".into(),
                    self.config.distro.clone().into(),
                ],
            )
            .map_err(|_| Failure::ServerStart)?,
        };
        self.authority = Some(authority);
        self.owner = Some(owner);
        Ok(Outcome::ServerOwned)
    }
    fn execute(&mut self, effect: Effect, cancel: &AtomicBool) -> Result<Outcome, Failure> {
        effect.remaining()?;
        if cancel.load(Ordering::Acquire) {
            return Err(Failure::Deadline);
        }
        let outcome = match effect.operation {
            Operation::EnsureServer => self.ensure()?,
            Operation::DiscoverRunning => Outcome::Running(self.running(&effect, cancel)?),
            Operation::Bootstrap { allow_start } => {
                if !allow_start && !self.running(&effect, cancel)? {
                    return Err(Failure::GuestStopped);
                }
                let mut bytes = self
                    .authority
                    .as_ref()
                    .ok_or(Failure::UnsafeStorage)?
                    .encode_xauthority();
                let result =
                    self.helper
                        .invoke("bootstrap", effect.remaining()?, Some(&bytes), cancel);
                bytes.fill(0);
                result.map_err(helper_failure)?;
                Outcome::Bootstrapped
            }
            Operation::Probe => {
                if !self.running(&effect, cancel)? {
                    return Err(Failure::GuestStopped);
                }
                match self
                    .helper
                    .invoke("probe", effect.remaining()?, None, cancel)
                {
                    Ok(_) => Outcome::Authenticated,
                    Err(HelperError::Command(ProcessError::Nonzero(7))) => {
                        let delay = effect.remaining()?.min(Duration::from_millis(100));
                        std::thread::sleep(delay);
                        Outcome::Retryable
                    }
                    Err(error) => return Err(helper_failure(error)),
                }
            }
            Operation::Launch => {
                self.helper
                    .invoke("launch", effect.remaining()?, None, cancel)
                    .map_err(helper_failure)?;
                Outcome::Launched
            }
            Operation::StopOwnedServer => {
                if let Some(owner) = &mut self.owner {
                    owner
                        .stop_confirmed(&self.directory, effect.deadline)
                        .map_err(|_| Failure::OwnershipMismatch)?;
                }
                self.owner = None;
                self.authority = None;
                Outcome::Stopped
            }
            Operation::RecoverStoppedRecord => {
                if self
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.is_alive().unwrap_or(true))
                {
                    return Err(Failure::OwnershipMismatch);
                }
                self.owner = None;
                self.authority = None;
                crate::recovery::retire_stopped_record(
                    &self.directory,
                    &self.config,
                    effect.deadline,
                )
                .map_err(|_| Failure::RecoveryRefused)?;
                Outcome::Recovered
            }
            Operation::Detach => {
                self.owner = None;
                self.authority = None;
                Outcome::Detached
            }
        };
        effect.remaining()?;
        Ok(outcome)
    }
    fn drive(&mut self, action: Action, cancel: &AtomicBool, publish: &impl Fn(&Runtime, bool)) {
        let mut effects = self.controller.dispatch(action);
        publish(self, true);
        while let Some(effect) = effects.pop() {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let outcome = self.execute(effect, cancel).unwrap_or_else(Outcome::Failed);
            effects = self.controller.observe(Event {
                attempt: effect.attempt,
                outcome,
            });
            publish(self, !effects.is_empty());
        }
    }
    fn snapshot(&self, busy: bool) -> Snapshot {
        let mut s = Snapshot::from_controller(&self.controller);
        s.busy = busy;
        s.companion_present = self.config.server_path.is_file();
        s.authority_valid = self.authority.is_some();
        s.ownership_present = self.owner.is_some();
        s.last_authenticated_state = Some(s.ready);
        s
    }
}
fn command_failure(error: ProcessError) -> Failure {
    match error {
        ProcessError::Timeout | ProcessError::Cancelled => Failure::Deadline,
        _ => Failure::Command,
    }
}
fn helper_failure(error: HelperError) -> Failure {
    match error {
        HelperError::Protocol => Failure::Protocol,
        HelperError::CredentialConflict => Failure::CredentialConflict,
        HelperError::Deadline => Failure::Deadline,
        _ => Failure::MissingHelper,
    }
}
pub struct Worker {
    pub queue: Arc<ActionQueue>,
    pub latest: Arc<Mutex<Snapshot>>,
}
impl Worker {
    pub fn start(
        options: Options,
        notify: impl Fn() + Send + 'static,
    ) -> Result<Self, &'static str> {
        let runtime = Runtime::new(&options)?;
        let (queue, receiver) = ActionQueue::new();
        let queue = Arc::new(queue);
        let latest = Arc::new(Mutex::new(runtime.snapshot(false)));
        let worker_queue = queue.clone();
        let worker_latest = latest.clone();
        std::thread::Builder::new()
            .name("manager-lifecycle".into())
            .spawn(move || {
                let mut runtime = runtime;
                let mut poller = HostPoller::new_at(std::time::Instant::now());
                let publish = |runtime: &Runtime, busy: bool| {
                    let snapshot = runtime.snapshot(busy);
                    if let Ok(bytes) = serde_json::to_vec(&snapshot) {
                        let _ = runtime.directory.replace("manager.log", &bytes);
                    }
                    *worker_latest.lock().unwrap() = snapshot;
                    notify();
                };
                loop {
                    match receiver.recv_timeout(Duration::from_secs(1)) {
                        Ok(Request::Exit) => break,
                        Ok(Request::Action(action)) => {
                            runtime.drive(action, worker_queue.cancel(), &publish);
                            worker_queue.complete();
                            publish(&runtime, false);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let observable = matches!(
                                runtime.controller.state(),
                                State::WaitingForWsl | State::Ready
                            );
                            if observable
                                && runtime
                                    .owner
                                    .as_ref()
                                    .is_some_and(|o| !o.is_alive().unwrap_or(false))
                            {
                                runtime.controller.invalidate(Invalidation::ServerChanged);
                                publish(&runtime, false);
                            } else if observable
                                && runtime.owner.is_some()
                                && poller.due_at(std::time::Instant::now())
                                && worker_queue.begin_observation()
                            {
                                publish(&runtime, true);
                                let effect = Effect {
                                    attempt: 0,
                                    deadline: std::time::Instant::now() + Duration::from_secs(5),
                                    operation: Operation::DiscoverRunning,
                                };
                                match runtime.running(&effect, worker_queue.cancel()) {
                                    Ok(running) => match poller.observe_at(
                                        running,
                                        runtime.controller.state(),
                                        std::time::Instant::now(),
                                    ) {
                                        HostChange::ConnectPassively => runtime.drive(
                                            Action::StartServer,
                                            worker_queue.cancel(),
                                            &publish,
                                        ),
                                        HostChange::InvalidateGuest => {
                                            runtime.controller.invalidate(Invalidation::VmChanged)
                                        }
                                        HostChange::None => {}
                                    },
                                    Err(_) => poller.defer_at(std::time::Instant::now()),
                                }
                                worker_queue.complete();
                                publish(&runtime, false);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if worker_queue.cancel().load(Ordering::Acquire) {
                        break;
                    }
                }
                runtime.owner = None;
                runtime.authority = None;
            })
            .map_err(|_| "Could not start lifecycle worker")?;
        Ok(Self { queue, latest })
    }
}
