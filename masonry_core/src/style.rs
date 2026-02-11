// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Helper types for style selection in higher layers (the embedder).
//!
//! Masonry Core does not implement a style system itself, but it does provide:
//! - Per-widget properties such as [`Classes`](crate::properties::Classes).
//! - Interaction state (hover/active/focus/disabled) via context methods.
//!
//! This module defines compact, allocation-free representations that an embedder can use
//! to build style selector inputs and caching keys.

use std::sync::Arc;

use crate::properties::{ClassId, Classes};

/// A stable identifier for an element "type" in style selectors.
///
/// This is application-defined (for example `Button`, `Label`, `SliderThumb`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeTag(pub u32);

/// A compact set of pseudoclasses for a single element.
///
/// This is intended for small, fixed vocabularies (for example `:hover`, `:active`, `:focus`,
/// `:disabled`) where allocating a collection would be wasteful.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StylePseudos {
    bits: u8,
}

impl StylePseudos {
    /// No pseudos are set.
    pub const EMPTY: Self = Self { bits: 0 };

    /// The element is hovered.
    pub const HOVER: Self = Self { bits: 1 << 0 };
    /// The element is active/pressed.
    pub const ACTIVE: Self = Self { bits: 1 << 1 };
    /// The element is the focus target.
    pub const FOCUS: Self = Self { bits: 1 << 2 };
    /// The element is disabled.
    ///
    /// Embedders should generally treat disabled as inherited, matching Masonry's behavior.
    pub const DISABLED: Self = Self { bits: 1 << 3 };

    /// Creates a pseudo set from common Masonry interaction flags.
    #[must_use]
    pub fn from_flags(hovered: bool, active: bool, focus: bool, disabled: bool) -> Self {
        let mut bits = 0;
        if hovered {
            bits |= Self::HOVER.bits;
        }
        if active {
            bits |= Self::ACTIVE.bits;
        }
        if focus {
            bits |= Self::FOCUS.bits;
        }
        if disabled {
            bits |= Self::DISABLED.bits;
        }
        Self { bits }
    }

    /// Returns `true` if this set contains `pseudo`.
    #[must_use]
    pub fn contains(self, pseudo: Self) -> bool {
        (self.bits & pseudo.bits) == pseudo.bits
    }
}

impl std::ops::BitOr for StylePseudos {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

impl std::ops::BitOrAssign for StylePseudos {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

/// A compact signature for style selection for a single element.
///
/// This is intended to be used as a cache key in embedders, and does not include theme/cascade
/// epochs; those are embedder-defined.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StyleSignature {
    /// The element type in selectors.
    pub type_tag: TypeTag,
    /// Pseudoclass set.
    pub pseudos: StylePseudos,
    /// Sorted, deduplicated classes.
    pub classes: Arc<[ClassId]>,
}

impl StyleSignature {
    /// Creates a signature from a type tag, pseudos, and a class set.
    #[must_use]
    pub fn new(type_tag: TypeTag, pseudos: StylePseudos, classes: &Classes) -> Self {
        Self {
            type_tag,
            pseudos,
            classes: classes.as_arc_slice(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudos_contains() {
        let pseudos = StylePseudos::from_flags(true, false, true, false);
        assert!(pseudos.contains(StylePseudos::HOVER));
        assert!(pseudos.contains(StylePseudos::FOCUS));
        assert!(!pseudos.contains(StylePseudos::ACTIVE));
        assert!(!pseudos.contains(StylePseudos::DISABLED));
    }
}
