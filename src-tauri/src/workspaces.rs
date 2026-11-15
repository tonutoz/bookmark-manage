//! 워크스페이스: 업무용·개인용처럼 서로 섞이지 않는 북마크 묶음.
//! 워크스페이스 하나가 DB 파일 한 벌(북마크 + 썸네일)이고, 앱은 한 번에 하나만 열어 둔다.
//! 그래서 북마크를 읽고 쓰는 나머지 코드는 워크스페이스를 몰라도 된다.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 처음부터 있는 워크스페이스. 앱 데이터 폴더 바로 아래의 DB 를 쓰고, 지울 수 없다.
pub const DEFAULT_ID: &str = "default";
pub const REGISTRY_FILE: &str = "workspaces.json";
pub const FOLDER: &str = "workspaces";
const MAX_NAME: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    /// 지금 열려 있는 워크스페이스의 id
    pub active: String,
    pub items: Vec<Workspace>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry {
            active: DEFAULT_ID.into(),
            items: vec![Workspace { id: DEFAULT_ID.into(), name: "개인".into() }],
        }
    }
}

/// 폴더 이름으로 써도 안전한 id 인지.
pub fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 워크스페이스의 DB 가 들어 있는 폴더.
pub fn dir(app_dir: &Path, id: &str) -> PathBuf {
    if id == DEFAULT_ID {
        app_dir.to_path_buf()
    } else {
        app_dir.join(FOLDER).join(id)
    }
}

pub fn load(app_dir: &Path) -> Registry {
    let mut registry: Registry = fs::read_to_string(app_dir.join(REGISTRY_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    // 파일이 손으로 고쳐졌거나 깨졌어도 앱이 뜰 수 있게 바로잡는다.
    registry.items.retain(|w| safe_id(&w.id));
    if !registry.contains(DEFAULT_ID) {
        registry.items.insert(0, Registry::default().items.remove(0));
    }
    if !registry.contains(&registry.active) {
        registry.active = DEFAULT_ID.into();
    }
    registry
}

pub fn save(app_dir: &Path, registry: &Registry) -> Result<(), String> {
    let text = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    fs::write(app_dir.join(REGISTRY_FILE), text).map_err(|e| format!("워크스페이스 목록을 저장할 수 없습니다: {e}"))
}

/// 지운 워크스페이스의 DB 폴더를 없앤다.
pub fn delete_files(app_dir: &Path, id: &str) -> Result<(), String> {
    if id == DEFAULT_ID || !safe_id(id) {
        return Err("지울 수 없는 워크스페이스입니다".into());
    }
    match fs::remove_dir_all(dir(app_dir, id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("워크스페이스 파일을 지울 수 없습니다: {e}")),
    }
}

impl Registry {
    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|w| w.id == id)
    }

    #[cfg(test)]
    pub fn name_of(&self, id: &str) -> Option<&str> {
        self.items.iter().find(|w| w.id == id).map(|w| w.name.as_str())
    }

    /// 이름을 다듬고, 비었거나 너무 길거나 이미 있는 이름이면 거절한다. `except` 는 이름을 바꾸는 중인 자신.
    fn clean_name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("이름을 입력하세요".into());
        }
        if name.chars().count() > MAX_NAME {
            return Err(format!("이름은 {MAX_NAME}자까지 쓸 수 있습니다"));
        }
        let taken = self.items.iter().any(|w| Some(w.id.as_str()) != except && w.name.to_lowercase() == name.to_lowercase());
        if taken {
            return Err("같은 이름의 워크스페이스가 이미 있습니다".into());
        }
        Ok(name.to_string())
    }

    pub fn add(&mut self, name: &str) -> Result<Workspace, String> {
        let name = self.clean_name(name, None)?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let mut id = format!("ws-{stamp:x}");
        while self.contains(&id) {
            id.push('x');
        }
        let workspace = Workspace { id, name };
        self.items.push(workspace.clone());
        Ok(workspace)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = self.clean_name(name, Some(id))?;
        let workspace = self.items.iter_mut().find(|w| w.id == id).ok_or("없는 워크스페이스입니다")?;
        workspace.name = name;
        Ok(())
    }

    /// 목록에서 뺀다. 기본 워크스페이스와 지금 열려 있는 것은 뺄 수 없다.
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        if id == DEFAULT_ID {
            return Err("처음부터 있던 워크스페이스는 지울 수 없습니다".into());
        }
        if id == self.active {
            return Err("지금 열려 있는 워크스페이스는 지울 수 없습니다. 다른 곳으로 전환한 뒤 지우세요".into());
        }
        let before = self.items.len();
        self.items.retain(|w| w.id != id);
        if self.items.len() == before {
            return Err("없는 워크스페이스입니다".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_one_workspace_in_the_app_folder() {
        let registry = Registry::default();
        assert_eq!((registry.active.as_str(), registry.items.len()), (DEFAULT_ID, 1));
        let app = Path::new("app");
        assert_eq!(dir(app, DEFAULT_ID), app);
        assert_eq!(dir(app, "ws-1"), app.join("workspaces").join("ws-1"));
    }

    #[test]
    fn add_rename_remove_rules() {
        let mut registry = Registry::default();
        let work = registry.add("  업무 ").unwrap();
        assert_eq!(work.name, "업무");
        assert!(safe_id(&work.id) && work.id != DEFAULT_ID);

        assert!(registry.add("업무").is_err(), "같은 이름");
        assert!(registry.add("   ").is_err(), "빈 이름");
        assert!(registry.add(&"가".repeat(41)).is_err(), "너무 긴 이름");
        assert!(registry.rename(&work.id, "개인").is_err(), "다른 워크스페이스의 이름");
        registry.rename(&work.id, "업무").unwrap();
        registry.rename(&work.id, "회사").unwrap();
        assert_eq!(registry.name_of(&work.id), Some("회사"));

        assert!(registry.remove(DEFAULT_ID).is_err());
        registry.active = work.id.clone();
        assert!(registry.remove(&work.id).is_err(), "열려 있는 것");
        registry.active = DEFAULT_ID.into();
        registry.remove(&work.id).unwrap();
        assert!(registry.remove(&work.id).is_err(), "이미 없는 것");
    }

    #[test]
    fn load_repairs_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("bm-ws-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir), Registry::default());

        fs::write(dir.join(REGISTRY_FILE), r#"{"active":"gone","items":[{"id":"../evil","name":"x"},{"id":"ws-1","name":"업무"}]}"#).unwrap();
        let repaired = load(&dir);
        assert_eq!(repaired.active, DEFAULT_ID);
        let ids: Vec<&str> = repaired.items.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, [DEFAULT_ID, "ws-1"]);

        save(&dir, &repaired).unwrap();
        assert_eq!(load(&dir), repaired);
        assert!(delete_files(&dir, DEFAULT_ID).is_err());
        assert!(delete_files(&dir, "..").is_err());
        assert!(delete_files(&dir, "ws-never-created").is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
