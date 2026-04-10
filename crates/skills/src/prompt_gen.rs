use crate::types::{SkillMetadata, SkillSource};

/// Generate the `<available_skills>` XML block for injection into the system prompt.
///
/// The block lists each enabled skill's name, source, and description. It
/// deliberately does **not** include the absolute `SKILL.md` path: the model
/// should activate a skill by calling the native `read_skill` tool with the
/// skill name, which resolves through the same discoverer the prompt block
/// was built from.
pub fn generate_skills_prompt(skills: &[SkillMetadata]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Available Skills\n\n<available_skills>\n");
    for skill in skills {
        let source = if skill.source.as_ref() == Some(&SkillSource::Plugin) {
            "plugin"
        } else {
            "skill"
        };
        out.push_str(&format!(
            "<skill name=\"{}\" source=\"{}\">\n{}\n</skill>\n",
            skill.name, source, skill.description,
        ));
    }
    out.push_str("</available_skills>\n\n");
    out.push_str(
        "To activate a skill, call the `read_skill` tool with its name \
         (e.g. `read_skill(name=\"<skill-name>\")`). To load a sidecar file \
         inside a skill directory (references/, templates/, scripts/), pass \
         the `file_path` argument as well \
         (e.g. `read_skill(name=\"<skill-name>\", file_path=\"references/api.md\")`).\n\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_empty_skills_produces_empty_string() {
        assert_eq!(generate_skills_prompt(&[]), "");
    }

    #[test]
    fn test_single_skill_prompt() {
        let skills = vec![SkillMetadata {
            name: "commit".into(),
            description: "Create git commits".into(),
            path: PathBuf::from("/home/user/.moltis/skills/commit"),
            source: Some(SkillSource::Personal),
            ..Default::default()
        }];
        let prompt = generate_skills_prompt(&skills);
        assert!(prompt.contains("<available_skills>"));
        assert!(prompt.contains("name=\"commit\""));
        assert!(prompt.contains("source=\"skill\""));
        assert!(prompt.contains("Create git commits"));
        assert!(prompt.contains("</available_skills>"));
        assert!(
            prompt.contains("read_skill"),
            "activation instruction should name the read_skill tool"
        );
    }

    #[test]
    fn test_prompt_does_not_leak_absolute_paths() {
        // The prompt must never include absolute paths — that was the bug.
        let skills = vec![SkillMetadata {
            name: "demo".into(),
            description: "A demo skill".into(),
            path: PathBuf::from("/home/secretuser/.moltis/skills/demo"),
            source: Some(SkillSource::Personal),
            ..Default::default()
        }];
        let prompt = generate_skills_prompt(&skills);
        assert!(
            !prompt.contains("/home/secretuser"),
            "prompt leaked absolute path: {prompt}"
        );
        assert!(
            !prompt.contains("SKILL.md"),
            "prompt should no longer mention SKILL.md: {prompt}"
        );
        // The <skill> element must not carry a path= attribute. (The
        // activation instruction still mentions `file_path=` for sidecar
        // reads, which is fine — we're checking the element itself.)
        assert!(
            !prompt.contains("<skill ") || !prompt.contains("\" path=\""),
            "prompt should not include a path= attribute on the <skill> element"
        );
    }

    #[test]
    fn test_plugin_source_is_labelled_as_plugin() {
        let skills = vec![SkillMetadata {
            name: "plugin-helper".into(),
            description: "Helper plugin".into(),
            path: PathBuf::from("/opt/plugins/helper.md"),
            source: Some(SkillSource::Plugin),
            ..Default::default()
        }];
        let prompt = generate_skills_prompt(&skills);
        assert!(prompt.contains("source=\"plugin\""));
        assert!(!prompt.contains("/opt/plugins"));
    }

    #[test]
    fn test_multiple_skills() {
        let skills = vec![
            SkillMetadata {
                name: "commit".into(),
                description: "Commits".into(),
                path: PathBuf::from("/a"),
                source: Some(SkillSource::Personal),
                ..Default::default()
            },
            SkillMetadata {
                name: "review".into(),
                description: "Reviews".into(),
                path: PathBuf::from("/b"),
                source: Some(SkillSource::Personal),
                ..Default::default()
            },
        ];
        let prompt = generate_skills_prompt(&skills);
        assert!(prompt.contains("name=\"commit\""));
        assert!(prompt.contains("name=\"review\""));
        // The activation instruction (which mentions `read_skill`) is emitted
        // once, not per-skill, so the match count should not grow with the
        // number of skills.
        let single_skill_prompt = generate_skills_prompt(&skills[..1]);
        assert_eq!(
            prompt.matches("read_skill").count(),
            single_skill_prompt.matches("read_skill").count()
        );
    }
}
