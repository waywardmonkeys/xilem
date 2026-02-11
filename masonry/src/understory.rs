// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Adapters for using Understory-style selector rules with Masonry.
//!
//! Masonry Core can consult an embedder-provided [`StyleResolver`] during `pre_paint`.
//! This module provides a resolver implemented using [`understory_style`].

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::vec::Vec;

use masonry_core::properties::{Background, BorderColor, ClassId};
use masonry_core::style::{BoxPaintStyle, StylePseudos, StyleResolver, StyleValue};

use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    IdSet, PseudoClassId, Selector, SelectorInputs, StyleBuilder, StyleCascade,
    StyleCascadeBuilder, StyleOrigin, StyleSheetBuilder, TypeTag,
};

/// Understory-backed resolver for pseudo/class-driven box painting.
#[derive(Debug)]
pub struct UnderstoryBoxStyleResolver {
    background: understory_property::Property<Background>,
    border_color: understory_property::Property<BorderColor>,
    cascade: StyleCascade,
    type_tags: Mutex<TypeTagState>,
    class_cache: Mutex<HashMap<Arc<[ClassId]>, Arc<[understory_style::ClassId]>>>,
    // TODO: This cache is currently unbounded and has no invalidation story.
    // Before making styles/theme rules dynamic, add:
    // - a size cap + eviction strategy, and/or
    // - an explicit epoch in the cache key so callers can invalidate on theme changes.
    computed_box_paint_cache: Mutex<HashMap<BoxPaintCacheKey, CachedBoxPaintStyle>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BoxPaintCacheKey {
    widget_type: TypeId,
    pseudos: StylePseudos,
    classes: Arc<[ClassId]>,
}

#[derive(Clone, Debug)]
struct CachedBoxPaintStyle {
    background: Option<Arc<Background>>,
    border_color: Option<BorderColor>,
}

#[derive(Debug, Default)]
struct TypeTagState {
    next: u32,
    known: HashMap<TypeId, TypeTag>,
}

impl UnderstoryBoxStyleResolver {
    /// Creates an empty resolver with no rules.
    #[must_use]
    pub fn new_empty() -> Self {
        let mut registry = PropertyRegistry::new();
        let background = registry.register(
            "Background",
            PropertyMetadataBuilder::new(Background::default()).build(),
        );
        let border_color = registry.register(
            "BorderColor",
            PropertyMetadataBuilder::new(BorderColor::default()).build(),
        );

        let empty_sheet = StyleSheetBuilder::new().build();
        let cascade = StyleCascadeBuilder::new()
            .push_sheet(StyleOrigin::Sheet, empty_sheet)
            .build();

        Self {
            background,
            border_color,
            cascade,
            type_tags: Mutex::new(TypeTagState::default()),
            class_cache: Mutex::new(HashMap::new()),
            computed_box_paint_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a resolver with default Masonry control rules (currently `Button` only).
    #[must_use]
    pub fn new_default() -> Self {
        let mut resolver = Self::new_empty();

        // Reserve stable tags for known widget types.
        const BUTTON: TypeTag = TypeTag(1);
        const CHECKBOX: TypeTag = TypeTag(2);
        const SWITCH: TypeTag = TypeTag(3);
        const TEXT_INPUT: TypeTag = TypeTag(4);
        const BADGE: TypeTag = TypeTag(5);
        {
            let mut type_tags = resolver
                .type_tags
                .lock()
                .expect("poisoned TypeTagState lock");
            type_tags.known.extend([
                (TypeId::of::<crate::widgets::Button>(), BUTTON),
                (TypeId::of::<crate::widgets::Checkbox>(), CHECKBOX),
                (TypeId::of::<crate::widgets::Switch>(), SWITCH),
                (TypeId::of::<crate::widgets::TextInput>(), TEXT_INPUT),
                (TypeId::of::<crate::widgets::Badge>(), BADGE),
            ]);
            // Ensure dynamically allocated tags don't collide with reserved ones.
            type_tags.next = 6;
        }

        const HOVER: PseudoClassId = PseudoClassId(1);
        const ACTIVE: PseudoClassId = PseudoClassId(2);
        const FOCUS_WITHIN: PseudoClassId = PseudoClassId(4);
        const DISABLED: PseudoClassId = PseudoClassId(5);
        const TOGGLED: PseudoClassId = PseudoClassId(6);

        const CONTROL: understory_style::ClassId =
            understory_style::ClassId(crate::theme::CLASS_CONTROL.0);
        const PRESSABLE: understory_style::ClassId =
            understory_style::ClassId(crate::theme::CLASS_PRESSABLE.0);

        // Shared pseudo styles.
        //
        // These are expressed as selectors over default classes, so we don't need to duplicate the
        // same rules for each widget type.
        let active_bg = StyleBuilder::new()
            .set(
                resolver.background,
                Background::Color(crate::theme::ZYNC_700),
            )
            .build();
        let disabled_bg = StyleBuilder::new()
            .set(
                resolver.background,
                Background::Color(crate::peniko::Color::BLACK),
            )
            .build();
        let hover_border = StyleBuilder::new()
            .set(
                resolver.border_color,
                BorderColor {
                    color: crate::theme::ZYNC_500,
                },
            )
            .build();
        let focus_border = StyleBuilder::new()
            .set(
                resolver.border_color,
                BorderColor {
                    color: crate::theme::FOCUS_COLOR,
                },
            )
            .build();

        // Widget-specific overrides where the values differ from the shared pseudo styles.
        let switch_toggled_bg = StyleBuilder::new()
            .set(
                resolver.background,
                Background::Color(crate::theme::ACCENT_COLOR),
            )
            .build();
        let switch_active_bg = StyleBuilder::new()
            .set(
                resolver.background,
                Background::Color(crate::theme::ZYNC_600),
            )
            .build();
        let badge_disabled_bg = StyleBuilder::new()
            .set(
                resolver.background,
                Background::Color(crate::theme::ZYNC_800),
            )
            .build();

        let sheet = StyleSheetBuilder::new()
            // Type-specific rules.
            //
            // Order matters when multiple selectors apply; later rules win for equal specificity.
            // This is arranged so `:active` overrides `:toggled`, and `:disabled` overrides both.
            .rule(
                Selector {
                    type_tag: Some(SWITCH),
                    required_classes: IdSet::from_ids([PRESSABLE]),
                    required_pseudos: IdSet::from_ids([TOGGLED]),
                },
                switch_toggled_bg,
            )
            // Shared pseudo rules.
            .rule(
                Selector {
                    type_tag: None,
                    required_classes: IdSet::from_ids([PRESSABLE]),
                    required_pseudos: IdSet::from_ids([ACTIVE]),
                },
                active_bg.clone(),
            )
            // Type-specific overrides.
            .rule(
                Selector {
                    type_tag: Some(SWITCH),
                    required_classes: IdSet::from_ids([PRESSABLE]),
                    required_pseudos: IdSet::from_ids([ACTIVE]),
                },
                switch_active_bg,
            )
            .rule(
                Selector {
                    type_tag: None,
                    required_classes: IdSet::from_ids([PRESSABLE]),
                    required_pseudos: IdSet::from_ids([DISABLED]),
                },
                disabled_bg.clone(),
            )
            .rule(
                Selector {
                    type_tag: Some(BADGE),
                    required_classes: IdSet::default(),
                    required_pseudos: IdSet::from_ids([DISABLED]),
                },
                badge_disabled_bg,
            )
            // Shared border pseudo rules.
            .rule(
                Selector {
                    type_tag: None,
                    required_classes: IdSet::from_ids([CONTROL]),
                    required_pseudos: IdSet::from_ids([HOVER]),
                },
                hover_border.clone(),
            )
            .rule(
                Selector {
                    type_tag: None,
                    required_classes: IdSet::from_ids([CONTROL]),
                    required_pseudos: IdSet::from_ids([FOCUS_WITHIN]),
                },
                focus_border.clone(),
            )
            .build();

        resolver.cascade = StyleCascadeBuilder::new()
            .push_sheet(StyleOrigin::Sheet, sheet)
            .build();

        resolver
    }

    fn type_tag_for(&self, widget_type: TypeId) -> TypeTag {
        let mut state = self.type_tags.lock().expect("poisoned TypeTagState lock");
        if let Some(tag) = state.known.get(&widget_type) {
            return *tag;
        }
        let tag = TypeTag(state.next);
        state.next = state
            .next
            .checked_add(1)
            .expect("UnderstoryBoxStyleResolver exhausted available tags");
        state.known.insert(widget_type, tag);
        tag
    }

    fn classes_for(&self, classes: &Arc<[ClassId]>) -> Arc<[understory_style::ClassId]> {
        let mut cache = self.class_cache.lock().expect("poisoned class cache lock");
        if let Some(mapped) = cache.get(classes) {
            return Arc::clone(mapped);
        }

        let mapped: Arc<[understory_style::ClassId]> = classes
            .iter()
            .map(|ClassId(id)| understory_style::ClassId(*id))
            .collect::<Vec<_>>()
            .into();
        cache.insert(Arc::clone(classes), Arc::clone(&mapped));
        mapped
    }
}

impl StyleResolver for UnderstoryBoxStyleResolver {
    fn resolve_box_paint(
        &self,
        widget_type: TypeId,
        pseudos: StylePseudos,
        classes: &Arc<[ClassId]>,
    ) -> BoxPaintStyle<'_> {
        let cache_key = BoxPaintCacheKey {
            widget_type,
            pseudos,
            classes: Arc::clone(classes),
        };
        if let Some(cached) = self
            .computed_box_paint_cache
            .lock()
            .expect("poisoned computed style cache lock")
            .get(&cache_key)
            .cloned()
        {
            return BoxPaintStyle {
                background: cached.background.map(StyleValue::Shared),
                border_color: cached.border_color.map(StyleValue::Owned),
            };
        }

        let type_tag = self.type_tag_for(widget_type);
        let classes = self.classes_for(classes);

        // Map Masonry's fixed pseudos to Understory pseudo IDs.
        let mut pseudo_ids = [PseudoClassId(0); 6];
        let mut len = 0;
        if pseudos.contains(StylePseudos::HOVER) {
            pseudo_ids[len] = PseudoClassId(1);
            len += 1;
        }
        if pseudos.contains(StylePseudos::ACTIVE) {
            pseudo_ids[len] = PseudoClassId(2);
            len += 1;
        }
        if pseudos.contains(StylePseudos::FOCUS) {
            pseudo_ids[len] = PseudoClassId(3);
            len += 1;
        }
        if pseudos.contains(StylePseudos::FOCUS_WITHIN) {
            pseudo_ids[len] = PseudoClassId(4);
            len += 1;
        }
        if pseudos.contains(StylePseudos::DISABLED) {
            pseudo_ids[len] = PseudoClassId(5);
            len += 1;
        }
        if pseudos.contains(StylePseudos::TOGGLED) {
            pseudo_ids[len] = PseudoClassId(6);
            len += 1;
        }

        let inputs = SelectorInputs {
            type_tag: Some(type_tag),
            classes: &classes,
            pseudos: &pseudo_ids[..len],
        };

        let background_ref = self.cascade.get_value_ref(&inputs, self.background);
        let border_color_ref = self.cascade.get_value_ref(&inputs, self.border_color);

        let cached = CachedBoxPaintStyle {
            background: background_ref.map(|v| Arc::new(v.clone())),
            border_color: border_color_ref.copied(),
        };
        self.computed_box_paint_cache
            .lock()
            .expect("poisoned computed style cache lock")
            .insert(cache_key, cached.clone());

        BoxPaintStyle {
            background: cached.background.map(StyleValue::Shared),
            border_color: cached.border_color.map(StyleValue::Owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::sync::Arc;

    use masonry_core::properties::ClassId;
    use masonry_core::style::{StylePseudos, StyleResolver as _};

    use super::UnderstoryBoxStyleResolver;

    #[test]
    fn pressable_disabled_background_applies() {
        let resolver = UnderstoryBoxStyleResolver::new_default();
        let classes: Arc<[ClassId]> = Arc::from([crate::theme::CLASS_PRESSABLE]);
        let pseudos = StylePseudos::DISABLED;

        let style =
            resolver.resolve_box_paint(TypeId::of::<crate::widgets::Button>(), pseudos, &classes);
        assert_eq!(
            style.background.map(|v| v.as_ref().clone()),
            Some(masonry_core::properties::Background::Color(
                crate::peniko::Color::BLACK
            ))
        );
    }

    #[test]
    fn type_specific_override_wins_over_universal() {
        let resolver = UnderstoryBoxStyleResolver::new_default();
        let pressable: Arc<[ClassId]> = Arc::from([crate::theme::CLASS_PRESSABLE]);
        let none: Arc<[ClassId]> = Arc::from([]);

        // Switch :active uses a different background than the universal :active background.
        let style = resolver.resolve_box_paint(
            TypeId::of::<crate::widgets::Switch>(),
            StylePseudos::ACTIVE,
            &pressable,
        );
        assert_eq!(
            style.background.map(|v| v.as_ref().clone()),
            Some(masonry_core::properties::Background::Color(
                crate::theme::ZYNC_600
            ))
        );

        // Badge :disabled uses a different background than the universal :disabled background.
        let style = resolver.resolve_box_paint(
            TypeId::of::<crate::widgets::Badge>(),
            StylePseudos::DISABLED,
            &none,
        );
        assert_eq!(
            style.background.map(|v| v.as_ref().clone()),
            Some(masonry_core::properties::Background::Color(
                crate::theme::ZYNC_800
            ))
        );
    }

    #[test]
    fn switch_toggled_background_applies() {
        let resolver = UnderstoryBoxStyleResolver::new_default();
        let classes: Arc<[ClassId]> = Arc::from([crate::theme::CLASS_PRESSABLE]);
        let pseudos = StylePseudos::TOGGLED;

        let style =
            resolver.resolve_box_paint(TypeId::of::<crate::widgets::Switch>(), pseudos, &classes);
        assert_eq!(
            style.background.map(|v| v.as_ref().clone()),
            Some(masonry_core::properties::Background::Color(
                crate::theme::ACCENT_COLOR
            ))
        );
    }
}
