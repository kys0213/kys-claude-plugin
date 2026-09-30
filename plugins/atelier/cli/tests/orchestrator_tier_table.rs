use atelier::orchestrator::core::tier::Tier;

const SKILL_MD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../skills/orchestrator/SKILL.md"
);
const DRIFT: &str = "SKILL.md 표 1 과 CLI tier 사다리가 어긋났다";

fn tier_of(word: &str) -> Option<Tier> {
    match word {
        "Fable" => Some(Tier::Fable),
        "Opus" => Some(Tier::Opus),
        "Sonnet" => Some(Tier::Sonnet),
        "Haiku" => Some(Tier::Haiku),
        _ => None,
    }
}

fn parse_cap_table(md: &str) -> Result<Vec<(Tier, Tier)>, String> {
    let mut in_section = false;
    let mut in_table = false;
    let mut rows = Vec::new();
    for line in md.lines() {
        if line.starts_with("### 표 1") {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if line.starts_with("## ") || line.starts_with("### ") {
            break;
        }
        let trimmed = line.trim();
        if !in_table {
            in_table = trimmed.starts_with('|')
                && trimmed.contains("메인 모델")
                && trimmed.contains("집행 위임 상한");
            continue;
        }
        if !trimmed.starts_with('|') {
            break;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells
            .iter()
            .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        let [main_cell, cap_cell, ..] = cells[..] else {
            return Err(format!("행을 해석할 수 없다: {line}"));
        };
        let mains: Vec<Tier> = match main_cell {
            "Sonnet 이하" => vec![Tier::Sonnet, Tier::Haiku],
            other => vec![tier_of(other).ok_or_else(|| format!("메인 모델 셀 해석 불가: {line}"))?],
        };
        for main in mains {
            let cap = if cap_cell.starts_with("메인과 같은 모델") {
                main
            } else {
                let first = cap_cell.split_whitespace().next().unwrap_or("");
                tier_of(first).ok_or_else(|| format!("상한 셀 해석 불가: {line}"))?
            };
            rows.push((main, cap));
        }
    }
    if !in_section {
        return Err("`### 표 1` 절을 찾지 못했다".to_string());
    }
    for tier in [Tier::Fable, Tier::Opus, Tier::Sonnet, Tier::Haiku] {
        if !rows.iter().any(|(main, _)| *main == tier) {
            return Err(format!("{tier:?} 행을 표에서 찾지 못했다"));
        }
    }
    Ok(rows)
}

fn check_table(md: &str) -> Result<(), String> {
    for (main, cap) in parse_cap_table(md)? {
        if main.execution_cap() != cap {
            return Err(format!(
                "{DRIFT}: 메인 {main:?} 의 상한이 표는 {cap:?}, CLI 는 {:?}",
                main.execution_cap()
            ));
        }
    }
    Ok(())
}

const GOOD: &str = "### 표 1 — 집행 tier\n\n| 메인 모델 | 집행 위임 상한 (최상위) | 자문 소집 |\n|---|---|---|\n| Fable | Opus — 집행에 쓰지 않는다 | Fable |\n| Opus | Opus | Fable |\n| Sonnet 이하 | 메인과 같은 모델 | 한 단 위 |\n\n### 다음\n";

#[test]
fn skill_md_table_matches_tier_ladder() {
    let md = std::fs::read_to_string(SKILL_MD).expect("read orchestrator SKILL.md");
    if let Err(e) = check_table(&md) {
        panic!("{DRIFT}: {e}");
    }
}

#[test]
fn well_formed_table_passes() {
    assert_eq!(check_table(GOOD), Ok(()));
}

#[test]
fn drifted_opus_row_fails() {
    let md = GOOD.replace("| Opus | Opus |", "| Opus | Sonnet |");
    let err = check_table(&md).unwrap_err();
    assert!(err.contains(DRIFT), "{err}");
}

#[test]
fn drifted_fable_row_fails() {
    let md = GOOD.replace("| Fable | Opus —", "| Fable | Fable —");
    assert!(check_table(&md).unwrap_err().contains(DRIFT));
}

#[test]
fn drifted_sonnet_row_fails() {
    let md = GOOD.replace("메인과 같은 모델", "Opus");
    assert!(check_table(&md).unwrap_err().contains(DRIFT));
}

#[test]
fn missing_section_fails() {
    assert!(check_table("# nothing here\n").is_err());
}

#[test]
fn missing_table_fails() {
    assert!(check_table("### 표 1\n\ntext only\n\n### 표 2\n").is_err());
}

#[test]
fn unparseable_row_fails() {
    let md = GOOD.replace("| Opus | Opus |", "| Opus | ??? |");
    assert!(check_table(&md).is_err());
}

#[test]
fn missing_row_fails() {
    let md = GOOD.replace("| Opus | Opus | Fable |\n", "");
    assert!(check_table(&md).is_err());
}
