use std::collections::HashSet;

use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
use crate::platforms::openai::codex::team_profiles::{
    normalize_member_code, team_profile_presets,
};

pub(crate) fn generate_antigravity_gateway_api_key(member_code: &str) -> String {
    let normalized = normalize_member_code(member_code).unwrap_or_else(|| "member".to_string());
    let random = uuid::Uuid::new_v4().simple().to_string();
    format!("sk-ant-{}-{}", normalized, &random[..8])
}

pub(crate) fn import_antigravity_team_template_into_profiles(
    mut profiles: GatewayAccessProfiles,
) -> GatewayAccessProfiles {
    for preset in team_profile_presets() {
        let preset_member_code = normalize_member_code(preset.member_code)
            .unwrap_or_else(|| preset.member_code.to_string());
        let matching_indices: Vec<usize> = profiles
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| {
                profile.target == GatewayTarget::Antigravity
                    && profile
                        .member_code
                        .as_deref()
                        .and_then(normalize_member_code)
                        == Some(preset_member_code.clone())
            })
            .map(|(index, _)| index)
            .collect();

        if let Some((primary_index, duplicate_indices)) = matching_indices.split_first() {
            let duplicate_profiles: Vec<GatewayAccessProfile> = duplicate_indices
                .iter()
                .map(|index| profiles.profiles[*index].clone())
                .collect();

            {
                let existing = &mut profiles.profiles[*primary_index];
                existing.name = preset.name.to_string();
                existing.member_code = Some(preset.member_code.to_string());
                existing.role_title = Some(preset.role_title.to_string());
                existing.persona_summary = Some(preset.persona_summary.to_string());
                existing.color = Some(preset.color.to_string());

                if existing.api_key.trim().is_empty() {
                    existing.api_key = duplicate_profiles
                        .iter()
                        .find_map(|profile| {
                            let trimmed = profile.api_key.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            generate_antigravity_gateway_api_key(preset.member_code)
                        });
                }

                let existing_notes_empty = existing
                    .notes
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty();
                if existing_notes_empty {
                    existing.notes = duplicate_profiles.iter().find_map(|profile| {
                        profile.notes.as_ref().and_then(|notes| {
                            let trimmed = notes.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_string())
                            }
                        })
                    });
                }
            }

            if !duplicate_indices.is_empty() {
                let duplicate_ids: HashSet<String> = duplicate_indices
                    .iter()
                    .map(|index| profiles.profiles[*index].id.clone())
                    .collect();
                profiles
                    .profiles
                    .retain(|profile| !duplicate_ids.contains(&profile.id));
            }

            continue;
        }

        profiles.upsert_profile(GatewayAccessProfile {
            id: format!("antigravity-{}", preset_member_code),
            name: preset.name.to_string(),
            target: GatewayTarget::Antigravity,
            api_key: generate_antigravity_gateway_api_key(preset.member_code),
            enabled: true,
            member_code: Some(preset.member_code.to_string()),
            role_title: Some(preset.role_title.to_string()),
            persona_summary: Some(preset.persona_summary.to_string()),
            color: Some(preset.color.to_string()),
            notes: None,
        });
    }

    profiles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_antigravity_gateway_api_key_uses_member_code_prefix() {
        let key = generate_antigravity_gateway_api_key("jdd");

        assert!(key.starts_with("sk-ant-jdd-"));
        assert_eq!(key.len(), "sk-ant-jdd-".len() + 8);
    }

    #[test]
    fn generate_antigravity_gateway_api_key_normalizes_member_code() {
        let key = generate_antigravity_gateway_api_key(" J/D D ");

        assert!(key.starts_with("sk-ant-jdd-"));
        assert_eq!(key.len(), "sk-ant-jdd-".len() + 8);
    }

    #[test]
    fn import_antigravity_team_template_keeps_codex_profiles_untouched() {
        let profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-jdd".into(),
                name: "姜大大".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-team-jdd-existing".into(),
                enabled: true,
                member_code: Some("jdd".into()),
                role_title: Some("产品与方法论".into()),
                persona_summary: None,
                color: Some("#4c6ef5".into()),
                notes: None,
            }],
        };

        let imported = import_antigravity_team_template_into_profiles(profiles);

        assert_eq!(imported.list_by_target(GatewayTarget::Codex).len(), 1);

        let antigravity_profiles = imported.list_by_target(GatewayTarget::Antigravity);
        assert_eq!(antigravity_profiles.len(), 10);
        assert_eq!(
            antigravity_profiles
                .iter()
                .filter(|profile| profile.member_code.as_deref() == Some("jdd"))
                .count(),
            1
        );
        assert!(
            antigravity_profiles
                .iter()
                .all(|profile| profile.api_key.starts_with("sk-ant-"))
        );
    }
}
