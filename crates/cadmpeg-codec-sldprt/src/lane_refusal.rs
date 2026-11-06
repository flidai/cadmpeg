// SPDX-License-Identifier: Apache-2.0
//! Sink for carrier records whose lanes the IR carrier refuses.

use cadmpeg_core::decode::DecodeContext;
use cadmpeg_core::CodecError;

/// Solver calls that may run out of work in one decode before the rest are
/// withheld without running. Each such call spends its whole slice, so the
/// limit bounds the wall time that failing solvers can take.
const SOLVER_EXHAUSTION_LIMIT: u32 = 4;

/// Local work units that all solver calls of one decode may spend together.
///
/// A solver call that finds no answer spends its whole slice, and a decode can
/// ask the same question of many edges. The session budget bounds that work
/// only in the end; this total bounds the wall time that the optional solvers
/// take and leaves the rest of the decode its time.
const SOLVER_TOTAL_WORK: u64 = 24_000_000;

/// The part of the session work allowance that the optional solvers leave
/// unspent, as a divisor of the allowance. The rest of the decode needs it for
/// the records that it must transfer. The reserve is a share of what the limit
/// allows, so a body that needs little work before the solvers leaves them
/// more.
const SOLVER_SESSION_RESERVE: u64 = 3;

/// Counts the solver calls of one decode that ran out of work and the work
/// that they spent.
#[derive(Debug, Default)]
pub(crate) struct SolverGate {
    exhausted: std::cell::Cell<u32>,
    spent: std::cell::Cell<u64>,
}

impl SolverGate {
    /// Whether another solver call may start.
    pub(crate) fn is_open(&self) -> bool {
        self.exhausted.get() < SOLVER_EXHAUSTION_LIMIT && self.spent.get() < SOLVER_TOTAL_WORK
    }

    /// The local work that the next solver call may spend.
    pub(crate) fn allowance(&self) -> u64 {
        SOLVER_TOTAL_WORK - self.spent.get().min(SOLVER_TOTAL_WORK)
    }

    /// The session work that the solvers may still spend: what the session has
    /// left above the reserve.
    pub(crate) fn session_allowance(ctx: &DecodeContext<'_>) -> u64 {
        let reserve = ctx.policy().limits.max_work_units / SOLVER_SESSION_RESERVE;
        let Some(left) = ctx.work_remaining().checked_sub(reserve) else {
            return 0;
        };
        left
    }

    /// Record the local work that a finished call spent.
    pub(crate) fn note_spent(&self, units: u64) {
        let total = match self.spent.get().checked_add(units) {
            Some(total) if total < SOLVER_TOTAL_WORK => total,
            _ => SOLVER_TOTAL_WORK,
        };
        self.spent.set(total);
    }

    /// Record a call that ran out of its work slice.
    pub(crate) fn note_exhausted(&self) {
        let count = self.exhausted.get();
        if count < SOLVER_EXHAUSTION_LIMIT {
            self.exhausted.set(count + 1);
        }
    }
}

/// Every carrier record a reader refused, each named by the record that stated
/// it.
///
/// The IR carrier states which lanes disagree; the codec states which source
/// record stated them. The sink is owned by the reader that reports, so no
/// `return None`, `?` or `.ok()?` between a refusal and the report can drop it,
/// and N refusals in one document stay N records.
#[derive(Debug, Default)]
pub(crate) struct LaneRefusals {
    records: Vec<String>,
    withheld: Vec<String>,
    gate: SolverGate,
}

impl LaneRefusals {
    /// An empty sink.
    pub(crate) fn new() -> Self {
        Self {
            records: Vec::new(),
            withheld: Vec::new(),
            gate: SolverGate::default(),
        }
    }

    /// Record one refusal against the record that stated it.
    pub(crate) fn note(
        &mut self,
        ctx: &DecodeContext<'_>,
        record: impl std::fmt::Display,
        error: &cadmpeg_ir::geometry::nurbs::NurbsError,
    ) -> Result<(), CodecError> {
        if let cadmpeg_ir::geometry::nurbs::NurbsError::ResourceLimit(limit) = error {
            return Err((*limit).into());
        }
        let message = ctx.format_retained(
            format_args!("{record}: {error}"),
            "record SLDPRT lane refusal",
        )?;
        ctx.reserve_vec(&mut self.records, 1, "collect SLDPRT lane refusals")?;
        self.records.push(message);
        Ok(())
    }

    /// The solver gate of the decode that owns this sink.
    pub(crate) fn gate(&self) -> &SolverGate {
        &self.gate
    }

    /// Record that optional solver work for `record` was withheld because the
    /// session could not pay for it.
    pub(crate) fn withhold(
        &mut self,
        ctx: &DecodeContext<'_>,
        record: impl std::fmt::Display,
    ) -> Result<(), CodecError> {
        let message = ctx.format_retained(
            format_args!("{record}"),
            "record SLDPRT withheld solver work",
        )?;
        ctx.reserve_vec(&mut self.withheld, 1, "collect SLDPRT withheld solver work")?;
        self.withheld.push(message);
        Ok(())
    }

    /// Take every withheld-work record, in reader order, and leave the list
    /// empty.
    pub(crate) fn take_withheld(&mut self) -> Vec<String> {
        std::mem::take(&mut self.withheld)
    }

    /// Take every refusal recorded so far, in reader order, and leave the sink
    /// empty.
    pub(crate) fn take_records(&mut self) -> Vec<String> {
        std::mem::take(&mut self.records)
    }
}

#[cfg(test)]
mod tests {
    use super::LaneRefusals;
    use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy, ResourceDimension};
    use cadmpeg_core::CodecError;
    use cadmpeg_ir::geometry::nurbs::NurbsError;

    #[test]
    fn lane_refusal_message_refuses_retained_limit() {
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_retained_bytes = 5;
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &policy).expect("empty root fits policy");
        let mut refusals = LaneRefusals::new();
        let Err(CodecError::ResourceLimit(limit)) =
            refusals.note(&ctx, "record", &NurbsError::Structure("invalid".into()))
        else {
            panic!("refusal message must use retained budget")
        };
        assert_eq!(limit.dimension, ResourceDimension::RetainedBytes);
        assert!(refusals.take_records().is_empty());
    }

    #[test]
    fn lane_refusal_record_refuses_collection_limit() {
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_collection_items = 0;
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &policy).expect("empty root fits policy");
        let mut refusals = LaneRefusals::new();
        let Err(CodecError::ResourceLimit(limit)) =
            refusals.note(&ctx, "record", &NurbsError::Structure("invalid".into()))
        else {
            panic!("refusal record must use collection budget")
        };
        assert_eq!(limit.dimension, ResourceDimension::CollectionItems);
        assert!(refusals.take_records().is_empty());
        let arena = DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service())
            .expect("empty root fits service policy");
        refusals
            .note(&ctx, "record", &NurbsError::Structure("invalid".into()))
            .expect("service budget");
        assert_eq!(refusals.take_records(), ["record: invalid"]);
    }
    #[test]
    fn lane_refusal_preserves_constructor_resource_limit() {
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_collection_items = 0;
        let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
        let CodecError::ResourceLimit(limit) = ctx
            .charge_collection_items(1, "test constructor admission")
            .unwrap_err()
        else {
            panic!("constructor admission must refuse the collection limit");
        };
        let error = NurbsError::ResourceLimit(limit);
        let mut refusals = LaneRefusals::new();
        assert!(
            matches!(refusals.note(&ctx, "record", &error), Err(CodecError::ResourceLimit(actual)) if actual == limit)
        );
        assert!(refusals.take_records().is_empty());
        assert!(
            matches!(CodecError::from(error), CodecError::ResourceLimit(actual) if actual == limit)
        );
    }
}
