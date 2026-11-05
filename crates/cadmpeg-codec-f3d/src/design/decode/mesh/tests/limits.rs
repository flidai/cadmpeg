// SPDX-License-Identifier: Apache-2.0
//! Resource-limit refusals of the mesh body projection and its diagnostics.

use super::*;

#[test]
fn mesh_body_projection_refuses_identifier_and_collection_limits() {
    let transform = crate::records::mesh::MeshAffineTransform::new([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
    .unwrap();
    let container = || MeshContainer {
        fusion_uuid: "AAAAAAAA-BBBB-4CCC-8DDD-EEEEEEEEEEEE".into(),
        mesh_uuid: crate::records::mesh::DesignMeshUuid::try_from(
            "11111111-2222-4333-8444-555555555555".to_owned(),
        )
        .unwrap(),
        vertices: vec![FinitePoint3::ZERO],
        triangles: Vec::new(),
        feature_edges: Vec::new(),
        corner_normals: Some(vec![UnitVector3::Z_AXIS]),
        triangle_groups: Vec::new(),
        texture_ids: None,
        attributes: Vec::new(),
    };
    let native_scope_bytes = u64_from_index(crate::ids::native_scope("mesh.paramesh").len());
    for (collection_limit, retained_limit, dimension, operation) in [
        (
            0,
            u64::MAX,
            cadmpeg_core::decode::ResourceDimension::CollectionItems,
            "f3d placed mesh vertices",
        ),
        (
            1,
            u64::MAX,
            cadmpeg_core::decode::ResourceDimension::CollectionItems,
            "f3d placed mesh normals",
        ),
        (
            2,
            0,
            cadmpeg_core::decode::ResourceDimension::RetainedBytes,
            "f3d native stream key",
        ),
        (
            2,
            native_scope_bytes,
            cadmpeg_core::decode::ResourceDimension::RetainedBytes,
            "f3d mesh body identifier",
        ),
    ] {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let mut policy = cadmpeg_core::decode::DecodePolicy::default();
        policy.limits.max_collection_items = collection_limit;
        policy.limits.max_retained_bytes = retained_limit;

        let (ctx, _) =
            cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
        assert!(matches!(
            MeshBody::from_container(&ctx, "mesh.paramesh", 100, transform, container()),
            Err(CodecError::ResourceLimit(failure))
                if failure.dimension == dimension && failure.operation == operation
        ));
    }
    crate::design::test_support::with_test_decode_context(|ctx| {
        let body =
            MeshBody::from_container(ctx, "mesh.paramesh", 100, transform, container()).unwrap();
        assert_eq!(
            body.id,
            crate::ids::native_mesh_body_id("mesh.paramesh", 100)
        );
        assert_eq!(body.vertices.len(), 1);
        assert_eq!(body.corner_normals.unwrap().len(), 1);
    });
}

#[test]
fn mesh_design_stream_and_feature_vectors_refuse_collection_limit() {
    let graph = synthetic_mesh_graph(false);
    let mut no_asset = no_texture_asset;
    let design = parse_mesh_design_records(
        &graph.bytes,
        &graph.meta,
        "Synthetic/BulkStream.dat",
        &mut no_asset,
    )
    .unwrap();
    assert_eq!(design.len(), 1);
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut policy = cadmpeg_core::decode::DecodePolicy::default();
    policy.limits.max_collection_items = 0;

    let (ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(matches!(
        ctx.push_vec(&mut Vec::new(), design, "f3d mesh design streams"),
        Err(CodecError::ResourceLimit(limit))
            if limit.dimension == cadmpeg_core::decode::ResourceDimension::CollectionItems
                && limit.operation == "f3d mesh design streams"
    ));
    let mut no_asset = no_texture_asset;
    let design = parse_mesh_design_records(
        &graph.bytes,
        &graph.meta,
        "Synthetic/BulkStream.dat",
        &mut no_asset,
    )
    .unwrap();
    let (flatten_ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(matches!(
        super::super::flatten_mesh_features(&flatten_ctx, vec![design]),
        Err(CodecError::ResourceLimit(limit))
            if limit.dimension == cadmpeg_core::decode::ResourceDimension::CollectionItems
                && limit.operation == "f3d mesh decoded features"
    ));
}

#[test]
fn mesh_outcome_and_text_copy_refuse_caller_limits() {
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut collection_policy = cadmpeg_core::decode::DecodePolicy::default();
    collection_policy.limits.max_collection_items = 0;
    let (collection_ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &collection_policy)
            .unwrap();
    assert!(matches!(
        collection_ctx.push_vec(&mut Vec::new(), super::super::MeshContainerOutcome::Missing { entry_name: String::new() }, "f3d mesh container outcomes"),
        Err(CodecError::ResourceLimit(limit))
            if limit.dimension == cadmpeg_core::decode::ResourceDimension::CollectionItems
                && limit.operation == "f3d mesh container outcomes"
    ));
    let mut retained_policy = cadmpeg_core::decode::DecodePolicy::default();
    retained_policy.limits.max_retained_bytes = 0;
    let (retained_ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &retained_policy)
            .unwrap();
    assert!(matches!(
        retained_ctx.copy_retained_text("mesh.paramesh", "f3d test mesh text"),
        Err(CodecError::ResourceLimit(limit))
            if limit.dimension == cadmpeg_core::decode::ResourceDimension::RetainedBytes
                && limit.operation == "f3d test mesh text"
    ));
}

#[test]
fn mesh_frame_diagnostic_refuses_retained_limit() {
    let expected = "F3D Design mesh-wrapper entity 108 has an invalid primary frame";
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut policy = cadmpeg_core::decode::DecodePolicy::default();
    policy.limits.max_retained_bytes = u64::try_from(expected.len() - 1).unwrap();
    let (ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(
        matches!(super::super::malformed_frame(&ctx, "mesh-wrapper", 108),
        CodecError::ResourceLimit(failure) if failure.operation == "f3d Design diagnostic")
    );
}

#[test]
fn mesh_graph_invariant_diagnostic_refuses_retained_limit() {
    let expected = "F3D Design mesh feature graph violates `member` in Synthetic/BulkStream.dat";
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut policy = cadmpeg_core::decode::DecodePolicy::default();
    policy.limits.max_retained_bytes = u64::try_from(expected.len() - 1).unwrap();
    let (ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(
        matches!(super::super::malformed_mesh_graph(&ctx, "Synthetic/BulkStream.dat", "member"),
        CodecError::ResourceLimit(failure) if failure.operation == "f3d Design diagnostic")
    );
}

#[test]
fn mesh_invalid_record_index_diagnostic_refuses_retained_limit() {
    let graph = synthetic_mesh_graph(false);
    let frame = sole_typed_frame(&graph, MESH_WRAPPER_TYPE_GUID);
    let expected = format!(
        "F3D Design mesh-wrapper entity {} has an invalid record index",
        frame.entity_id
    );
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut policy = cadmpeg_core::decode::DecodePolicy::default();
    policy.limits.max_retained_bytes = u64::try_from(expected.len() - 1).unwrap();

    let (ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(
        matches!(super::super::exact_record_index(&ctx, &[], frame, "mesh-wrapper"),
        Err(CodecError::ResourceLimit(failure)) if failure.operation == "f3d Design diagnostic")
    );
}

#[test]
fn mesh_unsupported_version_diagnostic_refuses_retained_limit() {
    let graph = synthetic_mesh_graph(false);
    let frame = sole_typed_frame(&graph, MESH_WRAPPER_TYPE_GUID);
    let expected = format!(
        "F3D Design mesh-wrapper record version {} is unsupported",
        frame.design_type.version
    );
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let mut policy = cadmpeg_core::decode::DecodePolicy::default();
    policy.limits.max_retained_bytes = u64::try_from(expected.len() - 1).unwrap();

    let (ctx, _) =
        cadmpeg_core::decode::DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
    assert!(
        matches!(super::super::validate_mesh_registration(&ctx, frame, frame.design_type.version + 1,
        MESH_WRAPPER_BASE_TYPE_GUID, PARAMESH_MODULE, "mesh-wrapper"),
        Err(CodecError::ResourceLimit(failure)) if failure.operation == "f3d Design unsupported diagnostic")
    );
}
