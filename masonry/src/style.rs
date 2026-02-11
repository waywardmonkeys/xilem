// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Helpers for building style selector inputs in embedders.
//!
//! Masonry Core provides the low-level ingredients for style selection:
//! - Widget interaction state (hover/active/focus/disabled) via context queries.
//! - A per-widget [`Classes`](masonry_core::properties::Classes) property.
//! - A compact [`StyleSignature`] value that can be used as a cache key.
//!
//! This module adds an embedder-friendly way to derive a stable [`TypeTag`] for a widget
//! (within a single process run), and convenience helpers for computing a widget's
//! [`StyleSignature`] without requiring the embedder to pass a tag manually.

use std::any::TypeId;
use std::collections::HashMap;

use masonry_core::core::{Widget, WidgetMut, WidgetRef};
use masonry_core::style::{StyleSignature, TypeTag};

/// A per-process mapping from Rust widget types to [`TypeTag`]s.
///
/// A `TypeTagMap` assigns a compact integer tag to each distinct widget Rust type,
/// using the widget's [`TypeId`] as the key.
///
/// Tags are stable for the lifetime of the map and are suitable for use in caching keys.
/// They are not stable across separate program runs.
#[derive(Debug, Default)]
pub struct TypeTagMap {
    next: u32,
    map: HashMap<TypeId, TypeTag>,
}

impl TypeTagMap {
    /// Creates an empty mapping.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the assigned [`TypeTag`] for `widget`, allocating a new tag if needed.
    ///
    /// This call does not allocate in the common case where `widget`'s type has been
    /// seen before.
    #[must_use]
    pub fn type_tag_for(&mut self, widget: &dyn Widget) -> TypeTag {
        let type_id = widget.type_id();
        if let Some(tag) = self.map.get(&type_id) {
            return *tag;
        }

        let tag = TypeTag(self.next);
        self.next = self
            .next
            .checked_add(1)
            .expect("TypeTagMap exhausted available tags");
        self.map.insert(type_id, tag);
        tag
    }
}

/// Extension methods for deriving [`StyleSignature`] values using a [`TypeTagMap`].
pub trait DeriveStyleSignature {
    /// Returns a compact signature for style selection for this widget.
    ///
    /// This combines:
    /// - A widget type tag derived from `type_tags`.
    /// - This widget's current pseudoclasses (hover/active/focus/disabled).
    /// - Its [`Classes`](masonry_core::properties::Classes) property.
    fn derived_style_signature(&self, type_tags: &mut TypeTagMap) -> StyleSignature;
}

impl<W: Widget + ?Sized> DeriveStyleSignature for WidgetRef<'_, W> {
    fn derived_style_signature(&self, type_tags: &mut TypeTagMap) -> StyleSignature {
        let type_tag = type_tags.type_tag_for((**self).as_dyn());
        self.style_signature(type_tag)
    }
}

impl<W: Widget + ?Sized> DeriveStyleSignature for WidgetMut<'_, W> {
    fn derived_style_signature(&self, type_tags: &mut TypeTagMap) -> StyleSignature {
        let type_tag = type_tags.type_tag_for(self.widget.as_dyn());
        self.style_signature(type_tag)
    }
}

#[cfg(test)]
mod tests {
    use masonry_core::core::AsDynWidget;
    use masonry_core::style::StylePseudos;
    use masonry_testing::TestHarness;

    use crate::core::{NewWidget, PointerButton, WidgetTag};
    use crate::theme::test_property_set;
    use crate::widgets::{Button, SizedBox};

    use super::*;

    #[test]
    fn type_tag_map_is_stable_for_same_widget_type() {
        let mut tags = TypeTagMap::new();
        let a = Button::with_text("a");
        let b = Button::with_text("b");
        let c = SizedBox::empty();

        let tag_a = tags.type_tag_for(a.as_dyn());
        let tag_b = tags.type_tag_for(b.as_dyn());
        let tag_c = tags.type_tag_for(c.as_dyn());

        assert_eq!(tag_a, tag_b);
        assert_ne!(tag_a, tag_c);
    }

    #[test]
    fn derived_style_signature_tracks_widget_state() {
        let button_tag = WidgetTag::named("button");
        let root_tag = WidgetTag::named("root");

        let button = NewWidget::new_with_tag(Button::with_text("ok"), button_tag);
        let root = NewWidget::new_with_tag(SizedBox::new(button), root_tag);

        let mut harness = TestHarness::create(test_property_set(), root);
        let button_id = harness.get_widget(button_tag).id();

        let mut tags = TypeTagMap::new();

        let sig0 = harness
            .get_widget(button_tag)
            .as_dyn()
            .derived_style_signature(&mut tags);
        assert!(!sig0.pseudos.contains(StylePseudos::HOVER));
        assert!(!sig0.pseudos.contains(StylePseudos::ACTIVE));
        assert!(!sig0.pseudos.contains(StylePseudos::FOCUS));
        assert!(!sig0.pseudos.contains(StylePseudos::DISABLED));

        harness.mouse_move_to(button_id);
        let sig_hover = harness
            .get_widget(button_tag)
            .as_dyn()
            .derived_style_signature(&mut tags);
        assert!(sig_hover.pseudos.contains(StylePseudos::HOVER));
        assert_ne!(sig0, sig_hover);

        harness.mouse_button_press(PointerButton::Primary);
        let sig_active = harness
            .get_widget(button_tag)
            .as_dyn()
            .derived_style_signature(&mut tags);
        assert!(sig_active.pseudos.contains(StylePseudos::ACTIVE));
        assert_ne!(sig_hover, sig_active);

        harness.focus_on(Some(button_id));
        let sig_focus = harness
            .get_widget(button_tag)
            .as_dyn()
            .derived_style_signature(&mut tags);
        assert!(sig_focus.pseudos.contains(StylePseudos::FOCUS));

        harness.set_disabled(root_tag, true);
        let sig_disabled = harness
            .get_widget(button_tag)
            .as_dyn()
            .derived_style_signature(&mut tags);
        assert!(sig_disabled.pseudos.contains(StylePseudos::DISABLED));
        assert_ne!(sig_focus, sig_disabled);
    }
}
