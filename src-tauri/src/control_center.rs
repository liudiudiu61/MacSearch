#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePolicy {
    pub physical_id: String,
    pub professional_description: String,
    pub low_battery_percent: u8,
    pub high_cpu_temperature_celsius: u16,
    pub battery_probe_failure_percent: u8,
    pub cpu_temperature_probe_failure_celsius: u16,
    pub full_disk_access_probe_paths: Vec<String>,
}

impl ResourcePolicy {
    pub fn from_json(payload: &str) -> Result<Self, PolicyParseError> {
        Ok(Self {
            physical_id: extract_json_string(payload, "physical_id")?,
            professional_description: extract_json_string(payload, "professional_description")?,
            low_battery_percent: extract_json_u8(payload, "low_battery_percent")?,
            high_cpu_temperature_celsius: extract_json_u16(
                payload,
                "high_cpu_temperature_celsius",
            )?,
            battery_probe_failure_percent: extract_json_u8(
                payload,
                "battery_probe_failure_percent",
            )?,
            cpu_temperature_probe_failure_celsius: extract_json_u16(
                payload,
                "cpu_temperature_probe_failure_celsius",
            )?,
            full_disk_access_probe_paths: extract_json_string_array(
                payload,
                "full_disk_access_probe_paths",
            )?,
        })
    }

    #[cfg(test)]
    fn default_for_tests() -> Self {
        Self {
            physical_id: "local.control.resource_policy".to_string(),
            professional_description: "Resource guard policy for tests".to_string(),
            low_battery_percent: 20,
            high_cpu_temperature_celsius: 75,
            battery_probe_failure_percent: 0,
            cpu_temperature_probe_failure_celsius: u16::MAX,
            full_disk_access_probe_paths: vec!["~/Library/Mail".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyParseError {
    pub field: &'static str,
}

fn extract_json_string(payload: &str, field: &'static str) -> Result<String, PolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload.find(&marker).ok_or(PolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let colon_index = after_field.find(':').ok_or(PolicyParseError { field })?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value_start = after_colon
        .strip_prefix('"')
        .ok_or(PolicyParseError { field })?;
    let value_end = value_start.find('"').ok_or(PolicyParseError { field })?;
    Ok(value_start[..value_end].to_string())
}

fn extract_json_u8(payload: &str, field: &'static str) -> Result<u8, PolicyParseError> {
    let value = extract_json_u16(payload, field)?;
    u8::try_from(value).map_err(|_| PolicyParseError { field })
}

fn extract_json_u16(payload: &str, field: &'static str) -> Result<u16, PolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload.find(&marker).ok_or(PolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let colon_index = after_field.find(':').ok_or(PolicyParseError { field })?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value_text: String = after_colon
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    value_text
        .parse::<u16>()
        .map_err(|_| PolicyParseError { field })
}

fn extract_json_string_array(
    payload: &str,
    field: &'static str,
) -> Result<Vec<String>, PolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload.find(&marker).ok_or(PolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let array_start = after_field.find('[').ok_or(PolicyParseError { field })?;
    let after_array_start = &after_field[array_start + 1..];
    let array_end = after_array_start
        .find(']')
        .ok_or(PolicyParseError { field })?;
    let array_body = &after_array_start[..array_end];
    let mut values = Vec::new();
    for raw_value in array_body.split(',') {
        let trimmed = raw_value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .ok_or(PolicyParseError { field })?;
        values.push(value.to_string());
    }
    Ok(values)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSnapshot {
    pub has_full_disk_access: bool,
    pub is_plugged_in: bool,
    pub battery_percent: u8,
    pub cpu_temperature_celsius: u16,
}

pub trait SystemProbe {
    fn snapshot(&self) -> SystemSnapshot;
}

pub struct MacOsSystemProbe {
    full_disk_access_probe_paths: Vec<String>,
    fallback_metrics: SystemMetrics,
}

impl MacOsSystemProbe {
    pub fn from_policy(policy: &ResourcePolicy) -> Self {
        Self {
            full_disk_access_probe_paths: policy.full_disk_access_probe_paths.clone(),
            fallback_metrics: fallback_system_metrics(policy),
        }
    }

    pub fn full_disk_access_probe_paths(&self) -> &[String] {
        &self.full_disk_access_probe_paths
    }
}

impl SystemProbe for MacOsSystemProbe {
    fn snapshot(&self) -> SystemSnapshot {
        platform_snapshot(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexingState {
    PermissionGuide,
    Building,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlDecision {
    pub indexing_state: IndexingState,
    pub allow_content_indexing: bool,
    pub allow_filename_updates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemMetrics {
    pub is_plugged_in: bool,
    pub battery_percent: u8,
    pub cpu_temperature_celsius: u16,
}

pub struct ResourcePoller<P: SystemProbe> {
    policy: ResourcePolicy,
    probe: P,
}

impl<P: SystemProbe> ResourcePoller<P> {
    pub fn new(policy: ResourcePolicy, probe: P) -> Self {
        Self { policy, probe }
    }

    pub fn poll_once(&self) -> ControlDecision {
        let snapshot = self.probe.snapshot();
        evaluate_control_state(&self.policy, &snapshot)
    }
}

pub fn evaluate_resource_guard(
    policy: ResourcePolicy,
    snapshot: SystemSnapshot,
) -> ControlDecision {
    evaluate_control_state(&policy, &snapshot)
}

pub fn evaluate_control_state(
    policy: &ResourcePolicy,
    snapshot: &SystemSnapshot,
) -> ControlDecision {
    if !snapshot.has_full_disk_access {
        return ControlDecision {
            indexing_state: IndexingState::PermissionGuide,
            allow_content_indexing: false,
            allow_filename_updates: false,
        };
    }

    let low_unplugged_battery =
        !snapshot.is_plugged_in && snapshot.battery_percent < policy.low_battery_percent;
    let high_cpu_temperature =
        snapshot.cpu_temperature_celsius > policy.high_cpu_temperature_celsius;

    if low_unplugged_battery || high_cpu_temperature {
        return ControlDecision {
            indexing_state: IndexingState::Suspended,
            allow_content_indexing: false,
            allow_filename_updates: true,
        };
    }

    ControlDecision {
        indexing_state: IndexingState::Building,
        allow_content_indexing: true,
        allow_filename_updates: true,
    }
}

#[cfg(target_os = "macos")]
fn platform_snapshot(probe: &MacOsSystemProbe) -> SystemSnapshot {
    let has_full_disk_access = has_full_disk_access(&probe.full_disk_access_probe_paths);
    let metrics = read_system_metrics().unwrap_or_else(|| probe.fallback_metrics.clone());
    SystemSnapshot {
        has_full_disk_access,
        is_plugged_in: metrics.is_plugged_in,
        battery_percent: metrics.battery_percent,
        cpu_temperature_celsius: metrics.cpu_temperature_celsius,
    }
}

#[cfg(not(target_os = "macos"))]
fn platform_snapshot(probe: &MacOsSystemProbe) -> SystemSnapshot {
    SystemSnapshot {
        has_full_disk_access: false,
        is_plugged_in: probe.fallback_metrics.is_plugged_in,
        battery_percent: probe.fallback_metrics.battery_percent,
        cpu_temperature_celsius: probe.fallback_metrics.cpu_temperature_celsius,
    }
}

pub fn fallback_system_metrics(policy: &ResourcePolicy) -> SystemMetrics {
    SystemMetrics {
        is_plugged_in: false,
        battery_percent: policy.battery_probe_failure_percent,
        cpu_temperature_celsius: policy.cpu_temperature_probe_failure_celsius,
    }
}

pub fn parse_pmset_battery_output(output: &str) -> Option<SystemMetrics> {
    let is_plugged_in = output.contains("AC Power");
    let percent_index = output.find('%')?;
    let before_percent = &output[..percent_index];
    let battery_percent_text: String = before_percent
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let battery_percent = battery_percent_text.parse::<u8>().ok()?;
    Some(SystemMetrics {
        is_plugged_in,
        battery_percent,
        cpu_temperature_celsius: u16::MAX,
    })
}

#[cfg(target_os = "macos")]
fn has_full_disk_access(paths: &[String]) -> bool {
    paths
        .iter()
        .all(|path| expand_home(path).read_dir().is_ok())
}

#[cfg(target_os = "macos")]
fn read_system_metrics() -> Option<SystemMetrics> {
    let output = std::process::Command::new("pmset")
        .arg("-g")
        .arg("batt")
        .output()
        .ok()?;
    let output_text = String::from_utf8(output.stdout).ok()?;
    parse_pmset_battery_output(&output_text)
}

#[cfg(target_os = "macos")]
fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_resource_policy_from_json_configuration() {
        let policy = ResourcePolicy::from_json(
            r#"{
              "physical_id": "local.control.resource_policy",
              "professional_description": "Resource guard policy for index building",
              "low_battery_percent": 20,
              "high_cpu_temperature_celsius": 75,
              "battery_probe_failure_percent": 0,
              "cpu_temperature_probe_failure_celsius": 65535,
              "full_disk_access_probe_paths": [
                "~/Library/Mail",
                "~/Library/Safari"
              ]
            }"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.low_battery_percent, 20);
        assert_eq!(policy.high_cpu_temperature_celsius, 75);
        assert_eq!(policy.full_disk_access_probe_paths.len(), 2);
        assert_eq!(policy.full_disk_access_probe_paths[0], "~/Library/Mail");
    }

    #[test]
    fn blocks_into_permission_guide_when_full_disk_access_is_missing() {
        let policy = ResourcePolicy::default_for_tests();
        let snapshot = SystemSnapshot {
            has_full_disk_access: false,
            is_plugged_in: true,
            battery_percent: 90,
            cpu_temperature_celsius: 45,
        };

        let decision = evaluate_control_state(&policy, &snapshot);

        assert_eq!(decision.indexing_state, IndexingState::PermissionGuide);
        assert!(!decision.allow_content_indexing);
        assert!(!decision.allow_filename_updates);
    }

    #[test]
    fn suspends_content_indexing_on_low_unplugged_battery() {
        let policy = ResourcePolicy::default_for_tests();
        let snapshot = SystemSnapshot {
            has_full_disk_access: true,
            is_plugged_in: false,
            battery_percent: 19,
            cpu_temperature_celsius: 45,
        };

        let decision = evaluate_control_state(&policy, &snapshot);

        assert_eq!(decision.indexing_state, IndexingState::Suspended);
        assert!(!decision.allow_content_indexing);
        assert!(decision.allow_filename_updates);
    }

    #[test]
    fn suspends_content_indexing_on_high_cpu_temperature() {
        let policy = ResourcePolicy::default_for_tests();
        let snapshot = SystemSnapshot {
            has_full_disk_access: true,
            is_plugged_in: true,
            battery_percent: 90,
            cpu_temperature_celsius: 76,
        };

        let decision = evaluate_control_state(&policy, &snapshot);

        assert_eq!(decision.indexing_state, IndexingState::Suspended);
        assert!(!decision.allow_content_indexing);
        assert!(decision.allow_filename_updates);
    }

    #[test]
    fn allows_full_indexing_when_authorized_and_resource_state_is_healthy() {
        let policy = ResourcePolicy::default_for_tests();
        let snapshot = SystemSnapshot {
            has_full_disk_access: true,
            is_plugged_in: false,
            battery_percent: 80,
            cpu_temperature_celsius: 45,
        };

        let decision = evaluate_control_state(&policy, &snapshot);

        assert_eq!(decision.indexing_state, IndexingState::Building);
        assert!(decision.allow_content_indexing);
        assert!(decision.allow_filename_updates);
    }

    #[test]
    fn poller_reads_provider_snapshot_and_returns_control_decision() {
        let policy = ResourcePolicy::default_for_tests();
        let provider = StaticSystemProbe {
            snapshot: SystemSnapshot {
                has_full_disk_access: true,
                is_plugged_in: false,
                battery_percent: 10,
                cpu_temperature_celsius: 45,
            },
        };
        let poller = ResourcePoller::new(policy, provider);

        let decision = poller.poll_once();

        assert_eq!(decision.indexing_state, IndexingState::Suspended);
        assert!(!decision.allow_content_indexing);
        assert!(decision.allow_filename_updates);
    }

    #[test]
    fn tauri_command_shape_returns_permission_guide_decision() {
        let policy = ResourcePolicy::default_for_tests();
        let snapshot = SystemSnapshot {
            has_full_disk_access: false,
            is_plugged_in: true,
            battery_percent: 90,
            cpu_temperature_celsius: 45,
        };

        let decision = evaluate_resource_guard(policy, snapshot);

        assert_eq!(decision.indexing_state, IndexingState::PermissionGuide);
    }

    #[test]
    fn macos_probe_inherits_configured_full_disk_access_paths() {
        let policy = ResourcePolicy::default_for_tests();

        let probe = MacOsSystemProbe::from_policy(&policy);

        assert_eq!(probe.full_disk_access_probe_paths(), &["~/Library/Mail"]);
    }

    #[test]
    fn parses_pmset_battery_output_without_hardcoded_policy_thresholds() {
        let reading = parse_pmset_battery_output(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1234567)    19%; discharging; 4:20 remaining present: true",
        )
        .expect("battery reading should parse");

        assert!(!reading.is_plugged_in);
        assert_eq!(reading.battery_percent, 19);
    }

    #[test]
    fn failed_system_metric_reads_fall_back_to_conservative_policy_values() {
        let policy = ResourcePolicy::default_for_tests();

        let reading = fallback_system_metrics(&policy);

        assert!(!reading.is_plugged_in);
        assert_eq!(
            reading.battery_percent,
            policy.battery_probe_failure_percent
        );
        assert_eq!(
            reading.cpu_temperature_celsius,
            policy.cpu_temperature_probe_failure_celsius
        );
    }

    struct StaticSystemProbe {
        snapshot: SystemSnapshot,
    }

    impl SystemProbe for StaticSystemProbe {
        fn snapshot(&self) -> SystemSnapshot {
            self.snapshot.clone()
        }
    }
}
