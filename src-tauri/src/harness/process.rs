//! Windows process-tree ownership for scoped, read-only Python operations.
//! Python starts suspended and joins its job before its first thread is resumed.
//! The bootstrap additionally waits for input. This is not a security sandbox.
use super::{
    execution::ExecutionControl,
    tools::{ErrorCode, ToolError},
};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 2 * 1024 * 1024;
#[derive(Debug)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub status: ExitStatus,
}

pub fn python_command(python: &Path, script: &str) -> Command {
    let mut command = Command::new(python);
    command.args([
        "-X",
        "utf8",
        "-u",
        "-c",
        &format!("import sys\nif sys.stdin.buffer.read(1) != b'!': sys.exit(125)\n{script}"),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{HANDLE, WAIT_OBJECT_0},
            System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
        },
    };

    pub struct Job(OwnedHandle);
    impl Job {
        pub fn new() -> Result<Self, ToolError> {
            let handle =
                unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|_| failure())?;
            // Own the handle before fallible configuration, so every exit closes it.
            let job = Self(unsafe { OwnedHandle::from_raw_handle(handle.0) });
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            unsafe {
                SetInformationJobObject(
                    job.handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32,
                )
            }
            .map_err(|_| failure())?;
            Ok(job)
        }
        fn handle(&self) -> HANDLE {
            HANDLE(self.0.as_raw_handle())
        }
        pub fn attach(&self, child: &Child) -> Result<(), ToolError> {
            unsafe { AssignProcessToJobObject(self.handle(), HANDLE(child.as_raw_handle())) }
                .map_err(|_| failure())
        }
        pub fn terminate(&self) -> Result<(), ToolError> {
            let handles = self.process_handles()?;
            unsafe { TerminateJobObject(self.handle(), 1) }.map_err(|_| failure())?;
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                unsafe {
                    QueryInformationJobObject(
                        Some(self.handle()),
                        JobObjectBasicAccountingInformation,
                        &mut info as *mut _ as _,
                        std::mem::size_of_val(&info) as u32,
                        None,
                    )
                }
                .map_err(|_| failure())?;
                if info.ActiveProcesses == 0 {
                    for handle in &handles {
                        let remaining = deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis()
                            .min(u32::MAX as u128) as u32;
                        if unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), remaining) }
                            != WAIT_OBJECT_0
                        {
                            return Err(failure());
                        }
                    }
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(failure());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn process_handles(&self) -> Result<Vec<OwnedHandle>, ToolError> {
            #[repr(C)]
            struct List {
                assigned: u32,
                count: u32,
                ids: [usize; 256],
            }
            let mut list = List {
                assigned: 0,
                count: 0,
                ids: [0; 256],
            };
            unsafe {
                QueryInformationJobObject(
                    Some(self.handle()),
                    JobObjectBasicProcessIdList,
                    &mut list as *mut _ as _,
                    std::mem::size_of_val(&list) as u32,
                    None,
                )
            }
            .map_err(|_| failure())?;
            if list.count > 256 {
                return Err(failure());
            }
            let mut handles = vec![];
            for pid in &list.ids[..list.count as usize] {
                // A process may exit between the job snapshot and opening its handle.
                if let Ok(handle) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, *pid as u32) }
                {
                    handles.push(unsafe { OwnedHandle::from_raw_handle(handle.0) });
                }
            }
            Ok(handles)
        }
    }
    pub fn resume(child: &Child) -> Result<(), ToolError> {
        let snapshot =
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(|_| failure())?;
        let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = unsafe { Thread32First(HANDLE(snapshot.as_raw_handle()), &mut entry) };
        while found.is_ok() {
            if entry.th32OwnerProcessID == child.id() {
                let thread =
                    unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
                        .map_err(|_| failure())?;
                let thread = unsafe { OwnedHandle::from_raw_handle(thread.0) };
                if unsafe { ResumeThread(HANDLE(thread.as_raw_handle())) } == u32::MAX {
                    return Err(failure());
                }
                return Ok(());
            }
            found = unsafe { Thread32Next(HANDLE(snapshot.as_raw_handle()), &mut entry) };
        }
        Err(failure())
    }
    fn failure() -> ToolError {
        ToolError::new(ErrorCode::CleanupFailed, "无法建立或清理资料进程组")
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub struct Job;
    pub fn resume(_: &Child) -> Result<(), ToolError> {
        unreachable!()
    }
    impl Job {
        pub fn new() -> Result<Self, ToolError> {
            Err(ToolError::new(
                ErrorCode::DependencyFailed,
                "当前平台未提供资料进程隔离",
            ))
        }
        pub fn attach(&self, _: &Child) -> Result<(), ToolError> {
            unreachable!()
        }
        pub fn terminate(&self) -> Result<(), ToolError> {
            unreachable!()
        }
    }
}

struct OwnedProcess {
    child: Child,
    job: platform::Job,
    control: ExecutionControl,
    reaped: bool,
}
impl OwnedProcess {
    fn cleanup(&mut self) -> Result<(), ToolError> {
        let result = self.job.terminate();
        if result.is_err() {
            self.control.cleanup_failed();
        }
        // Also reap the direct child. The job owns descendants even if it exited first.
        let _ = self.child.kill();
        self.child
            .wait()
            .map_err(|_| ToolError::new(ErrorCode::CleanupFailed, "无法回收资料进程"))?;
        self.reaped = true;
        result
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if !self.reaped && self.cleanup().is_err() {
            self.control.cleanup_failed();
        }
    }
}

fn collect(
    mut reader: impl Read,
    overflow: Arc<AtomicBool>,
    retain: bool,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(bytes);
        }
        if retain {
            if bytes.len() + count > MAX_OUTPUT {
                overflow.store(true, Ordering::SeqCst);
            } else {
                bytes.extend_from_slice(&buffer[..count]);
            }
        }
    }
}

fn run(
    mut command: Command,
    payload: Vec<u8>,
    control: ExecutionControl,
) -> Result<Output, ToolError> {
    control.check()?;
    if payload.len() > 65_536 {
        return Err(ToolError::new(ErrorCode::InvalidArguments, "资料请求过长"));
    }
    let job = platform::Job::new()?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
    }
    let child = command
        .spawn()
        .map_err(|_| ToolError::new(ErrorCode::DependencyFailed, "无法启动本地资料组件"))?;
    let mut owned = OwnedProcess {
        child,
        job,
        control: control.clone(),
        reaped: false,
    };
    owned.job.attach(&owned.child)?;
    platform::resume(&owned.child)?;
    // Task code cannot execute until attached. Writers/readers run independently so
    // a full pipe cannot prevent the owner from checking cancellation and deadlines.
    let mut stdin = owned.child.stdin.take().unwrap();
    let stdout = owned.child.stdout.take().unwrap();
    let stderr = owned.child.stderr.take().unwrap();
    let overflow = Arc::new(AtomicBool::new(false));
    let out_overflow = overflow.clone();
    let err_overflow = overflow.clone();
    let out = std::thread::spawn(move || collect(stdout, out_overflow, true));
    let err = std::thread::spawn(move || collect(stderr, err_overflow, false));
    let input = std::thread::spawn(move || {
        stdin.write_all(b"!")?;
        stdin.write_all(&payload)
    });
    let result = loop {
        if let Err(error) = control.check() {
            break Err(error);
        }
        if overflow.load(Ordering::SeqCst) {
            break Err(ToolError::new(ErrorCode::OutputLimit, "资料组件输出过长"));
        }
        match owned.child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                break Err(ToolError::new(
                    ErrorCode::DependencyUnavailable,
                    "无法读取资料进程状态",
                ))
            }
        }
    };
    let cleanup = owned.cleanup();
    drop(owned); // Closing the job is also a kill-on-close fallback on cleanup errors.
                 // Termination closes descendant pipe handles before these joins.
    let stdout = out.join();
    let _ = err.join();
    let input = input.join();
    cleanup?;
    let status = result?;
    control.check()?; // Never publish a result received after cancellation/deadline.
    if overflow.load(Ordering::SeqCst) {
        return Err(ToolError::new(ErrorCode::OutputLimit, "资料组件输出过长"));
    }
    input
        .map_err(|_| ToolError::new(ErrorCode::DependencyFailed, "资料请求写入中断"))?
        .map_err(|_| ToolError::new(ErrorCode::DependencyFailed, "无法写入资料请求"))?;
    let stdout = stdout
        .map_err(|_| ToolError::new(ErrorCode::InvalidResult, "资料结果读取中断"))?
        .map_err(|_| ToolError::new(ErrorCode::InvalidResult, "无法读取资料结果"))?;
    Ok(Output { stdout, status })
}

pub async fn execute(
    command: Command,
    payload: Vec<u8>,
    control: ExecutionControl,
) -> Result<Output, ToolError> {
    let worker = control.worker(); // Register before dispatch, including queued workers.
    tauri::async_runtime::spawn_blocking(move || {
        let _worker = worker;
        run(command, payload, control)
    })
    .await
    .map_err(|_| ToolError::new(ErrorCode::DependencyUnavailable, "资料工作线程中断"))?
}
