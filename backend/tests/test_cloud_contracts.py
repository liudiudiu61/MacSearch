from app.models.cloud_contracts import (
    CloudConfigSync,
    SubscriptionStatus,
    SynonymDictionaryEntry,
    UserSubscription,
)


def test_cloud_contracts_require_physical_identity_and_professional_description():
    subscription = UserSubscription(
        user_id="user_001",
        physical_id="cloud.subscription.user_001",
        professional_description="Developer trial subscription",
        status=SubscriptionStatus.ACTIVE,
        plan_code="trial",
    )

    synonym = SynonymDictionaryEntry(
        synonym_id="syn_architecture",
        physical_id="cloud.synonym.architecture",
        professional_description="Architecture search expansion",
        source_term="架构",
        equivalent_terms=["architecture", "design"],
        locale="zh-CN",
    )

    sync = CloudConfigSync(
        config_id="sync_default",
        physical_id="cloud.config.default",
        professional_description="Default local search configuration",
        config_payload={"max_size_mb": 50},
        version=1,
    )

    assert subscription.status is SubscriptionStatus.ACTIVE
    assert synonym.equivalent_terms == ["architecture", "design"]
    assert sync.config_payload["max_size_mb"] == 50
