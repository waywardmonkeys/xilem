// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::core::{HasProperty, Property, Widget};

/// A stable identifier for a user-defined class (e.g. `.primary`).
///
/// This is intentionally application-defined.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub u32);

/// A widget's class list.
///
/// This is a set (unordered) semantically: values are always stored sorted and deduplicated.
///
/// Intended use is for style-rule selection in higher layers (the embedder).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classes {
    classes: Arc<[ClassId]>,
}

// We expect any widget to potentially participate in style selection.
impl<W: Widget> HasProperty<Classes> for W {}

impl Classes {
    /// An empty class list.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            classes: Arc::from([]),
        }
    }

    /// Constructs a class set from an iterator, sorting and deduplicating.
    #[must_use]
    pub fn from_ids(ids: impl IntoIterator<Item = ClassId>) -> Self {
        let mut classes: Vec<ClassId> = ids.into_iter().collect();
        classes.sort();
        classes.dedup();
        Self {
            classes: Arc::from(classes.into_boxed_slice()),
        }
    }

    /// Returns the underlying class IDs as a sorted, deduplicated slice.
    #[must_use]
    pub fn as_slice(&self) -> &[ClassId] {
        &self.classes
    }

    /// Returns the underlying class IDs as a shared slice.
    #[must_use]
    pub fn as_arc_slice(&self) -> Arc<[ClassId]> {
        self.classes.clone()
    }
}

impl Default for Classes {
    fn default() -> Self {
        Self::empty()
    }
}

impl Property for Classes {
    fn static_default() -> &'static Self {
        static DEFAULT: std::sync::LazyLock<Classes> = std::sync::LazyLock::new(Classes::empty);
        &DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_is_set_semantics() {
        let a = ClassId(1);
        let b = ClassId(2);

        let classes = Classes::from_ids([b, a, b, a]);
        assert_eq!(classes.as_slice(), &[a, b]);
    }
}
