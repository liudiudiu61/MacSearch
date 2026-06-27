from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum

from app.config.control_rules import IndexerSuspensionRules


class DiskAccessStatus(StrEnum):
    AUTHORIZED = "authorized"
    DENIED = "denied"
    UNKNOWN = "unknown"


class PowerSource(StrEnum):
    AC = "ac"
    BATTERY = "battery"
    UNKNOWN = "unknown"


@dataclass(frozen=True)
class ResourceSnapshot:
    power_source: PowerSource
    battery_percent: int | None
    cpu_load_percent: int | None
    cpu_temperature_celsius: int | None


@dataclass(frozen=True)
class PermissionMode:
    state: str
    scan_scope: str
    requires_user_guidance: bool


@dataclass(frozen=True)
class IndexerMode:
    state: str
    allow_filename_indexing: bool
    allow_content_indexing: bool
    reason_code: str


def decide_permission_mode(status: DiskAccessStatus) -> PermissionMode:
    if status == DiskAccessStatus.AUTHORIZED:
        return PermissionMode(
            state="watching",
            scan_scope="configured_paths",
            requires_user_guidance=False,
        )
    return PermissionMode(
        state="init",
        scan_scope="sandbox_only",
        requires_user_guidance=True,
    )


def decide_indexer_mode(
    snapshot: ResourceSnapshot, rules: IndexerSuspensionRules
) -> IndexerMode:
    if _resource_state_unknown(snapshot):
        return _suspended("resource_state_unknown")

    if (
        snapshot.power_source == PowerSource.BATTERY
        and snapshot.battery_percent is not None
        and snapshot.battery_percent < rules.battery_percent_below
    ):
        return _suspended("battery_below_threshold")

    if (
        snapshot.cpu_load_percent is not None
        and snapshot.cpu_load_percent > rules.cpu_load_percent_above
    ):
        return _suspended("cpu_load_above_threshold")

    if (
        snapshot.cpu_temperature_celsius is not None
        and snapshot.cpu_temperature_celsius > rules.cpu_temperature_celsius_above
    ):
        return _suspended("cpu_temperature_above_threshold")

    return IndexerMode(
        state="watching",
        allow_filename_indexing=True,
        allow_content_indexing=True,
        reason_code="resource_state_healthy",
    )


def _resource_state_unknown(snapshot: ResourceSnapshot) -> bool:
    return snapshot.power_source == PowerSource.UNKNOWN or (
        snapshot.battery_percent is None and snapshot.cpu_load_percent is None
    )


def _suspended(reason_code: str) -> IndexerMode:
    return IndexerMode(
        state="suspended",
        allow_filename_indexing=True,
        allow_content_indexing=False,
        reason_code=reason_code,
    )
