// SPDX-License-Identifier: Apache-2.0

use super::super::retain_scope_listed_parameter_owners;
use crate::records::feature::scope::{DesignFeatureKind, DesignParameterScope};
use crate::records::identity::{Located, ReferenceRun};
use crate::records::parameters::DesignParameterOwner;

fn owner(record_index: u32, scope_record_index: u32, local_ordinal: u32) -> DesignParameterOwner {
    let mut frame = crate::design::test_support::parameter_owner_frame();
    frame[7..11].copy_from_slice(&record_index.to_le_bytes());
    frame[25..29].copy_from_slice(&scope_record_index.to_le_bytes());
    frame[35..39].copy_from_slice(&local_ordinal.to_le_bytes());
    frame[49..53].copy_from_slice(&(record_index + 1).to_le_bytes());
    frame[68..72].copy_from_slice(&scope_record_index.to_le_bytes());
    frame[82..86].copy_from_slice(&(record_index + 2).to_le_bytes());
    frame[94..98].copy_from_slice(&scope_record_index.to_le_bytes());
    crate::design::decode::parameters::parse_parameter_owner(&frame)
        .expect("owner frame")
        .into_record("Design1/BulkStream.dat", u64::from(record_index) * 200)
        .expect("owner record")
}

fn scope(owner: &DesignParameterOwner, members: &[u32]) -> DesignParameterScope {
    let mut scope = DesignParameterScope::empty(
        &owner
            .id()
            .replace("design-parameter-owner", "design-parameter-scope"),
        DesignFeatureKind::Fillet,
        owner.scope_record_index(),
    );
    scope
        .try_edit(|draft| {
            draft.reference_members = ReferenceRun::located(
                members
                    .iter()
                    .map(|value| Located {
                        value: *value,
                        offset: 0,
                    })
                    .collect(),
            );
            draft.reference_count_offset = draft.byte_offset + 9;
            draft.layout_fixture_references();
        })
        .expect("scope draft");
    scope
}

#[test]
fn unlisted_owner_repeating_a_listed_ordinal_is_dropped() {
    let listed = owner(50015, 22346, 0);
    let stale = owner(42864, 22346, 0);
    let distinct = owner(42870, 22346, 1);
    let other_scope = owner(7, 9, 0);
    let scopes = [scope(&listed, &[50015])];
    let mut owners = vec![listed, stale, distinct, other_scope];
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let policy = cadmpeg_core::decode::DecodePolicy::default();
    let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(&[0], &arena, &policy)
        .expect("test decode context");
    retain_scope_listed_parameter_owners(&ctx, &mut owners, &scopes).expect("retain");
    let kept = owners
        .iter()
        .map(DesignParameterOwner::record_index)
        .collect::<Vec<_>>();
    assert_eq!(kept, [50015, 42870, 7]);
}
