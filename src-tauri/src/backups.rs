//! 本机配置快照（最多 30 份），不备份终端输出；恢复前先保留当前配置。
use crate::{
    db::{self, models::*},
    error::{AppError, AppResult},
    state::AppState,
};
use serde::Serialize;
use std::io::Write;
pub fn bundle(state: &AppState) -> AppResult<ExportBundle> {
    Ok(ExportBundle {
        version: "2".into(),
        exported_at: now_ms(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings: state.settings(),
        groups: db::groups::list(&state.db)?,
        presets: db::presets::list(
            &state.db,
            &PresetFilter {
                include_hidden: true,
                ..Default::default()
            },
        )?,
        workflows: db::workflows::list(&state.db)?,
        schedules: db::schedules::list(&state.db)?,
        productivity: crate::productivity::load(&state.db)?,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub created_at: i64,
    pub bytes: u64,
}
pub fn list(state: &AppState) -> AppResult<Vec<BackupInfo>> {
    let dir = state.sub_dir("backups")?;
    let mut result = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&name) || !entry.file_type()?.is_file() {
            continue;
        }
        result.push(BackupInfo {
            id: name.clone(),
            created_at: name
                .split('-')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            bytes: entry.metadata()?.len(),
        });
    }
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}
fn valid_id(id: &str) -> bool {
    id.ends_with(".json")
        && id.len() < 100
        && id
            .strip_suffix(".json")
            .unwrap_or("")
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
        && id
            .split('-')
            .next()
            .is_some_and(|s| s.len() == 13 && s.chars().all(|c| c.is_ascii_digit()))
}
pub fn path(state: &AppState, id: &str) -> AppResult<std::path::PathBuf> {
    if !valid_id(id) {
        return Err(AppError::validation("备份 ID 无效"));
    }
    Ok(state.sub_dir("backups")?.join(id))
}
pub fn snapshot(state: &AppState) -> AppResult<String> {
    let id = format!("{}-{}.json", now_ms(), new_id());
    let dest = path(state, &id)?;
    let data =
        serde_json::to_vec_pretty(&bundle(state)?).map_err(|e| AppError::other(e.to_string()))?;
    // 临时文件完整写入后再原子改名，列表不会显示半截备份。
    let temp = dest.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(&data)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temp, dest)?;
    for old in list(state)?.into_iter().skip(30) {
        std::fs::remove_file(path(state, &old.id)?)?;
    }
    Ok(id)
}
/// 先在独立内存库校验全部记录，再通过一次 SQLite 事务替换配置。
/// 原库的审计与历史输出不变；任何一条记录失败都不会留下半份恢复结果。
pub fn restore_into(db: &db::Db, bundle: &ExportBundle) -> AppResult<()> {
    crate::commands::system_cmds::validate_settings(&bundle.settings)?;
    crate::productivity::validate(&bundle.productivity)?;
    let stage = db::Db::open_in_memory()?;
    for g in &bundle.groups {
        db::groups::save(&stage, g)?;
    }
    for p in &bundle.presets {
        if !p.group_id.is_empty() && !bundle.groups.iter().any(|g| g.id == p.group_id) {
            return Err(AppError::validation("备份中有预设引用不存在的分组"));
        }
        db::presets::save(&stage, p)?;
    }
    for w in &bundle.workflows {
        db::workflows::save(&stage, w)?;
    }
    for s in &bundle.schedules {
        db::schedules::save(&stage, s)?;
    }
    db::settings::save(&stage, &bundle.settings)?;
    crate::productivity::save(&stage, &bundle.productivity)?;
    let source = stage.conn();
    let mut target = db.conn();
    let tx = target.transaction()?;
    tx.execute_batch("DELETE FROM workflow_runs; DELETE FROM presets; DELETE FROM groups; DELETE FROM workflows; DELETE FROM schedules; DELETE FROM settings WHERE key IN ('app','productivity');")?;
    for table in ["groups", "presets", "workflows", "schedules", "settings"] {
        let mut columns = source.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = columns
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut read = source.prepare(&format!("SELECT * FROM {table}"))?;
        let count = names.len();
        let mut write = tx.prepare(&format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            names.join(","),
            vec!["?"; count].join(",")
        ))?;
        let mut rows = read.query([])?;
        while let Some(row) = rows.next()? {
            let values = (0..count)
                .map(|i| row.get::<_, rusqlite::types::Value>(i))
                .collect::<Result<Vec<_>, _>>()?;
            write.execute(rusqlite::params_from_iter(values))?;
        }
    }
    tx.commit()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    fn bundle(presets: Vec<crate::db::models::Preset>) -> crate::db::models::ExportBundle {
        crate::db::models::ExportBundle {
            version: "2".into(),
            exported_at: 0,
            app_version: "test".into(),
            settings: Default::default(),
            groups: vec![],
            presets,
            workflows: vec![],
            schedules: vec![],
            productivity: Default::default(),
        }
    }
    #[test]
    fn restore_transaction_rolls_back_after_insert_failure() {
        use crate::db::{self, models::Preset, Db};
        let db = Db::open_in_memory().unwrap();
        let mut old = Preset::default();
        old.name = "原配置".into();
        let old = db::presets::save(&db, &old).unwrap();
        db.conn().execute_batch("CREATE TRIGGER reject_restore BEFORE INSERT ON presets BEGIN SELECT RAISE(ABORT,'restore failure'); END;").unwrap();
        let mut incoming = old.clone();
        incoming.name = "新配置".into();
        assert!(super::restore_into(&db, &bundle(vec![incoming])).is_err());
        assert_eq!(db::presets::get(&db, &old.id).unwrap().name, "原配置");
    }
    #[test]
    fn restores_configuration_without_erasing_history() {
        use crate::db::{self, models::Preset, Db};
        let db = Db::open_in_memory().unwrap();
        let mut incoming = Preset::default();
        incoming.id = "restored".into();
        incoming.name = "恢复预设".into();
        super::restore_into(&db, &bundle(vec![incoming])).unwrap();
        assert_eq!(db::presets::get(&db, "restored").unwrap().name, "恢复预设");
        assert_eq!(db::settings::load(&db).unwrap().font_size, 14);
    }
    #[test]
    fn rejects_path_traversal() {
        assert!(!super::valid_id("../../settings.json"));
        assert!(!super::valid_id("1700000000000-abc.json/other"));
        assert!(super::valid_id("1700000000000-abcd.json"));
    }
}
