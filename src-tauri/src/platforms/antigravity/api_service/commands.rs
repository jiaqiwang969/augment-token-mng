use uuid::Uuid;

use super::models::AntigravityGatewayProfileMutation;
use super::team_profiles::generate_antigravity_gateway_api_key;
use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
use crate::platforms::openai::codex::team_profiles::normalize_member_code;

const ANTIGRAVITY_GATEWAY_PROFILE_NAME_PREFIX: &str = "Antigravity Key";

fn normalize_gateway_api_key(api_key: Option<String>) -> Option<String> {
    api_key.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_gateway_field(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_gateway_profile_name(name: Option<String>) -> Option<String> {
    name.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_gateway_member_code(member_code: Option<String>) -> Option<String> {
    member_code.and_then(|value| normalize_member_code(&value))
}

fn default_antigravity_gateway_profile_name(profiles: &GatewayAccessProfiles) -> String {
    let next_index = profiles
        .list_by_target(GatewayTarget::Antigravity)
        .len()
        .saturating_add(1);
    format!("{} {}", ANTIGRAVITY_GATEWAY_PROFILE_NAME_PREFIX, next_index)
}

fn ensure_unique_antigravity_member_code(
    profiles: &GatewayAccessProfiles,
    member_code: Option<&str>,
    ignored_profile_id: Option<&str>,
) -> Result<Option<String>, String> {
    let normalized = member_code.and_then(normalize_member_code);
    let Some(expected) = normalized.clone() else {
        return Ok(None);
    };

    let has_conflict = profiles
        .list_by_target(GatewayTarget::Antigravity)
        .into_iter()
        .any(|profile| {
            ignored_profile_id != Some(profile.id.as_str())
                && profile
                    .member_code
                    .as_deref()
                    .and_then(normalize_member_code)
                    .as_deref()
                    == Some(expected.as_str())
        });

    if has_conflict {
        Err(format!(
            "Antigravity gateway member code already exists: {}",
            expected
        ))
    } else {
        Ok(Some(expected))
    }
}

fn generate_gateway_api_key_for_member_code(member_code: Option<&str>) -> String {
    match member_code.and_then(normalize_member_code) {
        Some(member_code) => generate_antigravity_gateway_api_key(&member_code),
        None => generate_antigravity_gateway_api_key("member"),
    }
}

pub(crate) fn create_antigravity_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    mutation: AntigravityGatewayProfileMutation,
) -> Result<GatewayAccessProfile, String> {
    let member_code =
        ensure_unique_antigravity_member_code(profiles, mutation.member_code.as_deref(), None)?;
    let entry = GatewayAccessProfile {
        id: format!("antigravity-{}", Uuid::new_v4().simple()),
        name: normalize_gateway_profile_name(mutation.name)
            .unwrap_or_else(|| default_antigravity_gateway_profile_name(profiles)),
        target: GatewayTarget::Antigravity,
        api_key: normalize_gateway_api_key(mutation.api_key)
            .unwrap_or_else(|| generate_gateway_api_key_for_member_code(member_code.as_deref())),
        enabled: mutation.enabled.unwrap_or(true),
        member_code,
        role_title: normalize_optional_gateway_field(mutation.role_title),
        persona_summary: normalize_optional_gateway_field(mutation.persona_summary),
        color: normalize_optional_gateway_field(mutation.color),
        notes: normalize_optional_gateway_field(mutation.notes),
    };

    profiles.upsert_profile(entry.clone());
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::antigravity::api_service::team_profiles::import_antigravity_team_template_into_profiles;

    #[test]
    fn create_antigravity_gateway_profile_preserves_member_metadata_and_uses_ant_key() {
        let mut profiles = GatewayAccessProfiles::default();

        let created = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
                name: Some("姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some(" J/D D ".into()),
                role_title: Some("产品与方法论".into()),
                persona_summary: Some("高频输出".into()),
                color: Some("#4c6ef5".into()),
                notes: Some("高频使用成员".into()),
            },
        )
        .unwrap();

        assert_eq!(created.target, GatewayTarget::Antigravity);
        assert_eq!(created.member_code.as_deref(), Some("jdd"));
        assert_eq!(created.role_title.as_deref(), Some("产品与方法论"));
        assert_eq!(created.persona_summary.as_deref(), Some("高频输出"));
        assert_eq!(created.color.as_deref(), Some("#4c6ef5"));
        assert_eq!(created.notes.as_deref(), Some("高频使用成员"));
        assert!(created.api_key.starts_with("sk-ant-jdd-"));
    }

    #[test]
    fn create_antigravity_gateway_profile_rejects_duplicate_member_code_inside_antigravity_target() {
        let mut profiles = import_antigravity_team_template_into_profiles(GatewayAccessProfiles::default());

        let error = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
                name: Some("重复姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some("JDD".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        )
        .unwrap_err();

        assert!(error.contains("member code"));
    }

    #[test]
    fn create_antigravity_gateway_profile_allows_member_code_used_by_codex_target() {
        let mut profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-jdd".into(),
                name: "姜大大".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-team-jdd-existing".into(),
                enabled: true,
                member_code: Some("jdd".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            }],
        };

        let created = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
                name: Some("姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some("jdd".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        )
        .unwrap();

        assert_eq!(created.member_code.as_deref(), Some("jdd"));
        assert_eq!(profiles.list_by_target(GatewayTarget::Codex).len(), 1);
        assert_eq!(profiles.list_by_target(GatewayTarget::Antigravity).len(), 1);
    }
}
