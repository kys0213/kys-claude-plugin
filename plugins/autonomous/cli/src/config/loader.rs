use std::path::Path;

use anyhow::Result;

use super::models::WorkflowConfig;

const CONFIG_FILENAME: &str = ".develop-workflow.yaml";

/// 글로벌(~/) + 레포별 YAML을 머지하여 최종 설정 반환
pub fn load_merged(repo_path: Option<&Path>) -> WorkflowConfig {
    let global = load_global();
    let repo = repo_path.and_then(|p| load_from_dir(p));

    match (global, repo) {
        (Some(g), Some(r)) => merge(g, r),
        (Some(g), None) => g,
        (None, Some(r)) => r,
        (None, None) => WorkflowConfig::default(),
    }
}

/// 글로벌 설정 로드: ~/.develop-workflow.yaml
fn load_global() -> Option<WorkflowConfig> {
    let home = std::env::var("HOME").ok()?;
    let path = Path::new(&home).join(CONFIG_FILENAME);
    load_file(&path)
}

/// 디렉토리에서 .develop-workflow.yaml 로드
fn load_from_dir(dir: &Path) -> Option<WorkflowConfig> {
    let path = dir.join(CONFIG_FILENAME);
    load_file(&path)
}

/// YAML 파일 읽기 + 파싱
fn load_file(path: &Path) -> Option<WorkflowConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_yaml::from_str(&content).ok()
}

/// 레포 설정으로 글로벌 설정을 오버라이드 (딥머지)
/// 레포 YAML에 명시된 값만 덮어씀
fn merge(base: WorkflowConfig, over: WorkflowConfig) -> WorkflowConfig {
    // serde_yaml → serde_json::Value로 변환 후 딥머지
    let base_val = serde_json::to_value(&base).unwrap_or_default();
    let over_val = serde_json::to_value(&over).unwrap_or_default();

    let merged = deep_merge(base_val, over_val);
    serde_json::from_value(merged).unwrap_or(base)
}

/// JSON Value 딥머지: over의 non-null 값으로 base를 덮어씀
fn deep_merge(base: serde_json::Value, over: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;

    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (key, over_val) in o {
                let base_val = b.remove(&key).unwrap_or(Value::Null);
                b.insert(key, deep_merge(base_val, over_val));
            }
            Value::Object(b)
        }
        (base, Value::Null) => base,
        (_, over) => over,
    }
}

/// 글로벌 설정 파일 경로
pub fn global_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(CONFIG_FILENAME)
}

/// 글로벌 설정 파일 초기화 (setup 시 사용)
pub fn init_global(config: &WorkflowConfig) -> Result<()> {
    let path = global_config_path();
    let yaml = serde_yaml::to_string(config)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}
