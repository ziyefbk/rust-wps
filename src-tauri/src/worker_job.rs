//! Windows commit-memory quota and child-process lifetime for parser workers.
use std::{os::windows::io::AsRawHandle, process::Child};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::*,
    },
    core::PCWSTR,
};

pub struct WorkerJob(HANDLE);
impl WorkerJob {
    pub fn attach(child: &Child, memory_bytes: usize) -> Result<Self, String> {
        // SAFETY: Handle ownership stays in this RAII wrapper. The process
        // handle is borrowed from a live Child. The information buffer is a
        // correctly sized, initialized Windows structure, used synchronously.
        unsafe {
            let job = Self(CreateJobObjectW(None, PCWSTR::null()).map_err(|e| e.to_string())?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            limits.BasicLimitInformation.ActiveProcessLimit = 1;
            limits.ProcessMemoryLimit = memory_bytes;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|e| e.to_string())?;
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))
                .map_err(|e| e.to_string())?;
            Ok(job)
        }
    }
}
impl Drop for WorkerJob {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    #[test]
    fn commit_memory_quota_is_enforced_by_windows() {
        let script = "$ErrorActionPreference='Stop'; $null=[Console]::In.ReadToEnd(); try {$x=New-Object byte[] 268435456; [Console]::Out.Write('allocated')} catch {exit 12}";
        let control = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(control.status.success());
        assert!(String::from_utf8_lossy(&control.stdout).contains("allocated"));
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let _job = WorkerJob::attach(&child, 128 * 1024 * 1024).unwrap();
        child.stdin.take().unwrap().write_all(b"start").unwrap();
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                panic!("Quota test worker hung");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("allocated"));
    }
}
