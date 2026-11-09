// SPDX-License-Identifier: Apache-2.0
//! The regions of a decoded body.

use std::collections::HashSet;

use cadmpeg_core::decode::DecodeContext;
use cadmpeg_core::CodecError;
use cadmpeg_ir::document::CadIr;

/// Remove the void region that a body states without a shell.
///
/// A Parasolid solid body states its infinite outside as a region. That region
/// bounds no shell of its own, so it has nothing to write. A body that has a
/// region with a shell and a region without one is a body with such a void.
/// A body whose regions are all empty keeps them: it has no geometry to show.
///
/// Returns the identities of the regions removed.
pub(crate) fn drop_void_regions(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
) -> Result<Vec<String>, CodecError> {
    let mut with_shell = HashSet::new();
    for region in &ir.model.regions {
        ctx.charge_work(1, "index Solid Edge regions")?;
        if !region.shells.is_empty() {
            ctx.insert_hash_set(
                &mut with_shell,
                region.body.clone(),
                "index Solid Edge bodies with a bounded region",
            )?;
        }
    }
    let mut dropped = HashSet::new();
    for region in &ir.model.regions {
        if region.shells.is_empty() && with_shell.contains(&region.body) {
            ctx.insert_hash_set(
                &mut dropped,
                region.id.clone(),
                "track Solid Edge void regions",
            )?;
        }
    }
    if dropped.is_empty() {
        return Ok(Vec::new());
    }
    ir.model
        .regions
        .retain(|region| !dropped.contains(&region.id));
    for body in &mut ir.model.bodies {
        body.regions.retain(|region| !dropped.contains(region));
    }
    let mut ids: Vec<String> = dropped.iter().map(|id| id.as_str().to_owned()).collect();
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_decode_context;

    #[test]
    fn a_void_region_without_a_shell_is_dropped_beside_a_bounded_region() {
        let mut ir = cadmpeg_ir::examples::unit_cube().expect("the example cube builds");
        let bounded = ir.model.regions[0].clone();
        let mut void = bounded.clone();
        void.id = cadmpeg_ir::ids::RegionId::compose(
            &cadmpeg_ir::identity_namespace!("solidedge", "test", "region"),
            cadmpeg_ir::identity_key!("void"),
        );
        void.shells.clear();
        let void_id = void.id.as_str().to_owned();
        ir.model.bodies[0].regions.push(void.id.clone());
        ir.model.regions.push(void);
        with_decode_context(&[], |ctx| {
            let dropped = drop_void_regions(ctx, &mut ir).expect("admitted");
            assert_eq!(dropped, vec![void_id]);
            assert!(drop_void_regions(ctx, &mut ir)
                .expect("admitted")
                .is_empty());
        });
        assert_eq!(ir.model.regions, vec![bounded.clone()]);
        assert_eq!(ir.model.bodies[0].regions, vec![bounded.id]);
    }

    #[test]
    fn a_body_whose_regions_are_all_empty_keeps_them() {
        let mut ir = cadmpeg_ir::examples::unit_cube().expect("the example cube builds");
        ir.model.regions[0].shells.clear();
        with_decode_context(&[], |ctx| {
            assert!(drop_void_regions(ctx, &mut ir)
                .expect("admitted")
                .is_empty());
        });
        assert_eq!(ir.model.regions.len(), 1);
    }
}
