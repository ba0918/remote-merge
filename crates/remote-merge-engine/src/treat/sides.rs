//! 比べる側の決め方（docs/ir/cli/status-targets.md の TBL-cli-004、sync.md の TBL-cli-006、merge.md の
//! TBL-cli-008、reference.md の REQ-cli-062・066、status-targets.md の REQ-cli-037、merge.md の
//! REQ-cli-050）。`servers` は設定のサーバの名前で、既定サーバは名前順で最初のもの（docs/ir/CONTEXT.md）

use crate::side::Side;

/// 指定の誤りの文（そのまま利用者に示す）
pub type SidesError = String;

/// 名前を比べる側にする。"local" はローカル、ほかは設定にあるサーバの名前でなければ誤り
fn known(name: &str, servers: &[String]) -> Result<Side, SidesError> {
    match Side::new(name) {
        Side::Local => Ok(Side::Local),
        Side::Remote(server) if servers.contains(&server) => Ok(Side::Remote(server)),
        Side::Remote(server) => Err(format!("Server '{server}' not found in config")),
    }
}

fn default_server(servers: &[String]) -> Result<Side, SidesError> {
    let mut names: Vec<&String> = servers.iter().collect();
    names.sort();
    names
        .first()
        .map(|name| Side::Remote(name.to_string()))
        .ok_or_else(|| {
            "no server is configured, so the default server cannot be used as a side".to_string()
        })
}

fn different(left: &Side, right: &Side) -> Result<(), SidesError> {
    if left == right {
        return Err(format!(
            "--left and --right must be different (both resolved to '{}')",
            left.display_name()
        ));
    }
    Ok(())
}

/// status と diff の左右（TBL-cli-004）。左がなければ "local"、右がなければ既定サーバ
pub fn compared_sides(
    left: Option<&str>,
    right: Option<&str>,
    servers: &[String],
) -> Result<(Side, Side), SidesError> {
    let left_side = match left {
        Some(name) => known(name, servers)?,
        None => Side::Local,
    };
    let right_side = match right {
        Some(name) => known(name, servers)?,
        None => default_server(servers)?,
    };
    if right.is_none() && left_side == right_side {
        return Err(format!(
            "--left and --right must be different (both resolved to '{}'; --right was not given \
             and the default server is '{}')",
            left_side.display_name(),
            right_side.display_name()
        ));
    }
    different(&left_side, &right_side)?;
    Ok((left_side, right_side))
}

/// merge の読み込み元と書き込み先（TBL-cli-008）。どちらも明示する
pub fn merge_sides(
    left: Option<&str>,
    right: Option<&str>,
    servers: &[String],
) -> Result<(Side, Side), SidesError> {
    let (Some(left), Some(right)) = (left, right) else {
        return Err(
            "--left and --right are required for merge command (e.g. --left local --right staging)"
                .to_string(),
        );
    };
    let (left, right) = (known(left, servers)?, known(right, servers)?);
    different(&left, &right)?;
    Ok((left, right))
}

/// sync の読み込み元と、指定した順の書き込み先（TBL-cli-006）
pub fn sync_sides(
    left: Option<&str>,
    right: &[String],
    servers: &[String],
) -> Result<(Side, Vec<Side>), SidesError> {
    let Some(left) = left else {
        return Err("--left is required for sync command".to_string());
    };
    if right.is_empty() {
        return Err("--right requires at least one target server for sync command".to_string());
    }
    let source = known(left, servers)?;
    let mut targets: Vec<Side> = Vec::new();
    for name in right {
        let target = known(name, servers)?;
        if targets.contains(&target) {
            return Err(format!(
                "Duplicate target server: {}",
                target.display_name()
            ));
        }
        different(&source, &target)?;
        targets.push(target);
    }
    Ok((source, targets))
}

/// 参照先の扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    None,
    Use(Side),
    /// 左右と同じなので使わない。値は標準エラーに出す警告
    Skipped(String),
}

/// --ref の参照先。左右と同じなら警告して使わない
pub fn reference_side(
    reference: Option<&str>,
    sides: &(Side, Side),
    servers: &[String],
) -> Result<Reference, SidesError> {
    let Some(name) = reference else {
        return Ok(Reference::None);
    };
    let side = known(name, servers)?;
    let skipped = |which: &str| {
        Reference::Skipped(format!(
            "Warning: --ref server is the same as {which} side; ref comparison skipped."
        ))
    };
    Ok(if side == sides.0 {
        skipped("left")
    } else if side == sides.1 {
        skipped("right")
    } else {
        Reference::Use(side)
    })
}

#[cfg(test)]
#[path = "sides_tests.rs"]
mod tests;
