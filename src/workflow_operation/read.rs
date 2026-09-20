use super::CatalogSnapshot;
use crate::XcStringsError;
use crate::model::translation::{
    PluralUnit, TranslationDestination, TranslationLeaf, TranslationUnit,
};
use crate::service::workflow::{self, WorkflowView};

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct ValidationOutput {
    pub reports: Vec<crate::model::translation::ValidationReport>,
    pub terminology: crate::guidance_operation::GuidanceReport,
    pub tracking: crate::model::workflow::TrackingStatus,
    pub source_changed_keys: Vec<String>,
    pub untracked_keys: Vec<String>,
    pub input_revisions: super::InputRevisions,
}

pub fn validate_catalog(
    snapshot: &CatalogSnapshot,
    guidance: &crate::guidance_operation::GuidanceSnapshot,
    locale: Option<&str>,
) -> Result<ValidationOutput, XcStringsError> {
    use crate::model::workflow::SourceFreshness;
    let view = snapshot.view()?;
    Ok(ValidationOutput {
        reports: crate::service::file_validator::validate_file(&view.effective_catalog, locale),
        terminology: guidance.check_catalog(&snapshot.catalog, &snapshot.workflow.contexts, locale),
        tracking: view.tracking,
        source_changed_keys: view
            .keys
            .iter()
            .filter(|(_, status)| status.freshness == SourceFreshness::SourceChanged)
            .map(|(key, _)| key.clone())
            .collect(),
        untracked_keys: view
            .keys
            .iter()
            .filter(|(_, status)| status.freshness == SourceFreshness::Untracked)
            .map(|(key, _)| key.clone())
            .collect(),
        input_revisions: snapshot.revisions(),
    })
}

pub fn annotate_leaves(
    snapshot: &CatalogSnapshot,
    view: &WorkflowView<'_>,
    key: &str,
    locale: &str,
    leaves: &mut [TranslationLeaf],
) -> Result<(), XcStringsError> {
    for leaf in leaves {
        let destination = TranslationDestination {
            key: key.into(),
            locale: locale.into(),
            path: leaf.path.clone(),
        };
        leaf.workflow = Some(workflow::leaf_status(
            snapshot.identity_text()?,
            &snapshot.catalog,
            view,
            &destination,
        )?);
    }
    Ok(())
}

pub fn annotate_unit(
    snapshot: &CatalogSnapshot,
    view: &WorkflowView<'_>,
    unit: &mut TranslationUnit,
) -> Result<(), XcStringsError> {
    let status = view
        .keys
        .get(&unit.key)
        .ok_or_else(|| XcStringsError::KeyNotFound(unit.key.clone()))?;
    unit.source_version = Some(status.source_version.clone());
    unit.source_freshness = Some(status.freshness);
    annotate_leaves(
        snapshot,
        view,
        &unit.key,
        &unit.target_locale,
        &mut unit.leaves,
    )
}

pub fn annotate_plural(
    snapshot: &CatalogSnapshot,
    view: &WorkflowView<'_>,
    unit: &mut PluralUnit,
) -> Result<(), XcStringsError> {
    let status = view
        .keys
        .get(&unit.key)
        .ok_or_else(|| XcStringsError::KeyNotFound(unit.key.clone()))?;
    unit.source_version = Some(status.source_version.clone());
    unit.source_freshness = Some(status.freshness);
    annotate_leaves(
        snapshot,
        view,
        &unit.key,
        &unit.target_locale,
        &mut unit.leaves,
    )
}
