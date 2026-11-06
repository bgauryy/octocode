//! Persistent CLI calls own their socket namespace and every server in it.
use super::{Workspace, cli_executable};
use std::io;
use std::path::Path;
use std::process::Command;

pub struct PersistentCli<'a> {
    workspace: &'a Workspace,
    sockets: tempfile::TempDir,
}

impl<'a> PersistentCli<'a> {
    pub(super) fn new(workspace: &'a Workspace) -> Self {
        Self {
            workspace,
            // Short enough for sockaddr_un on macOS as well as Linux.
            sockets: tempfile::Builder::new()
                .prefix("oc")
                .tempdir_in("/tmp")
                .expect("private socket directory"),
        }
    }

    pub fn cli(&self) -> Command {
        let mut command = self.workspace.cli();
        command
            .env("TMPDIR", self.sockets())
            .env("OCTOCODE_STORAGE_MODE", "persistent");
        command
    }

    pub fn sockets(&self) -> &Path {
        self.sockets.path()
    }

    pub fn server_pids(&self) -> Vec<u32> {
        self.owned_server_pids().expect("list owned warm servers")
    }

    /// Stop only servers whose executable and socket still identify this
    /// fixture immediately before each signal. False means cleanup could not
    /// be verified; Drop never panics while another assertion is unwinding.
    pub fn stop(&self) -> bool {
        for signal in ["-TERM", "-KILL"] {
            let Ok(pids) = self.owned_server_pids() else {
                return false;
            };
            if pids.is_empty() {
                return true;
            }
            for pid in pids {
                if self
                    .owned_server_pids()
                    .is_ok_and(|current| current.contains(&pid))
                {
                    let _ = Command::new("kill")
                        .arg(signal)
                        .arg(pid.to_string())
                        .output();
                }
            }
            for _ in 0..100 {
                match self.owned_server_pids() {
                    Ok(pids) if pids.is_empty() => return true,
                    Err(_) => return false,
                    Ok(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            }
        }
        self.owned_server_pids().is_ok_and(|pids| pids.is_empty())
    }

    fn owned_server_pids(&self) -> io::Result<Vec<u32>> {
        let listed = Command::new("ps").args(["-axo", "pid=,args="]).output()?;
        if !listed.status.success() {
            return Err(io::Error::other("ps failed to list warm servers"));
        }
        let prefix = format!("{} serve --socket ", cli_executable().display());
        Ok(String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter_map(|line| {
                let (pid, args) = line.trim().split_once(char::is_whitespace)?;
                let socket = args.trim_start().strip_prefix(&prefix)?;
                let pid = pid.parse::<u32>().ok().filter(|pid| *pid > 0)?;
                Path::new(socket).starts_with(self.sockets()).then_some(pid)
            })
            .collect())
    }
}

impl Drop for PersistentCli<'_> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
