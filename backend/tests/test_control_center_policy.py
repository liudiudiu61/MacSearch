from pathlib import Path

from app.config.control_rules import load_control_rules
from app.control_center.policy import (
    DiskAccessStatus,
    PowerSource,
    ResourceSnapshot,
    decide_indexer_mode,
    decide_permission_mode,
)


PROJECT_ROOT = Path(__file__).resolve().parents[2]


def test_permission_mode_blocks_full_scan_without_full_disk_access():
    mode = decide_permission_mode(DiskAccessStatus.DENIED)

    assert mode.state == "init"
    assert mode.scan_scope == "sandbox_only"
    assert mode.requires_user_guidance is True


def test_permission_mode_allows_local_indexing_when_authorized():
    mode = decide_permission_mode(DiskAccessStatus.AUTHORIZED)

    assert mode.state == "watching"
    assert mode.scan_scope == "configured_paths"
    assert mode.requires_user_guidance is False


def test_unplugged_low_battery_suspends_content_indexing():
    rules = load_control_rules(PROJECT_ROOT / "config" / "control_rules.json")
    snapshot = ResourceSnapshot(
        power_source=PowerSource.BATTERY,
        battery_percent=19,
        cpu_load_percent=20,
        cpu_temperature_celsius=45,
    )

    decision = decide_indexer_mode(snapshot, rules.indexer_suspension)

    assert decision.state == "suspended"
    assert decision.allow_filename_indexing is True
    assert decision.allow_content_indexing is False
    assert decision.reason_code == "battery_below_threshold"


def test_high_cpu_load_suspends_content_indexing():
    rules = load_control_rules(PROJECT_ROOT / "config" / "control_rules.json")
    snapshot = ResourceSnapshot(
        power_source=PowerSource.AC,
        battery_percent=90,
        cpu_load_percent=81,
        cpu_temperature_celsius=55,
    )

    decision = decide_indexer_mode(snapshot, rules.indexer_suspension)

    assert decision.state == "suspended"
    assert decision.reason_code == "cpu_load_above_threshold"


def test_unknown_battery_defaults_to_conservative_suspension():
    rules = load_control_rules(PROJECT_ROOT / "config" / "control_rules.json")
    snapshot = ResourceSnapshot(
        power_source=PowerSource.UNKNOWN,
        battery_percent=None,
        cpu_load_percent=20,
        cpu_temperature_celsius=None,
    )

    decision = decide_indexer_mode(snapshot, rules.indexer_suspension)

    assert decision.state == "suspended"
    assert decision.reason_code == "resource_state_unknown"
