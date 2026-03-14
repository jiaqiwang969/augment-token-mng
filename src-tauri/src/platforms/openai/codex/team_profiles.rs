use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};

use super::commands::generate_team_gateway_api_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamProfilePreset {
    pub name: &'static str,
    pub member_code: &'static str,
    pub role_title: &'static str,
    pub persona_summary: &'static str,
    pub color: &'static str,
    pub sort_order: u8,
}

const TEAM_PROFILE_PRESETS: [TeamProfilePreset; 10] = [
    TeamProfilePreset {
        name: "姜大大",
        member_code: "jdd",
        role_title: "产品与方法论",
        persona_summary: "高频输出，偏产品与方法论视角，擅长比较工具优劣并推动落地。",
        color: "#4c6ef5",
        sort_order: 1,
    },
    TeamProfilePreset {
        name: "佳琪",
        member_code: "jqw",
        role_title: "架构与趋势",
        persona_summary: "偏架构与趋势判断，关注系统性、长期性和可扩展性。",
        color: "#0ea5e9",
        sort_order: 2,
    },
    TeamProfilePreset {
        name: "CR",
        member_code: "cr",
        role_title: "实验执行",
        persona_summary: "执行导向明显，偏实验型，强调并行推进和实操效率。",
        color: "#2f9e44",
        sort_order: 3,
    },
    TeamProfilePreset {
        name: "梁十本",
        member_code: "lsb",
        role_title: "流程落地",
        persona_summary: "问题驱动，善于追问细节，偏把想法变成流程。",
        color: "#f08c00",
        sort_order: 4,
    },
    TeamProfilePreset {
        name: "Will",
        member_code: "will",
        role_title: "节奏与执行",
        persona_summary: "实用主义和节奏管理，常把讨论拉回可执行层面。",
        color: "#f06595",
        sort_order: 5,
    },
    TeamProfilePreset {
        name: "CP 全栈负责人",
        member_code: "cp",
        role_title: "工程与产品",
        persona_summary: "工程与产品结合视角，关注集成、性能和可用性。",
        color: "#7c3aed",
        sort_order: 6,
    },
    TeamProfilePreset {
        name: "大栗子",
        member_code: "dlz",
        role_title: "风险与成本",
        persona_summary: "风险意识强，重视来源真实性与成本可控。",
        color: "#c2410c",
        sort_order: 7,
    },
    TeamProfilePreset {
        name: "Camper Wu",
        member_code: "cw",
        role_title: "务实反馈",
        persona_summary: "反馈直接，互动配合度高，偏务实沟通。",
        color: "#1d4ed8",
        sort_order: 8,
    },
    TeamProfilePreset {
        name: "小蒋",
        member_code: "xj",
        role_title: "趋势判断",
        persona_summary: "趋势敏感，擅长从行业变化中提炼判断。",
        color: "#16a34a",
        sort_order: 9,
    },
    TeamProfilePreset {
        name: "钟大嘴",
        member_code: "zdz",
        role_title: "稳定执行",
        persona_summary: "务实、低调、情绪稳定。",
        color: "#d6336c",
        sort_order: 10,
    },
];

pub fn team_profile_presets() -> &'static [TeamProfilePreset] {
    &TEAM_PROFILE_PRESETS
}

pub fn import_team_template_into_profiles(
    mut profiles: GatewayAccessProfiles,
) -> GatewayAccessProfiles {
    for preset in team_profile_presets() {
        if let Some(existing) = profiles.profiles.iter_mut().find(|profile| {
            profile.target == GatewayTarget::Codex
                && profile.member_code.as_deref() == Some(preset.member_code)
        }) {
            existing.name = preset.name.to_string();
            existing.member_code = Some(preset.member_code.to_string());
            existing.role_title = Some(preset.role_title.to_string());
            existing.persona_summary = Some(preset.persona_summary.to_string());
            existing.color = Some(preset.color.to_string());

            if existing.api_key.trim().is_empty() {
                existing.api_key = generate_team_gateway_api_key(preset.member_code);
            }

            continue;
        }

        profiles.upsert_profile(GatewayAccessProfile {
            id: format!("codex-{}", preset.member_code),
            name: preset.name.to_string(),
            target: GatewayTarget::Codex,
            api_key: generate_team_gateway_api_key(preset.member_code),
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
