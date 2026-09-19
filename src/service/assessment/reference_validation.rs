use std::collections::HashSet;

use crate::model::xcstrings::{
    Localization,
    paths::{LeafStep, collect_leaves},
    references::substitution_references,
};

/// Named substitutions belong to the localization root, including references in
/// device leaves. Preserve all metadata and reject edits that disconnect it.
pub(crate) fn validate_substitution_references(root: &Localization) -> Result<(), String> {
    let mut referenced = HashSet::new();
    for leaf in collect_leaves(root).leaves {
        if leaf
            .path
            .iter()
            .any(|step| matches!(step, LeafStep::Substitution(_)))
        {
            continue;
        }
        for reference in substitution_references(&leaf.unit.value)? {
            let sub = root
                .substitutions
                .as_ref()
                .and_then(|subs| subs.get(&reference.name))
                .ok_or_else(|| format!("undefined root substitution '{}'", reference.name))?;
            let position = sub
                .arg_num
                .filter(|position| *position > 0)
                .ok_or_else(|| {
                    format!("substitution '{}' requires a valid argNum", reference.name)
                })?;
            if reference.position.is_some_and(|actual| actual != position) {
                return Err(format!(
                    "substitution '{}' reference position disagrees with argNum",
                    reference.name
                ));
            }
            referenced.insert(reference.name);
        }
    }
    if let Some(substitutions) = &root.substitutions {
        for name in substitutions.keys() {
            if !referenced.contains(name) {
                return Err(format!(
                    "substitution '{name}' has no reference in a parent stringUnit"
                ));
            }
        }
    }
    Ok(())
}
