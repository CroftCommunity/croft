//! The state directory: the endpoint's secret key, the stored session, the
//! camping pass. Three files, plain, mode 0600 where the OS has modes.
//!
//! The key is what makes the endpoint id stable across runs (the mint binds
//! a pass to the id, and the published record names it — §16's rig-state
//! note is what happens when a key is lost). The pass is the cache (O1: the
//! token IS the cache). The session is a stored token, which is not a
//! session until the PDS says so.

use std::path::{Path, PathBuf};

use call_core::model::CampPass;

use crate::session::StoredSession;
use crate::ArcError;

/// Where a run keeps its three files.
#[derive(Debug)]
pub struct StateDir {
    path: PathBuf,
}

impl StateDir {
    /// Open (creating) `explicit`, or the default under the user's state
    /// directory (`$XDG_STATE_HOME` or `~/.local/state`, then `croft-arc`).
    pub fn open(explicit: Option<PathBuf>) -> Result<Self, ArcError> {
        let path = match explicit {
            Some(p) => p,
            None => default_dir()?,
        };
        std::fs::create_dir_all(&path).map_err(|e| ArcError::State {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        Ok(StateDir { path })
    }

    /// The directory itself.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The endpoint's secret key, generated on first use.
    pub fn secret_key(&self) -> Result<[u8; 32], ArcError> {
        let file = self.path.join("endpoint.key");
        if let Some(bytes) = self.read(&file)? {
            return bytes.as_slice().try_into().map_err(|_| ArcError::State {
                path: file.display().to_string(),
                reason: format!("an endpoint key is 32 bytes, got {}", bytes.len()),
            });
        }
        let key = iroh::SecretKey::generate().to_bytes();
        self.write(&file, &key)?;
        Ok(key)
    }

    /// The stored session, if any.
    pub fn session(&self) -> Result<Option<StoredSession>, ArcError> {
        self.read_json(&self.path.join("session.json"))
    }

    /// Remember a session.
    pub fn save_session(&self, s: &StoredSession) -> Result<(), ArcError> {
        self.write_json(&self.path.join("session.json"), s)
    }

    /// The cached camping pass, if any.
    pub fn pass(&self) -> Result<Option<CampPass>, ArcError> {
        let file = self.path.join("pass.json");
        let Some(v): Option<serde_json::Value> = self.read_json(&file)? else {
            return Ok(None);
        };
        let token = v.get("token").and_then(serde_json::Value::as_str);
        let expires = v
            .get("expires_at_millis")
            .and_then(serde_json::Value::as_i64);
        Ok(match (token, expires) {
            (Some(token), Some(expires_at_millis)) => Some(CampPass {
                token: token.to_string(),
                expires_at_millis,
            }),
            _ => None,
        })
    }

    /// Remember a camping pass.
    pub fn save_pass(&self, p: &CampPass) -> Result<(), ArcError> {
        self.write_json(
            &self.path.join("pass.json"),
            &serde_json::json!({ "token": p.token, "expires_at_millis": p.expires_at_millis }),
        )
    }

    fn read(&self, file: &Path) -> Result<Option<Vec<u8>>, ArcError> {
        match std::fs::read(file) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ArcError::State {
                path: file.display().to_string(),
                reason: e.to_string(),
            }),
        }
    }

    fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        file: &Path,
    ) -> Result<Option<T>, ArcError> {
        let Some(bytes) = self.read(file)? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| ArcError::State {
                path: file.display().to_string(),
                reason: format!("not the JSON this build writes: {e}"),
            })
    }

    fn write(&self, file: &Path, bytes: &[u8]) -> Result<(), ArcError> {
        let fail = |e: std::io::Error| ArcError::State {
            path: file.display().to_string(),
            reason: e.to_string(),
        };
        std::fs::write(file, bytes).map_err(fail)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).map_err(fail)?;
        }
        Ok(())
    }

    fn write_json<T: serde::Serialize>(&self, file: &Path, value: &T) -> Result<(), ArcError> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| ArcError::State {
            path: file.display().to_string(),
            reason: e.to_string(),
        })?;
        self.write(file, &bytes)
    }
}

fn default_dir() -> Result<PathBuf, ArcError> {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(xdg).join("croft-arc"));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| ArcError::State {
        path: "~/.local/state/croft-arc".to_string(),
        reason: "neither XDG_STATE_HOME nor HOME is set; pass --state-dir".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".local/state/croft-arc"))
}
