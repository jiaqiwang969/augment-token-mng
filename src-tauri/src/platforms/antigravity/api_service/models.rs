use serde::{Deserialize, Serialize};

pub use crate::platforms::openai::codex::models::{
    DailyStats, DailyStatsResponse, GatewayDailyStatsResponse, GatewayDailyStatsSeries, LogPage,
    LogQuery, LogSummary, ModelTokenStats, PeriodTokenStats, RequestLog,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityGatewayProfileEntry {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub enabled: bool,
    pub is_primary: bool,
    pub member_code: Option<String>,
    pub role_title: Option<String>,
    pub persona_summary: Option<String>,
    pub color: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityGatewayProfileMutation {
    pub name: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
    pub member_code: Option<String>,
    pub role_title: Option<String>,
    pub persona_summary: Option<String>,
    pub color: Option<String>,
    pub notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_gateway_profile_entry_defaults_to_trimmed_api_key() {
        let entry = AntigravityGatewayProfileEntry {
            id: "antigravity-jdd".into(),
            name: "姜大大".into(),
            api_key: "  sk-ant-jdd-12345678  ".into(),
            enabled: true,
            is_primary: true,
            member_code: Some("jdd".into()),
            role_title: Some("产品与方法论".into()),
            persona_summary: Some("高频输出".into()),
            color: Some("#4c6ef5".into()),
            notes: Some("高频使用成员".into()),
        };

        assert_eq!(entry.api_key.trim(), "sk-ant-jdd-12345678");
        assert_eq!(entry.member_code.as_deref(), Some("jdd"));
    }
}
