use std::path::Path;

use anyhow::Result;

use super::models::WorkflowConfig;
use super::Env;

const CONFIG_FILENAME: &str = ".develop-workflow.yaml";

/// 글로벌(~/) + 레포별 YAML을 머지하여 최종 설정 반환
/// Raw YAML Value 단계에서 딥머지 → 최종 역직렬화
pub fn load_merged(env: &dyn Env, repo_path: Option<&Path>) -> WorkflowConfig {
    let global = load_raw_yaml_global(env);
    let repo = repo_path.and_then(load_raw_yaml_from_dir);

    let merged = match (global, repo) {
        (Some(g), Some(r)) => deep_merge(g, r),
        (Some(g), None) => g,
        (None, Some(r)) => r,
        (None, None) => return WorkflowConfig::default(),
    };

    // 머지된 YAML Value → WorkflowConfig (serde(default)가 미지정 필드 채움)
    serde_json::from_value(merged).unwrap_or_default()
}

/// 글로벌 YAML을 raw JSON Value로 로드
fn load_raw_yaml_global(env: &dyn Env) -> Option<serde_json::Value> {
    let home = env.var("HOME").ok()?;
    let path = Path::new(&home).join(CONFIG_FILENAME);
    load_raw_yaml(&path)
}

/// 디렉토리에서 YAML을 raw JSON Value로 로드
fn load_raw_yaml_from_dir(dir: &Path) -> Option<serde_json::Value> {
    let path = dir.join(CONFIG_FILENAME);
    load_raw_yaml(&path)
}

/// YAML 파일 → serde_json::Value (struct가 아닌 raw value)
/// 미지정 필드는 Value에 존재하지 않아 머지 시 base를 보존
fn load_raw_yaml(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_yaml::from_str(&content).ok()
}

/// JSON Value 딥머지: over에 명시적으로 존재하는 값만 base를 덮어씀
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
pub fn global_config_path(env: &dyn Env) -> std::path::PathBuf {
    let home = env.var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(CONFIG_FILENAME)
}

/// 글로벌 설정 파일 초기화 (setup 시 사용)
pub fn init_global(env: &dyn Env, config: &WorkflowConfig) -> Result<()> {
    let path = global_config_path(env);
    let yaml = serde_yaml::to_string(config)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}
