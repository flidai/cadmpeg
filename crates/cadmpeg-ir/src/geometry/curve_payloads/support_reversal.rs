// SPDX-License-Identifier: Apache-2.0
//! Support contexts of payload constructions on the reversed parameter `-t`.

use super::super::support_reversal::{reverse_cache_first_form, reversed_pcurve, reversed_side};
use super::{
    SilhouetteCurveConstruction, SpringCurvePayload, SurfaceOffsetCurveConstruction,
    ThreeSurfaceIntersectionCurvePayload, TwoSidedOffsetCurveConstruction,
};
use crate::geometry::{CacheContract, SpringLayout, SpringPcurve};
use crate::scalar::FiniteReal;
use crate::topology::ParameterInterval;

impl ThreeSurfaceIntersectionCurvePayload {
    pub(crate) fn reverse_about_zero(&mut self) -> Option<()> {
        let context = self.context.reversed_about_zero()?;
        let third = reversed_side(&self.third)?;
        self.context = context;
        self.third = third;
        Some(())
    }
}

impl TwoSidedOffsetCurveConstruction {
    pub(crate) fn reverse_about_zero(&mut self) -> Option<()> {
        self.context = self.context.reversed_about_zero()?;
        Some(())
    }
}

impl SilhouetteCurveConstruction {
    pub(crate) fn reverse_about_zero(&mut self) -> Option<()> {
        self.context = self.context.reversed_about_zero()?;
        Some(())
    }
}

impl SurfaceOffsetCurveConstruction {
    /// The base endpoints pair with the context endpoints by position, so
    /// they swap with them. They are base-curve parameters and keep their
    /// values.
    pub(crate) fn reverse_about_zero(&mut self) -> Option<()> {
        self.context = self.context.reversed_about_zero()?;
        self.base_endpoints.reverse();
        if let CacheContract::Revision { form } = &mut self.cache {
            reverse_cache_first_form(form);
        }
        Some(())
    }
}

impl SpringCurvePayload {
    /// Restate the support context of either layout on `-t`. The context-first
    /// layout also restates its interval, discontinuity arrays, and pcurves.
    /// Null-support ranges are support-surface parameters and keep their
    /// values. The direction code keeps its stored value.
    ///
    /// `None`, leaving the payload unchanged, when the first pcurve slot holds
    /// a replacement interval or a pcurve has no reversed form.
    pub(crate) fn reverse_about_zero(&mut self) -> Option<()> {
        let context = self.context.reversed_about_zero()?;
        let layout = match &self.layout {
            SpringLayout::CacheFirst { context, form } => {
                let mut form = form.clone();
                reverse_cache_first_form(&mut form);
                SpringLayout::CacheFirst {
                    context: context.reversed_about_zero()?,
                    form,
                }
            }
            SpringLayout::ContextFirst {
                supports,
                first_pcurve,
                second_pcurve,
                parameter_range,
                discontinuities,
                discontinuity_flag,
                cache,
            } => {
                let SpringPcurve::Pcurve(first) = first_pcurve.as_ref() else {
                    return None;
                };
                let [start, end] = parameter_range.endpoints();
                SpringLayout::ContextFirst {
                    supports: supports.clone(),
                    first_pcurve: Box::new(SpringPcurve::Pcurve(reversed_pcurve(first)?)),
                    second_pcurve: match second_pcurve {
                        Some(pcurve) => Some(reversed_pcurve(pcurve)?),
                        None => None,
                    },
                    parameter_range: ParameterInterval::new([-end, -start]).ok()?,
                    discontinuities: discontinuities
                        .clone()
                        .map(|values| values.into_iter().rev().map(FiniteReal::negated).collect()),
                    discontinuity_flag: *discontinuity_flag,
                    cache: *cache,
                }
            }
        };
        self.layout = layout;
        self.context = context;
        Some(())
    }
}

#[cfg(test)]
mod tests;
