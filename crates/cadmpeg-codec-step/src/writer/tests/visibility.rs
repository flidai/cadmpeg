// SPDX-License-Identifier: Apache-2.0
//! STEP writer visibility tests.

use std::io::Cursor;

use cadmpeg_ir::codec::{Codec, DecodeOptions};
use cadmpeg_ir::examples::unit_cube;

use crate::export::write_step;
use crate::{StepCodec, StepSchema, StepWriteOptions};

/// ISO 10303-46 `invisible_item` selects a styled item, a presentation layer
/// assignment or a presentation representation, never a solid. A hidden body is
/// hidden through a null-style `STYLED_ITEM` that `INVISIBILITY` names.
#[test]
fn writer_round_trips_body_visibility_through_a_null_styled_item() {
    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    ir.model.bodies[0].visible = Some(false);

    let mut output = Vec::new();
    let report = write_step(
        &ir,
        &mut output,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .expect("write hidden body");
    assert!(report.losses.is_empty(), "{:#?}", report.losses);
    let text = String::from_utf8(output).expect("STEP output is UTF-8");
    let invisibility = text
        .lines()
        .find(|line| line.contains("= INVISIBILITY("))
        .expect("an INVISIBILITY record");
    let targets = invisibility
        .split_once("((")
        .and_then(|(_, rest)| rest.split_once("))"))
        .map(|(targets, _)| targets)
        .expect("one set of targets");
    for target in targets.split(',') {
        let record = text
            .lines()
            .find(|line| line.starts_with(&format!("{target} = ")))
            .expect("the target record");
        assert!(record.contains("= STYLED_ITEM('',(#"), "{record}");
    }
    assert!(text.contains("PRESENTATION_STYLE_ASSIGNMENT((NULL_STYLE(.NULL.)))"));

    let decoded = StepCodec::default()
        .decode(&mut Cursor::new(text), &DecodeOptions::default())
        .expect("decode hidden body");
    assert_eq!(decoded.ir().model.bodies[0].visible, Some(false));
    assert!(
        !decoded
            .report()
            .losses
            .iter()
            .any(|loss| loss.message.contains("INVISIBILITY")),
        "{:#?}",
        decoded.report().losses
    );
}
