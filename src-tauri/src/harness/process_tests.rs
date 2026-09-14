#![cfg(windows)]
use super::{execution::ExecutionScope, process, tools::ErrorCode};
use serde_json::{json, Value};
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

pub fn python() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rag-service/.venv/Scripts/python.exe");
    assert!(
        path.is_file(),
        "process acceptance requires the repository Python environment"
    );
    path
}
pub fn tree_script(marker: &Path, parent_exits: bool) -> String {
    format!("import subprocess,os,time,json,pathlib\nchild=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])\npathlib.Path({}).write_text(json.dumps({{'parent':os.getpid(),'child':child.pid}}))\nprint('ready',flush=True)\n{}",json!(marker.to_string_lossy()),if parent_exits {""}else{"time.sleep(60)"})
}
pub async fn wait_marker(path: &Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return value;
            }
        }
        assert!(Instant::now() < deadline, "test process did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
pub fn process_handle(pid: u32) -> OwnedHandle {
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.unwrap();
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}
pub fn exited(handle: &OwnedHandle) -> bool {
    unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 0) == WAIT_OBJECT_0 }
}

#[test]
fn python_worker_drains_stderr_and_returns_bounded_stdout() {
    tauri::async_runtime::block_on(async {
        let scope = ExecutionScope::new(10_000);
        let cmd=process::python_command(&python(),"sys.stdin.read()\nsys.stderr.write('log'*200000)\nprint('TELLWHY_DESKTOP:{\"ok\":true,\"data\":{\"ready\":true}}',flush=True)");
        let output = process::execute(cmd, b"{}".to_vec(), scope.control.clone())
            .await
            .unwrap();
        scope.finish().await.unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("\"ready\":true"));
    });
}

#[test]
fn python_cancel_and_dropped_future_reap_parent_and_descendants() {
    tauri::async_runtime::block_on(async {
        for drop_future in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let marker = directory.path().join("pids.json");
            let scope = ExecutionScope::new(15_000);
            let cmd = process::python_command(&python(), &tree_script(&marker, false));
            let task = tokio::spawn(process::execute(cmd, vec![], scope.control.clone()));
            let ids = wait_marker(&marker).await;
            let parent = process_handle(ids["parent"].as_u64().unwrap() as u32);
            let child = process_handle(ids["child"].as_u64().unwrap() as u32);
            assert!(!exited(&parent) && !exited(&child));
            let start = Instant::now();
            if drop_future {
                task.abort();
            } else {
                scope.control.cancel();
            }
            scope.finish().await.unwrap();
            assert!(start.elapsed() < Duration::from_secs(3));
            assert!(exited(&parent) && exited(&child));
            if !drop_future {
                assert_eq!(task.await.unwrap().unwrap_err().code, ErrorCode::Cancelled);
            }
        }
    });
}

#[test]
fn python_timeout_and_closed_scope_stop_blocked_input_and_discard_late_output() {
    tauri::async_runtime::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid.json");
        // Include interpreter startup under concurrent test load. The worker still
        // blocks on input and sleeps for 60 seconds, well beyond this deadline.
        let scope = ExecutionScope::new(5_000);
        let script=format!("import os,time,pathlib,json\npathlib.Path({}).write_text(json.dumps({{'parent':os.getpid()}}))\ntime.sleep(60)\nprint('late',flush=True)",json!(marker.to_string_lossy()));
        let task = tokio::spawn(process::execute(
            process::python_command(&python(), &script),
            vec![b'x'; 65_536],
            scope.control.clone(),
        ));
        let ids = wait_marker(&marker).await;
        let parent = process_handle(ids["parent"].as_u64().unwrap() as u32);
        assert_eq!(task.await.unwrap().unwrap_err().code, ErrorCode::Timeout);
        scope.finish().await.unwrap();
        assert!(exited(&parent));

        let closed = ExecutionScope::new(10_000);
        closed.control.cancel();
        let never = directory.path().join("never-started");
        let script = format!(
            "from pathlib import Path\nPath({}).touch()",
            json!(never.to_string_lossy())
        );
        assert_eq!(
            process::execute(
                process::python_command(&python(), &script),
                vec![],
                closed.control.clone()
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Cancelled
        );
        closed.finish().await.unwrap();
        assert!(!never.exists());
    });
}

#[test]
fn python_parent_exit_still_cleans_children_and_oversized_output_is_rejected() {
    tauri::async_runtime::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("tree.json");
        let scope = ExecutionScope::new(10_000);
        let mut script = tree_script(&marker, false);
        script = format!(
            "{}time.sleep(0.8)",
            script.strip_suffix("time.sleep(60)").unwrap()
        );
        let task = tokio::spawn(process::execute(
            process::python_command(&python(), &script),
            vec![],
            scope.control.clone(),
        ));
        let ids = wait_marker(&marker).await;
        let child = process_handle(ids["child"].as_u64().unwrap() as u32);
        let output = task.await.unwrap().unwrap();
        scope.finish().await.unwrap();
        assert!(output.status.success() && exited(&child));

        let scope = ExecutionScope::new(10_000);
        let result = process::execute(
            process::python_command(&python(), "print('x'*3000000,flush=True)"),
            vec![],
            scope.control.clone(),
        )
        .await;
        scope.finish().await.unwrap();
        assert_eq!(result.unwrap_err().code, ErrorCode::OutputLimit);
    });
}
