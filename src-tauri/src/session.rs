use std::path::{Path, PathBuf};

pub fn uid_from_status(status: &str) -> Option<String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|values| values.split_whitespace().next())
        .filter(|uid| !uid.is_empty())
        .map(str::to_string)
}

pub fn bus_address(runtime_dir: &Path) -> String {
    format!("unix:path={}", runtime_dir.join("bus").display())
}

fn runtime_dir() -> Option<PathBuf> {
    let configured = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.join("bus").exists());
    if configured.is_some() {
        return configured;
    }

    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| uid_from_status(&status))?;
    let fallback = PathBuf::from(format!("/run/user/{uid}"));
    (fallback.join("bus").exists()).then_some(fallback)
}

pub fn ensure_session_environment() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        return;
    }
    let Some(runtime) = runtime_dir() else {
        return;
    };
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        std::env::set_var("XDG_RUNTIME_DIR", &runtime);
    }
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", bus_address(&runtime));
}

#[cfg(test)]
mod tests {
    use super::{bus_address, uid_from_status};

    #[test]
    fn reads_the_real_uid_from_proc_status() {
        let status = "Name:\tauramail\nUid:\t1000\t1000\t1000\t1000\nGid:\t100\t100\t100\t100\n";
        assert_eq!(uid_from_status(status).as_deref(), Some("1000"));
        assert_eq!(uid_from_status("Name:\tauramail\n"), None);
    }

    #[test]
    fn builds_a_session_bus_address() {
        assert_eq!(
            bus_address(std::path::Path::new("/run/user/1000")),
            "unix:path=/run/user/1000/bus"
        );
    }
}
