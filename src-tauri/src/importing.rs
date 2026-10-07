//! 三种导入先生成完整候选配置，全部验证成功后才在一次事务中提交。
use crate::{
    db::models::*,
    error::{AppError, AppResult},
};
pub fn plan(
    mut current: ExportBundle,
    mut incoming: ExportBundle,
    mode: ImportMode,
) -> AppResult<(ExportBundle, ImportReport)> {
    let mut report = ImportReport {
        groups_added: 0,
        groups_updated: 0,
        presets_added: 0,
        presets_updated: 0,
        workflows_added: 0,
        schedules_added: 0,
        warnings: vec![],
        settings_imported: mode == ImportMode::Replace,
    };
    if mode == ImportMode::Replace {
        current.groups.clear();
        current.presets.clear();
        current.workflows.clear();
        current.schedules.clear();
        current.settings = incoming.settings.clone();
        current.productivity = incoming.productivity.clone();
    }
    macro_rules! merge {
        ($field:ident,$added:ident,$updated:ident) => {
            for mut item in incoming.$field.drain(..) {
                if item.id.trim().is_empty() {
                    item.id = new_id();
                }
                if let Some(index) = current.$field.iter().position(|p| p.id == item.id) {
                    if mode != ImportMode::Append {
                        current.$field[index] = item;
                        report.$updated += 1;
                    }
                } else {
                    current.$field.push(item);
                    report.$added += 1;
                }
            }
        };
    }
    // 先校验全部名称，避免覆盖导入在清空后才发现坏记录。
    for p in &incoming.presets {
        crate::db::presets::validate(p)?;
    }
    for g in &incoming.groups {
        if g.name.trim().is_empty() {
            return Err(AppError::validation("导入分组名称不能为空"));
        }
    }
    merge!(groups, groups_added, groups_updated);
    for p in &mut incoming.presets {
        if !p.group_id.is_empty() && !current.groups.iter().any(|g| g.id == p.group_id) {
            report
                .warnings
                .push(format!("预设「{}」所属分组不存在，已放入未分组", p.name));
            p.group_id.clear();
        }
    }
    merge!(presets, presets_added, presets_updated);
    // 自动化记录不单列更新数；复用临时计数，不污染预设更新报告。
    let count = report.presets_updated;
    merge!(workflows, workflows_added, presets_updated);
    merge!(schedules, schedules_added, presets_updated);
    report.presets_updated = count;
    if mode != ImportMode::Replace {
        report
            .warnings
            .push("合并/追加模式不会导入设置，当前设置已保留".into());
    }
    Ok((current, report))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bundle() -> ExportBundle {
        ExportBundle {
            version: "2".into(),
            exported_at: 0,
            app_version: "test".into(),
            settings: Default::default(),
            groups: vec![],
            presets: vec![],
            workflows: vec![],
            schedules: vec![],
            productivity: Default::default(),
        }
    }
    #[test]
    fn invalid_import_keeps_original_data() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let old = crate::db::presets::save(&db, &Preset::default()).unwrap();
        let mut before = bundle();
        before.presets.push(old.clone());
        let mut bad = bundle();
        let mut p = old.clone();
        p.name = "x".repeat(101);
        bad.presets.push(p);
        assert!(plan(before, bad, ImportMode::Replace).is_err());
        assert_eq!(
            crate::db::presets::get(&db, &old.id).unwrap().name,
            old.name
        );
    }
    #[test]
    fn merge_and_append_preserve_settings_and_existing_records() {
        let mut current = bundle();
        let mut p = Preset::default();
        p.id = "one".into();
        p.name = "旧值".into();
        current.presets.push(p.clone());
        let mut incoming = bundle();
        p.name = "新值".into();
        incoming.presets.push(p);
        let (a, r) = plan(current.clone(), incoming.clone(), ImportMode::Append).unwrap();
        assert_eq!(a.presets[0].name, "旧值");
        assert_eq!(r.presets_added, 0);
        let (b, r) = plan(current, incoming, ImportMode::Merge).unwrap();
        assert_eq!(b.presets[0].name, "新值");
        assert_eq!(r.presets_updated, 1);
    }
}
