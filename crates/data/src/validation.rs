//! Bundle invariants checker.
//!
//! Run after schema version checks pass. Verifies internal consistency:
//! - All `BaseType.item_class` references exist in `item_classes`
//! - All `ModDefinition.allowed_item_classes` references exist
//! - All `BaseType.implicits` reference mods in `mods`
//! - All `mods_by_base` entries reference real bases and mods
//! - Patch ranges are coherent (`min <= max`)
//! - No duplicate ids in any top-level section
//!
//! M2.3 expands this to check weight scope coverage, synergy graph
//! connectivity, etc.

use ahash::{AHashMap, AHashSet};

use crate::bundle::Bundle;
use crate::error::{DataError, DataResult};

pub fn validate(bundle: &Bundle) -> DataResult<()> {
    let ids = IdSets::from(bundle);
    validate_bases(bundle, &ids)?;
    validate_classes(bundle, &ids)?;
    validate_mods(bundle, &ids)?;
    validate_mods_by_base(bundle, &ids)?;
    validate_concept_map(bundle, &ids)?;
    validate_no_duplicates(bundle)?;
    Ok(())
}

struct IdSets<'a> {
    classes: AHashSet<&'a str>,
    bases: AHashSet<&'a str>,
    mods: AHashSet<&'a str>,
    tags: AHashSet<&'a str>,
    concepts: AHashSet<&'a str>,
}

impl<'a> From<&'a Bundle> for IdSets<'a> {
    fn from(b: &'a Bundle) -> Self {
        Self {
            classes: b.item_classes.iter().map(|c| c.id.as_str()).collect(),
            bases: b.base_items.iter().map(|x| x.id.as_str()).collect(),
            mods: b.mods.iter().map(|x| x.id.as_str()).collect(),
            tags: b.tags.iter().map(|x| x.id.as_str()).collect(),
            concepts: b.concepts.iter().map(|x| x.id.as_str()).collect(),
        }
    }
}

fn dangling(
    entity_kind: &'static str,
    id: impl Into<String>,
    ref_kind: &'static str,
    ref_id: impl Into<String>,
) -> DataError {
    DataError::DanglingReference {
        entity_kind,
        id: id.into(),
        ref_kind,
        ref_id: ref_id.into(),
    }
}

fn validate_bases(bundle: &Bundle, ids: &IdSets<'_>) -> DataResult<()> {
    for base in &bundle.base_items {
        if !ids.classes.contains(base.item_class.as_str()) {
            return Err(dangling(
                "BaseType",
                base.id.to_string(),
                "ItemClass",
                base.item_class.to_string(),
            ));
        }
        for implicit in &base.implicits {
            if !ids.mods.contains(implicit.as_str()) {
                return Err(dangling(
                    "BaseType.implicit",
                    base.id.to_string(),
                    "Mod",
                    implicit.to_string(),
                ));
            }
        }
        for tag in &base.tags {
            if !ids.tags.contains(tag.as_str()) {
                return Err(dangling(
                    "BaseType.tag",
                    base.id.to_string(),
                    "Tag",
                    tag.to_string(),
                ));
            }
        }
        validate_patch_range(base.patch_range)?;
    }
    Ok(())
}

fn validate_classes(bundle: &Bundle, ids: &IdSets<'_>) -> DataResult<()> {
    for class in &bundle.item_classes {
        for tag in &class.class_tags {
            if !ids.tags.contains(tag.as_str()) {
                return Err(dangling(
                    "ItemClass.class_tag",
                    class.id.to_string(),
                    "Tag",
                    tag.to_string(),
                ));
            }
        }
        validate_patch_range(class.patch_range)?;
    }
    Ok(())
}

fn validate_mods(bundle: &Bundle, ids: &IdSets<'_>) -> DataResult<()> {
    for m in &bundle.mods {
        for tag in &m.tags {
            if !ids.tags.contains(tag.as_str()) {
                return Err(dangling(
                    "ModDefinition.tag",
                    m.id.to_string(),
                    "Tag",
                    tag.to_string(),
                ));
            }
        }
        for class in &m.allowed_item_classes {
            if !ids.classes.contains(class.as_str()) {
                return Err(dangling(
                    "ModDefinition.allowed_item_class",
                    m.id.to_string(),
                    "ItemClass",
                    class.to_string(),
                ));
            }
        }
        for sw in &m.spawn_weights {
            if !ids.tags.contains(sw.tag.as_str()) {
                return Err(dangling(
                    "ModDefinition.spawn_weight",
                    m.id.to_string(),
                    "Tag",
                    sw.tag.to_string(),
                ));
            }
        }
        validate_patch_range(m.patch_range)?;
    }
    Ok(())
}

fn validate_mods_by_base(bundle: &Bundle, ids: &IdSets<'_>) -> DataResult<()> {
    for (base_id, mods) in &bundle.mods_by_base {
        if !ids.bases.contains(base_id.as_str()) {
            return Err(dangling(
                "mods_by_base.key",
                base_id.clone(),
                "BaseType",
                base_id.clone(),
            ));
        }
        for m in mods {
            if !ids.mods.contains(m.as_str()) {
                return Err(dangling(
                    "mods_by_base.value",
                    base_id.clone(),
                    "Mod",
                    m.clone(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_concept_map(bundle: &Bundle, ids: &IdSets<'_>) -> DataResult<()> {
    for entry in &bundle.concept_map.0 {
        if !ids.concepts.contains(entry.concept_id.as_str()) {
            return Err(dangling(
                "ConceptMap.entry",
                entry.stat_id.to_string(),
                "Concept",
                entry.concept_id.to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_no_duplicates(bundle: &Bundle) -> DataResult<()> {
    detect_duplicates(&bundle.mods.iter().map(|x| x.id.as_str()), "ModDefinition")?;
    detect_duplicates(&bundle.base_items.iter().map(|x| x.id.as_str()), "BaseType")?;
    detect_duplicates(
        &bundle.item_classes.iter().map(|x| x.id.as_str()),
        "ItemClass",
    )?;
    detect_duplicates(&bundle.tags.iter().map(|x| x.id.as_str()), "Tag")?;
    detect_duplicates(&bundle.concepts.iter().map(|x| x.id.as_str()), "Concept")?;
    Ok(())
}

fn validate_patch_range(r: poc2_engine::PatchRange) -> DataResult<()> {
    if let (Some(min), Some(max)) = (r.min, r.max) {
        if min > max {
            return Err(DataError::Validation(format!(
                "patch range invalid: min {min} > max {max}"
            )));
        }
    }
    Ok(())
}

fn detect_duplicates<'a, I>(ids: &I, kind: &'static str) -> DataResult<()>
where
    I: Iterator<Item = &'a str> + Clone,
{
    let mut seen: AHashMap<&str, ()> = AHashMap::new();
    for id in ids.clone() {
        if seen.insert(id, ()).is_some() {
            return Err(DataError::Validation(format!("duplicate {kind} id: {id}")));
        }
    }
    Ok(())
}
