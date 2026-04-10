//! End-to-end test for the `read_skill` agent tool, mirroring the live
//! gateway's skill-tool registration.
//!
//! The gateway registers `ReadSkillTool` alongside the other skill management
//! tools in `server.rs`. This test bootstraps a matching `ToolRegistry`
//! independently (using the same discoverer type the gateway does), seeds a
//! temporary skill tree, and exercises the registered tool end-to-end:
//!
//!   1. `read_skill` is present in the registry's tool list.
//!   2. `{ name: "test-skill" }` returns the seeded body and lists sidecars.
//!   3. `{ name: "test-skill", file_path: "references/notes.md" }` returns
//!      the sidecar content.
//!   4. An unknown skill name returns a friendly error.
//!   5. A path-traversal `file_path` is rejected.
//!
//! This is the test that would have caught the original bug where the model
//! silently fell back to a filesystem MCP server (or fabricated a body) when
//! no native read tool was registered.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{path::Path, sync::Arc};

use {
    moltis_agents::tool_registry::ToolRegistry,
    moltis_skills::{
        discover::{FsSkillDiscoverer, SkillDiscoverer},
        types::SkillSource,
    },
    serde_json::json,
    tempfile::TempDir,
};

/// Seed a personal-source skill at `<data_dir>/skills/<name>` with a known
/// SKILL.md body, and create a `references/notes.md` sidecar. Returns the
/// `(data_dir guard, skill_dir_path)` pair.
fn seed_test_skill(name: &str, body: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: an integration-test skill\n---\n{body}"),
    )
    .unwrap();
    std::fs::create_dir_all(skill_dir.join("references")).unwrap();
    std::fs::write(
        skill_dir.join("references/notes.md"),
        "# Notes\n\nSome reference material.\n",
    )
    .unwrap();
    (tmp, skill_dir)
}

/// Build a `ToolRegistry` populated the same way `server.rs` does for
/// skill-management tools, but scoped to a single temporary data directory.
fn registry_for(data_dir: &Path) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Mirror the gateway's skill tool registration block from `server.rs`.
    registry.register(Box::new(moltis_tools::skill_tools::CreateSkillTool::new(
        data_dir.to_path_buf(),
    )));
    registry.register(Box::new(moltis_tools::skill_tools::UpdateSkillTool::new(
        data_dir.to_path_buf(),
    )));
    registry.register(Box::new(moltis_tools::skill_tools::DeleteSkillTool::new(
        data_dir.to_path_buf(),
    )));

    // `ReadSkillTool` uses a discoverer pointed at the same personal-skills
    // directory the other tools write to. This intentionally does not include
    // the global default paths — the test must be hermetic.
    let discoverer: Arc<dyn SkillDiscoverer> = Arc::new(FsSkillDiscoverer::new(vec![(
        data_dir.join("skills"),
        SkillSource::Personal,
    )]));
    registry.register(Box::new(moltis_tools::skill_tools::ReadSkillTool::new(
        discoverer,
    )));

    registry
}

#[tokio::test]
async fn read_skill_tool_is_registered() {
    let (tmp, _skill_dir) = seed_test_skill("test-skill", "# Test\nHello.\n");
    let registry = registry_for(tmp.path());
    let names = registry.list_names();
    assert!(
        names.iter().any(|name| name == "read_skill"),
        "read_skill must be registered alongside the other skill tools, got: {names:?}"
    );
    // Sanity check: the create/update/delete tools are still there too, so
    // the registration block is intact.
    assert!(names.iter().any(|name| name == "create_skill"));
    assert!(names.iter().any(|name| name == "update_skill"));
    assert!(names.iter().any(|name| name == "delete_skill"));
}

#[tokio::test]
async fn read_skill_primary_call_returns_body_and_sidecar_listing() {
    let (tmp, _skill_dir) = seed_test_skill("test-skill", "# Test\n\nHello world.\n");
    let registry = registry_for(tmp.path());
    let tool = registry
        .get("read_skill")
        .expect("read_skill is registered");

    let result = tool
        .execute(json!({ "name": "test-skill" }))
        .await
        .expect("reading a seeded skill must succeed");

    assert_eq!(result["name"], "test-skill");
    assert_eq!(result["source"], "personal");
    assert_eq!(result["description"], "an integration-test skill");
    let body = result["body"].as_str().expect("body is a string");
    assert!(
        body.contains("Hello world"),
        "body should contain seeded text: {body:?}"
    );

    let linked: Vec<String> = result["linked_files"]
        .as_array()
        .expect("linked_files is an array")
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_string())
        .collect();
    assert!(
        linked.contains(&"references/notes.md".to_string()),
        "linked_files should list the seeded sidecar: {linked:?}"
    );

    // The serialized response must not leak the absolute tmp path.
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(
        !serialized.contains(tmp.path().to_string_lossy().as_ref()),
        "response must not leak the absolute data_dir path: {serialized}"
    );
}

#[tokio::test]
async fn read_skill_sidecar_call_returns_file_content() {
    let (tmp, _skill_dir) = seed_test_skill("test-skill", "# Test\n");
    let registry = registry_for(tmp.path());
    let tool = registry
        .get("read_skill")
        .expect("read_skill is registered");

    let result = tool
        .execute(json!({
            "name": "test-skill",
            "file_path": "references/notes.md"
        }))
        .await
        .expect("reading the sidecar must succeed");

    assert_eq!(result["name"], "test-skill");
    assert_eq!(result["file_path"], "references/notes.md");
    assert_eq!(
        result["content"].as_str().unwrap(),
        "# Notes\n\nSome reference material.\n"
    );
}

#[tokio::test]
async fn read_skill_unknown_name_returns_error() {
    let (tmp, _skill_dir) = seed_test_skill("test-skill", "# Test\n");
    let registry = registry_for(tmp.path());
    let tool = registry
        .get("read_skill")
        .expect("read_skill is registered");

    let result = tool.execute(json!({ "name": "does-not-exist" })).await;
    let err = result.expect_err("unknown skill name must return an error");
    let msg = format!("{err}");
    assert!(
        msg.contains("'does-not-exist'"),
        "error should name the skill: {msg}"
    );
    assert!(
        msg.contains("test-skill"),
        "error should hint at available names: {msg}"
    );
}

#[tokio::test]
async fn read_skill_rejects_path_traversal_in_file_path() {
    let (tmp, _skill_dir) = seed_test_skill("test-skill", "# Test\n");
    // Create a sibling file that traversal would try to reach.
    std::fs::write(tmp.path().join("skills/secret.txt"), "top secret\n").unwrap();
    let registry = registry_for(tmp.path());
    let tool = registry
        .get("read_skill")
        .expect("read_skill is registered");

    let result = tool
        .execute(json!({
            "name": "test-skill",
            "file_path": "../../etc/passwd"
        }))
        .await;
    assert!(result.is_err(), "path traversal must be rejected");
}
