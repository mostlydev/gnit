use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const ROSTER_PATH: &str = ".nit/roster.yaml";
pub const PINS_DIR: &str = ".nit/pins";

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Roster {
    pub version: u32,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default)]
    pub members: Vec<RosterMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct RosterMember {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_excludes: Vec<String>,
}

impl Roster {
    pub fn new(mode: impl Into<String>, remote: Option<String>) -> Self {
        Self {
            version: 1,
            mode: mode.into(),
            remote,
            members: Vec::new(),
        }
    }

    pub fn read(root: &Path) -> Result<Self> {
        let path = root.join(ROSTER_PATH);
        let text =
            fs::read_to_string(&path).with_context(|| format!("read roster {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parse roster {}", path.display()))
    }

    pub fn write(&self, root: &Path) -> Result<()> {
        let path = root.join(ROSTER_PATH);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create roster directory {}", parent.display()))?;
        }
        let text = serde_yaml::to_string(self).context("serialize roster")?;
        fs::write(&path, text).with_context(|| format!("write roster {}", path.display()))
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.members.iter().any(|member| member.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Pin {
    pub version: u32,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default)]
    pub members: Vec<PinMember>,
    #[serde(default)]
    pub provenance: PinProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct PinMember {
    pub id: String,
    pub path: String,
    pub commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_hint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct PinProvenance {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
}

impl Pin {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            version: 1,
            id: id.into(),
            label: None,
            created_at: None,
            members: Vec::new(),
            provenance: PinProvenance::default(),
        }
    }

    pub fn path(root: &Path, id: &str) -> PathBuf {
        root.join(PINS_DIR).join(format!("{id}.yaml"))
    }

    pub fn read(root: &Path, id: &str) -> Result<Self> {
        let path = Self::path(root, id);
        let text =
            fs::read_to_string(&path).with_context(|| format!("read pin {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parse pin {}", path.display()))
    }

    pub fn write(&self, root: &Path) -> Result<PathBuf> {
        let path = Self::path(root, &self.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create pin directory {}", parent.display()))?;
        }
        let text = serde_yaml::to_string(self).context("serialize pin")?;
        fs::write(&path, text).with_context(|| format!("write pin {}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_round_trips_members_and_excludes() {
        let temp = tempfile::tempdir().unwrap();
        let mut roster = Roster::new("shared", None);
        roster.members.push(RosterMember {
            id: "app".to_string(),
            path: "app".to_string(),
            remote: Some("git@example.com:app.git".to_string()),
            required_excludes: vec!["app".to_string()],
        });

        roster.write(temp.path()).unwrap();
        let actual = Roster::read(temp.path()).unwrap();

        assert_eq!(actual, roster);
        assert!(actual.contains_id("app"));
        assert!(!actual.contains_id("sdk"));
    }

    #[test]
    fn pin_round_trips_to_pins_directory() {
        let temp = tempfile::tempdir().unwrap();
        let mut pin = Pin::new("PIN-20260603-test");
        pin.label = Some("baseline".to_string());
        pin.members.push(PinMember {
            id: "app".to_string(),
            path: "app".to_string(),
            commit: "abc123".to_string(),
            branch_hint: Some("main".to_string()),
        });
        pin.provenance.changes.push("NCH-20260603-test".to_string());

        let path = pin.write(temp.path()).unwrap();
        let actual = Pin::read(temp.path(), "PIN-20260603-test").unwrap();

        assert_eq!(path, temp.path().join(".nit/pins/PIN-20260603-test.yaml"));
        assert_eq!(actual, pin);
    }
}
