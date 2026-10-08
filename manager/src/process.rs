use std::{ffi::OsString, fmt, path::PathBuf, time::Duration};
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub timeout: Duration,
}
impl fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandSpec")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}
/// Captured bytes are untrusted and can contain secrets. Debug deliberately omits them.
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: u32,
}
impl fmt::Debug for CommandOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandOutput")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    InvalidCommand,
    Timeout,
    OutputLimit,
    InputLimit,
    Nonzero(u32),
    Native(u32),
    InputClosed,
    Cancelled,
}
impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "command failed: {self:?}")
    }
}
impl std::error::Error for ProcessError {}
/// Disposable host commands only. Never use this job-owning runner for the persistent server.
pub struct ProcessRunner {
    output_limit: usize,
}
impl Default for ProcessRunner {
    fn default() -> Self {
        Self::new(1024 * 1024)
    }
}
impl ProcessRunner {
    pub fn new(output_limit: usize) -> Self {
        Self { output_limit }
    }
    pub fn run(
        &self,
        spec: &CommandSpec,
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, ProcessError> {
        native::run(spec, stdin.unwrap_or_default(), self.output_limit, None)
    }
}
impl ProcessRunner {
    pub fn run_cancellable(
        &self,
        spec: &CommandSpec,
        stdin: Option<&[u8]>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<CommandOutput, ProcessError> {
        native::run(
            spec,
            stdin.unwrap_or_default(),
            self.output_limit,
            Some(cancel),
        )
    }
}
#[cfg(not(windows))]
compile_error!("The manager currently supports native Windows only");
#[cfg(windows)]
pub(crate) mod native {
    use super::*;
    use std::{
        ffi::{c_void, OsStr},
        os::windows::ffi::OsStrExt,
        ptr::{null, null_mut},
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{JobObjects::*, Pipes::*, Threading::*, IO::*},
    };
    struct Handle(HANDLE);
    // Owned Win32 handles may move across threads; pending I/O's storage moves with them.
    unsafe impl Send for Handle {}
    impl Handle {
        fn new(h: HANDLE) -> Result<Self, ProcessError> {
            if h.is_null() || h == INVALID_HANDLE_VALUE {
                Err(last())
            } else {
                Ok(Self(h))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn last() -> ProcessError {
        ProcessError::Native(unsafe { GetLastError() })
    }
    fn wide(s: &OsStr) -> Result<Vec<u16>, ProcessError> {
        let mut v: Vec<u16> = s.encode_wide().collect();
        if v.contains(&0) {
            return Err(ProcessError::InvalidCommand);
        }
        v.push(0);
        Ok(v)
    }
    /// Standard minimal Windows quoting. Bare switches are required by installed WSL.
    pub(crate) fn argument(s: &OsStr, out: &mut Vec<u16>) -> Result<(), ProcessError> {
        let s: Vec<u16> = s.encode_wide().collect();
        if s.contains(&0) {
            return Err(ProcessError::InvalidCommand);
        }
        if !s.is_empty()
            && !s
                .iter()
                .any(|c| matches!(*c, 9 | 10 | 11 | 12 | 13 | 32 | 34))
        {
            out.extend(s);
            return Ok(());
        }
        out.push(34);
        let mut slashes = 0;
        for c in s {
            if c == 92 {
                slashes += 1;
                continue;
            }
            if c == 34 {
                out.extend(std::iter::repeat_n(92, slashes * 2 + 1));
            } else {
                out.extend(std::iter::repeat_n(92, slashes));
            }
            out.push(c);
            slashes = 0;
        }
        out.extend(std::iter::repeat_n(92, slashes * 2));
        out.push(34);
        Ok(())
    }
    struct Attributes {
        storage: Vec<usize>,
        initialized: bool,
    }
    impl Attributes {
        fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
            self.storage.as_mut_ptr().cast()
        }
        fn new(handles: &[HANDLE]) -> Result<Self, ProcessError> {
            unsafe {
                let mut size = 0;
                InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
                let mut a = Self {
                    storage: vec![0; size.div_ceil(std::mem::size_of::<usize>())],
                    initialized: false,
                };
                if InitializeProcThreadAttributeList(a.pointer(), 1, 0, &mut size) == 0 {
                    return Err(last());
                }
                a.initialized = true;
                if UpdateProcThreadAttribute(
                    a.pointer(),
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_ptr().cast::<c_void>(),
                    std::mem::size_of_val(handles),
                    null_mut(),
                    null(),
                ) == 0
                {
                    return Err(last());
                }
                Ok(a)
            }
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            if self.initialized {
                unsafe {
                    DeleteProcThreadAttributeList(self.pointer());
                }
            }
        }
    }
    /// Pipe ACLs protect transient stdin/output, independently of persistent storage.
    struct PipeSecurity(PSECURITY_DESCRIPTOR);
    impl Drop for PipeSecurity {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    impl PipeSecurity {
        fn current_user() -> Result<Self, ProcessError> {
            unsafe {
                let mut token = null_mut();
                if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                    return Err(last());
                }
                let token = Handle::new(token)?;
                let mut n = 0;
                GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut n);
                if n == 0 {
                    return Err(last());
                }
                let mut data = vec![0usize; (n as usize).div_ceil(std::mem::size_of::<usize>())];
                if GetTokenInformation(token.0, TokenUser, data.as_mut_ptr().cast(), n, &mut n) == 0
                {
                    return Err(last());
                }
                let sid = (*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid;
                let mut string = null_mut();
                if ConvertSidToStringSidW(sid, &mut string) == 0 {
                    return Err(last());
                }
                let mut len = 0;
                while *string.add(len) != 0 {
                    len += 1;
                }
                let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
                LocalFree(string.cast());
                let sddl = wide(OsStr::new(&format!("D:P(A;;GA;;;{sid_text})")))?;
                let mut sd = null_mut();
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd,
                    null_mut(),
                ) == 0
                {
                    return Err(last());
                }
                Ok(Self(sd))
            }
        }
    }
    static PIPE_ID: AtomicU64 = AtomicU64::new(0);
    fn pipe(input: bool) -> Result<(Handle, Handle), ProcessError> {
        unsafe {
            let name = wide(OsStr::new(&format!(
                r"\\.\pipe\VcXsrvWSL.command.{}.{}",
                std::process::id(),
                PIPE_ID.fetch_add(1, Ordering::Relaxed)
            )))?;
            let security = PipeSecurity::current_user()?;
            let parent_sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: security.0,
                bInheritHandle: 0,
            };
            let parent = Handle::new(CreateNamedPipeW(
                name.as_ptr(),
                if input {
                    PIPE_ACCESS_OUTBOUND
                } else {
                    PIPE_ACCESS_INBOUND
                } | FILE_FLAG_OVERLAPPED
                    | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                16384,
                16384,
                0,
                &parent_sa,
            ))?;
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: null_mut(),
                bInheritHandle: 1,
            };
            let child = Handle::new(CreateFileW(
                name.as_ptr(),
                (if input { GENERIC_READ } else { GENERIC_WRITE }) | READ_CONTROL,
                0,
                &sa,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            ))?;
            // Client is already connected; no blocking ConnectNamedPipe operation is needed.
            Ok((parent, child))
        }
    }
    struct Io {
        handle: Handle,
        event: Handle,
        overlapped: Box<OVERLAPPED>,
        buffer: Vec<u8>,
        pending: bool,
        done: bool,
        write: bool,
        offset: usize,
    }
    unsafe impl Send for Io {}
    impl Io {
        fn new(handle: Handle, write: bool, buffer: Vec<u8>) -> Result<Self, ProcessError> {
            unsafe {
                let event = Handle::new(CreateEventW(null(), 1, 0, null()))?;
                let mut overlapped: Box<OVERLAPPED> = Box::new(std::mem::zeroed());
                overlapped.hEvent = event.0;
                Ok(Self {
                    handle,
                    event,
                    overlapped,
                    buffer,
                    pending: false,
                    done: false,
                    write,
                    offset: 0,
                })
            }
        }
        /// One bounded chunk. Returns captured bytes/count; no synchronous I/O or wait.
        fn step(&mut self) -> Result<Option<usize>, ProcessError> {
            unsafe {
                if self.done {
                    return Ok(None);
                }
                if !self.pending {
                    if self.write && self.offset == self.buffer.len() {
                        self.done = true;
                        return Ok(None);
                    }
                    ResetEvent(self.event.0);
                    let length = if self.write {
                        (self.buffer.len() - self.offset).min(16384)
                    } else {
                        self.buffer.len()
                    };
                    let ok = if self.write {
                        WriteFile(
                            self.handle.0,
                            self.buffer[self.offset..].as_ptr(),
                            length as u32,
                            null_mut(),
                            &mut *self.overlapped,
                        )
                    } else {
                        ReadFile(
                            self.handle.0,
                            self.buffer.as_mut_ptr(),
                            length as u32,
                            null_mut(),
                            &mut *self.overlapped,
                        )
                    };
                    if ok == 0 {
                        let e = GetLastError();
                        if e == ERROR_BROKEN_PIPE || e == ERROR_NO_DATA {
                            self.done = true;
                            return Ok(None);
                        }
                        if e != ERROR_IO_PENDING {
                            return Err(ProcessError::Native(e));
                        }
                    }
                    self.pending = true;
                }
                let mut n = 0;
                if GetOverlappedResult(self.handle.0, &*self.overlapped, &mut n, 0) == 0 {
                    let e = GetLastError();
                    if e == ERROR_IO_INCOMPLETE {
                        return Ok(None);
                    }
                    self.pending = false;
                    if e == ERROR_BROKEN_PIPE || e == ERROR_NO_DATA {
                        self.done = true;
                        return Ok(None);
                    }
                    return Err(ProcessError::Native(e));
                }
                self.pending = false;
                if self.write {
                    self.offset += n as usize;
                } else if n == 0 {
                    self.done = true;
                }
                Ok(Some(n as usize))
            }
        }
        fn retire(mut self) {
            if self.pending {
                unsafe {
                    CancelIoEx(self.handle.0, &*self.overlapped);
                }
            }
            if self.pending && unsafe { WaitForSingleObject(self.event.0, 0) } != WAIT_OBJECT_0 {
                // Cancellation completion is asynchronous. A recycler owns the stable buffers
                // and handles; the command caller never joins a blocked I/O worker.
                let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(self)));
                let worker = shared.clone();
                if std::thread::Builder::new()
                    .name("command-io-reaper".into())
                    .spawn(move || {
                        let mut io = worker.lock().unwrap().take().unwrap();
                        if unsafe { WaitForSingleObject(io.event.0, INFINITE) } != WAIT_OBJECT_0 {
                            std::mem::forget(io);
                            return;
                        }
                        io.pending = false;
                        if io.write {
                            io.buffer.fill(0);
                        }
                    })
                    .is_err()
                {
                    // Resource exhaustion cannot justify freeing a kernel-owned buffer.
                    if let Some(io) = shared.lock().unwrap().take() {
                        std::mem::forget(io);
                    }
                }
            } else {
                self.pending = false;
                if self.write {
                    self.buffer.fill(0);
                }
            }
        }
    }
    struct Streams {
        input: Option<Io>,
        out: Option<Io>,
        err: Option<Io>,
    }
    impl Drop for Streams {
        fn drop(&mut self) {
            for slot in [&mut self.input, &mut self.out, &mut self.err] {
                if let Some(io) = slot.take() {
                    io.retire();
                }
            }
        }
    }
    struct Job(Handle);
    impl Job {
        fn new() -> Result<Self, ProcessError> {
            unsafe {
                let h = Handle::new(CreateJobObjectW(null(), null()))?;
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    h.0,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                ) == 0
                {
                    return Err(last());
                }
                Ok(Self(h))
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                TerminateJobObject(self.0 .0, 1);
            }
        }
    }
    pub(super) fn run(
        spec: &CommandSpec,
        input: &[u8],
        limit: usize,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<CommandOutput, ProcessError> {
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
            return Err(ProcessError::Cancelled);
        }
        let deadline = Instant::now()
            .checked_add(spec.timeout)
            .ok_or(ProcessError::InvalidCommand)?;
        if !spec.program.is_absolute() {
            return Err(ProcessError::InvalidCommand);
        }
        if input.len() > 1024 * 1024 {
            return Err(ProcessError::InputLimit);
        }
        let app = wide(spec.program.as_os_str())?;
        let mut argv = Vec::new();
        argument(spec.program.as_os_str(), &mut argv)?;
        for a in &spec.args {
            argv.push(32);
            argument(a, &mut argv)?;
        }
        if argv.len() >= 32767 {
            return Err(ProcessError::InvalidCommand);
        }
        argv.push(0);
        if Instant::now() >= deadline {
            return Err(ProcessError::Timeout);
        }
        let (pin, cin) = pipe(true)?;
        let (pout, cout) = pipe(false)?;
        let (perr, cerr) = pipe(false)?;
        let mut streams = Streams {
            input: Some(Io::new(pin, true, input.to_vec())?),
            out: Some(Io::new(pout, false, vec![0; 16384])?),
            err: Some(Io::new(perr, false, vec![0; 16384])?),
        };
        let handles = [cin.0, cout.0, cerr.0];
        let mut attrs = Attributes::new(&handles)?;
        let job = Job::new()?;
        let (process, thread) = unsafe {
            let mut si: STARTUPINFOEXW = std::mem::zeroed();
            si.StartupInfo.cb = std::mem::size_of_val(&si) as u32;
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdInput = cin.0;
            si.StartupInfo.hStdOutput = cout.0;
            si.StartupInfo.hStdError = cerr.0;
            si.lpAttributeList = attrs.pointer();
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                return Err(ProcessError::Cancelled);
            }
            if CreateProcessW(
                app.as_ptr(),
                argv.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                null(),
                null(),
                &si.StartupInfo,
                &mut pi,
            ) == 0
            {
                return Err(last());
            }
            (Handle::new(pi.hProcess)?, Handle::new(pi.hThread)?)
        };
        drop(cin);
        drop(cout);
        drop(cerr);
        unsafe {
            if AssignProcessToJobObject(job.0 .0, process.0) == 0 {
                let e = last();
                TerminateProcess(process.0, 1);
                return Err(e);
            }
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                return Err(ProcessError::Cancelled);
            }
            if ResumeThread(thread.0) == u32::MAX {
                let e = last();
                TerminateProcess(process.0, 1);
                return Err(e);
            }
        }
        drop(thread);
        let mut output = CommandOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            status: 0,
        };
        let mut exited = false;
        let mut input_closed = false;
        let mut captured = 0usize;
        loop {
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                return Err(ProcessError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(ProcessError::Timeout);
            }
            if let Some(io) = streams.input.as_mut() {
                io.step()?;
                if io.done || (io.write && !io.pending && io.offset == io.buffer.len()) {
                    input_closed = io.offset != io.buffer.len();
                    streams.input.take().unwrap().retire();
                }
            }
            for (slot, destination) in [
                (&mut streams.out, &mut output.stdout),
                (&mut streams.err, &mut output.stderr),
            ] {
                if let Some(io) = slot.as_mut() {
                    if let Some(n) = io.step()? {
                        captured = captured.saturating_add(n);
                        if captured > limit {
                            return Err(ProcessError::OutputLimit);
                        }
                        destination.extend_from_slice(&io.buffer[..n]);
                    }
                    if io.done {
                        slot.take().unwrap().retire();
                    }
                }
            }
            if output.stdout.len().saturating_add(output.stderr.len()) > limit {
                return Err(ProcessError::OutputLimit);
            }
            if !exited {
                let result = unsafe { WaitForSingleObject(process.0, 0) };
                if result == WAIT_FAILED {
                    return Err(last());
                }
                exited = result == WAIT_OBJECT_0;
            }
            if exited && streams.out.is_none() && streams.err.is_none() {
                if unsafe { GetExitCodeProcess(process.0, &mut output.status) } == 0 {
                    return Err(last());
                }
                if output.status != 0 {
                    return Err(ProcessError::Nonzero(output.status));
                }
                if input_closed || streams.input.is_some() {
                    return Err(ProcessError::InputClosed);
                }
                return Ok(output);
            }
            let mut events = Vec::new();
            if !exited {
                events.push(process.0);
            }
            for io in [&streams.input, &streams.out, &streams.err]
                .into_iter()
                .flatten()
            {
                if io.pending {
                    events.push(io.event.0);
                }
            }
            if !events.is_empty() {
                let wait = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(10) as u32;
                if unsafe { WaitForMultipleObjects(events.len() as u32, events.as_ptr(), 0, wait) }
                    == WAIT_FAILED
                {
                    return Err(last());
                }
            }
        }
    }
}
