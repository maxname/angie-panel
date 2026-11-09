//! Thin bridge to systemd over D-Bus. All calls are Linux-only; on other
//! platforms (dev on macOS) the stubs report "unavailable" so the panel can
//! degrade gracefully.

#[derive(Debug, thiserror::Error)]
// The Denied variant is only constructed by the Linux implementation.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum SystemdError {
    #[error("D-Bus/polkit denied the request: {0}")]
    Denied(String),
    #[error("systemd unavailable: {0}")]
    Unavailable(String),
}

#[cfg(target_os = "linux")]
mod imp {
    use super::SystemdError;

    const DEST: &str = "org.freedesktop.systemd1";
    const PATH: &str = "/org/freedesktop/systemd1";
    const MANAGER: &str = "org.freedesktop.systemd1.Manager";

    async fn system_bus() -> Result<zbus::Connection, SystemdError> {
        zbus::Connection::system()
            .await
            .map_err(|e| SystemdError::Unavailable(e.to_string()))
    }

    pub async fn dbus_available() -> bool {
        zbus::Connection::system().await.is_ok()
    }

    /// Start a unit (mode "replace"). Requires polkit authorization when the
    /// caller is unprivileged — this is the panel's only privileged action.
    pub async fn start_unit(name: &str) -> Result<(), SystemdError> {
        let conn = system_bus().await?;
        match conn
            .call_method(
                Some(DEST),
                PATH,
                Some(MANAGER),
                "StartUnit",
                &(name, "replace"),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(err_name, detail, _)) => {
                let name_str = err_name.as_str();
                let detail = detail.unwrap_or_default();
                if name_str.contains("AccessDenied")
                    || name_str.contains("InteractiveAuthorizationRequired")
                {
                    Err(SystemdError::Denied(format!("{name_str}: {detail}")))
                } else {
                    Err(SystemdError::Unavailable(format!("{name_str}: {detail}")))
                }
            }
            Err(e) => Err(SystemdError::Unavailable(e.to_string())),
        }
    }

    /// Whether a unit is currently active. None = cannot tell.
    pub async fn unit_active(name: &str) -> Option<bool> {
        let conn = system_bus().await.ok()?;
        let reply = conn
            .call_method(Some(DEST), PATH, Some(MANAGER), "LoadUnit", &(name,))
            .await
            .ok()?;
        let unit_path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize().ok()?;
        let reply = conn
            .call_method(
                Some(DEST),
                unit_path.as_str(),
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.freedesktop.systemd1.Unit", "ActiveState"),
            )
            .await
            .ok()?;
        let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
        let state: String = value.try_into().ok()?;
        Some(state == "active")
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::SystemdError;

    pub async fn dbus_available() -> bool {
        false
    }

    pub async fn start_unit(_name: &str) -> Result<(), SystemdError> {
        Err(SystemdError::Unavailable(
            "systemd is only available on Linux".into(),
        ))
    }

    pub async fn unit_active(_name: &str) -> Option<bool> {
        None
    }
}

pub use imp::{dbus_available, start_unit, unit_active};
