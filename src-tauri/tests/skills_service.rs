use cc_switch_lib::SkillService;
use serde_json::json;

#[path = "support.rs"]
mod support;
use support::{ensure_test_home, reset_test_fs, test_mutex};

fn write_skill_md(dir: &std::path::Path, name: &str, description: &str) {
    std::fs::create_dir_all(dir).expect("create skill dir");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .expect("write SKILL.md");
}

#[test]
fn list_installed_triggers_initial_ssot_migration() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let claude_skill_dir = home.join(".claude").join("skills").join("hello-skill");
    write_skill_md(&claude_skill_dir, "Hello Skill", "A test skill");

    let installed = SkillService::list_installed().expect("list installed");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].directory, "hello-skill");
    assert!(
        installed[0].apps.claude,
        "skill should be enabled for claude"
    );

    let ssot_skill_dir = home.join(".cc-switch").join("skills").join("hello-skill");
    assert!(
        ssot_skill_dir.exists(),
        "SSOT directory should be created and populated"
    );

    let index_path = home.join(".cc-switch").join("skills.json");
    let index_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index_path).expect("read skills.json"))
            .expect("parse skills.json");
    assert_eq!(index_value.get("version").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        index_value
            .get("ssotMigrationPending")
            .and_then(|v| v.as_bool()),
        Some(false),
        "migration flag should be cleared after import"
    );
    assert!(
        index_value
            .get("skills")
            .and_then(|v| v.get("hello-skill"))
            .is_some(),
        "skills.json should contain imported record"
    );
}

#[test]
fn pending_migration_with_existing_managed_list_does_not_claim_unmanaged_skills() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Two skills exist in the app dir.
    let claude_dir = home.join(".claude").join("skills");
    write_skill_md(
        &claude_dir.join("managed-skill"),
        "Managed Skill",
        "Managed",
    );
    write_skill_md(
        &claude_dir.join("unmanaged-skill"),
        "Unmanaged Skill",
        "Unmanaged",
    );

    // Pre-seed skills.json with a managed list containing only "managed-skill" and migration pending.
    let index_path = home.join(".cc-switch").join("skills.json");
    std::fs::create_dir_all(index_path.parent().expect("parent")).expect("create .cc-switch");

    let seeded = json!({
        "version": 1,
        "syncMethod": "auto",
        "ssotMigrationPending": true,
        "skills": {
            "managed-skill": {
                "id": "local:managed-skill",
                "name": "managed-skill",
                "directory": "managed-skill",
                "apps": { "claude": true, "codex": false, "gemini": false },
                "installedAt": 1
            }
        }
    });
    std::fs::write(&index_path, serde_json::to_string_pretty(&seeded).unwrap())
        .expect("write seeded skills.json");

    // Calling list_installed should perform best-effort SSOT copy for the managed skill,
    // without auto-importing all app dir skills into the managed list.
    let installed = SkillService::list_installed().expect("list installed");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].directory, "managed-skill");

    let ssot_dir = home.join(".cc-switch").join("skills");
    assert!(
        ssot_dir.join("managed-skill").exists(),
        "managed skill should be copied into SSOT"
    );
    assert!(
        !ssot_dir.join("unmanaged-skill").exists(),
        "unmanaged skill should NOT be claimed/copied during pending migration when managed list is non-empty"
    );

    let index_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index_path).expect("read skills.json"))
            .expect("parse skills.json");
    assert_eq!(
        index_value
            .get("ssotMigrationPending")
            .and_then(|v| v.as_bool()),
        Some(false),
        "migration flag should be cleared after best-effort copy"
    );
    assert!(
        index_value
            .get("skills")
            .and_then(|v| v.get("unmanaged-skill"))
            .is_none(),
        "unmanaged skill should remain unmanaged (not added to index)"
    );
}
