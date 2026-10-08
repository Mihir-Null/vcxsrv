//! Pure orchestration. The worker supplies observations; only a validated positive relay
//! handshake may become Authenticated. Effects contain no credentials or arbitrary output.
use crate::diagnostics::Failure;
use std::time::{Duration, Instant};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    WaitingForWsl,
    Connecting,
    Ready,
    Error(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    StartServer,
    Launch,
    Refresh,
    Detach,
    StopServerConfirmed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    EnsureServer,
    DiscoverRunning,
    Bootstrap { allow_start: bool },
    Probe,
    Launch,
    Detach,
    StopOwnedServer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub attempt: u64,
    pub deadline: Instant,
    pub operation: Operation,
}
impl Effect {
    pub fn remaining_at(&self, now: Instant) -> Result<Duration, Failure> {
        self.deadline
            .checked_duration_since(now)
            .filter(|d| !d.is_zero())
            .ok_or(Failure::Deadline)
    }
    pub fn remaining(&self) -> Result<Duration, Failure> {
        self.remaining_at(Instant::now())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    ServerOwned,
    Running(bool),
    Bootstrapped,
    Authenticated,
    Launched,
    Stopped,
    Detached,
    Retryable,
    Failed(Failure),
}
#[derive(Debug, Clone, Copy)]
pub struct Event {
    pub attempt: u64,
    pub outcome: Outcome,
}
#[derive(Debug, Clone, Copy)]
pub enum Invalidation {
    ServerChanged,
    VmChanged,
}
pub struct Controller {
    state: State,
    generation: u64,
    active: Option<Effect>,
    action: Action,
    detached: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
impl Controller {
    pub fn new() -> Self {
        Self {
            state: State::WaitingForWsl,
            generation: 0,
            active: None,
            action: Action::StartServer,
            detached: false,
        }
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        self.dispatch_at(action, Instant::now())
    }
    pub fn dispatch_at(&mut self, action: Action, now: Instant) -> Vec<Effect> {
        if self.detached {
            return vec![];
        }
        if self.active.is_some() && self.action == action {
            return vec![];
        }
        if self.active.is_some() && !matches!(action, Action::Detach | Action::StopServerConfirmed)
        {
            return vec![];
        }
        self.generation += 1;
        self.action = action;
        let operation = match action {
            Action::Detach => {
                self.detached = true;
                Operation::Detach
            }
            Action::StopServerConfirmed => Operation::StopOwnedServer,
            _ => Operation::EnsureServer,
        };
        self.state = if matches!(action, Action::Detach) {
            State::WaitingForWsl
        } else {
            State::Connecting
        };
        self.active = Some(Effect {
            attempt: self.generation,
            deadline: now + Duration::from_secs(15),
            operation,
        });
        vec![self.active.unwrap()]
    }
    pub fn invalidate(&mut self, reason: Invalidation) {
        self.generation += 1;
        self.active = None;
        self.state = match reason {
            Invalidation::VmChanged => State::WaitingForWsl,
            Invalidation::ServerChanged => State::Error(Failure::ServerExited.message().into()),
        };
    }
    pub fn observe(&mut self, event: Event) -> Vec<Effect> {
        self.observe_at(event, Instant::now())
    }
    pub fn observe_at(&mut self, event: Event, now: Instant) -> Vec<Effect> {
        let Some(e) = self.active else { return vec![] };
        if e.attempt != event.attempt {
            return vec![];
        }
        if e.remaining_at(now).is_err() {
            self.fail(Failure::Deadline);
            return vec![];
        }
        let next = match (e.operation, event.outcome) {
            (_, Outcome::Failed(f)) => {
                self.fail(f);
                None
            }
            (Operation::EnsureServer, Outcome::ServerOwned) => Some(Operation::DiscoverRunning),
            (Operation::DiscoverRunning, Outcome::Running(false))
                if self.action != Action::Launch =>
            {
                self.state = State::WaitingForWsl;
                self.active = None;
                None
            }
            (Operation::DiscoverRunning, Outcome::Running(_)) => {
                Some(if self.action == Action::Refresh {
                    Operation::Probe
                } else {
                    Operation::Bootstrap {
                        allow_start: self.action == Action::Launch,
                    }
                })
            }
            (Operation::Bootstrap { .. }, Outcome::Bootstrapped) => Some(Operation::Probe),
            (Operation::Probe, Outcome::Retryable) => Some(Operation::Probe),
            (Operation::Probe, Outcome::Authenticated) => {
                self.state = State::Ready;
                if self.action == Action::Launch {
                    Some(Operation::Launch)
                } else {
                    self.active = None;
                    None
                }
            }
            (Operation::Launch, Outcome::Launched) => {
                self.active = None;
                None
            }
            (Operation::StopOwnedServer, Outcome::Stopped)
            | (Operation::Detach, Outcome::Detached) => {
                self.active = None;
                self.state = State::WaitingForWsl;
                None
            }
            _ => None,
        };
        if let Some(operation) = next {
            let e = Effect { operation, ..e };
            self.active = Some(e);
            vec![e]
        } else {
            vec![]
        }
    }
    fn fail(&mut self, f: Failure) {
        self.active = None;
        self.state = State::Error(f.message().into())
    }
}
