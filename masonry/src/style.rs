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
use std::sync::Arc;

use masonry_core::core::{Widget, WidgetMut, WidgetRef};
use masonry_core::style::{StyleSignature, TypeTag};

use crate::core::Property;
use crate::properties::{Background, BorderColor, BorderWidth, BoxShadow, CornerRadius, Padding};

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

/// Bitflags indicating which rendering channels are affected by a style change.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleChannels {
    bits: u8,
}

impl StyleChannels {
    /// No channels are affected.
    pub const EMPTY: Self = Self { bits: 0 };
    /// Layout (measure/layout/compose) must run.
    pub const LAYOUT: Self = Self { bits: 1 << 0 };
    /// Paint must run.
    pub const PAINT: Self = Self { bits: 1 << 1 };
    /// Text shaping/line breaking must run.
    pub const TEXT: Self = Self { bits: 1 << 2 };

    /// Returns `true` if this set contains `channel`.
    #[must_use]
    pub fn contains(self, channel: Self) -> bool {
        (self.bits & channel.bits) == channel.bits
    }
}

impl std::ops::BitOr for StyleChannels {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

impl std::ops::BitOrAssign for StyleChannels {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

/// Optional overrides for "box-like" style properties.
///
/// This represents the contribution of a style cascade for a given element signature.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxStyle {
    /// Overrides [`Padding`].
    pub padding: Option<Padding>,
    /// Overrides [`BorderWidth`].
    pub border_width: Option<BorderWidth>,
    /// Overrides [`Background`].
    pub background: Option<Background>,
    /// Overrides [`BorderColor`].
    pub border_color: Option<BorderColor>,
    /// Overrides [`CornerRadius`].
    pub corner_radius: Option<CornerRadius>,
    /// Overrides [`BoxShadow`].
    pub box_shadow: Option<BoxShadow>,
}

/// A resolved, shared bundle of box-like properties.
///
/// This value is intended to be cached by `StyleSignature` and embedder epochs.
#[derive(Clone, Debug, PartialEq)]
pub struct ComputedBox {
    /// Resolved [`Padding`].
    pub padding: Padding,
    /// Resolved [`BorderWidth`].
    pub border_width: BorderWidth,
    /// Resolved [`Background`].
    pub background: Background,
    /// Resolved [`BorderColor`].
    pub border_color: BorderColor,
    /// Resolved [`CornerRadius`].
    pub corner_radius: CornerRadius,
    /// Resolved [`BoxShadow`].
    pub box_shadow: BoxShadow,
}

impl ComputedBox {
    /// Resolves a computed box value with precedence:
    /// `local widget properties` > `style` > `defaults/static`.
    #[must_use]
    pub fn resolve(props: &masonry_core::core::PropertiesRef<'_>, style: &BoxStyle) -> Self {
        resolve_box_with_source(props, style)
    }

    /// Computes which channels need to be invalidated when moving from `old` to `new`.
    #[must_use]
    pub fn changed_channels(old: &Self, new: &Self) -> StyleChannels {
        let mut channels = StyleChannels::EMPTY;

        if old.padding != new.padding || old.border_width != new.border_width {
            channels |= StyleChannels::LAYOUT;
            channels |= StyleChannels::PAINT;
        }

        if old.background != new.background
            || old.border_color != new.border_color
            || old.corner_radius != new.corner_radius
            || old.box_shadow != new.box_shadow
        {
            channels |= StyleChannels::PAINT;
        }

        channels
    }
}

fn resolve_box_with_source(props: &impl BoxPropertySource, style: &BoxStyle) -> ComputedBox {
    let padding = if props.contains::<Padding>() {
        *props.get::<Padding>()
    } else if let Some(padding) = style.padding {
        padding
    } else {
        *props.get::<Padding>()
    };

    let border_width = if props.contains::<BorderWidth>() {
        *props.get::<BorderWidth>()
    } else if let Some(border_width) = style.border_width {
        border_width
    } else {
        *props.get::<BorderWidth>()
    };

    let background = if props.contains::<Background>() {
        props.get::<Background>().clone()
    } else if let Some(background) = style.background.clone() {
        background
    } else {
        props.get::<Background>().clone()
    };

    let border_color = if props.contains::<BorderColor>() {
        *props.get::<BorderColor>()
    } else if let Some(border_color) = style.border_color {
        border_color
    } else {
        *props.get::<BorderColor>()
    };

    let corner_radius = if props.contains::<CornerRadius>() {
        *props.get::<CornerRadius>()
    } else if let Some(corner_radius) = style.corner_radius {
        corner_radius
    } else {
        *props.get::<CornerRadius>()
    };

    let box_shadow = if props.contains::<BoxShadow>() {
        *props.get::<BoxShadow>()
    } else if let Some(box_shadow) = style.box_shadow {
        box_shadow
    } else {
        *props.get::<BoxShadow>()
    };

    ComputedBox {
        padding,
        border_width,
        background,
        border_color,
        corner_radius,
        box_shadow,
    }
}

/// A minimal abstraction over a source of widget properties.
///
/// This allows style resolution code to be tested without constructing a `PropertiesRef`.
trait BoxPropertySource {
    fn contains<P: Property>(&self) -> bool;
    fn get<P: Property>(&self) -> &P;
}

impl BoxPropertySource for masonry_core::core::PropertiesRef<'_> {
    fn contains<P: Property>(&self) -> bool {
        masonry_core::core::PropertiesRef::contains::<P>(self)
    }

    fn get<P: Property>(&self) -> &P {
        masonry_core::core::PropertiesRef::get::<P>(self)
    }
}

/// Cache key for computed box bundles.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComputedBoxKey {
    /// The selector signature for an element.
    pub signature: StyleSignature,
    /// An embedder-defined epoch representing theme/default changes.
    pub theme_epoch: u64,
    /// An embedder-defined epoch representing cascade/rule changes.
    pub cascade_epoch: u64,
}

/// A cache for [`ComputedBox`] bundles keyed by signature and embedder epochs.
#[derive(Debug, Default)]
pub struct ComputedBoxCache {
    map: HashMap<ComputedBoxKey, Arc<ComputedBox>>,
}

impl ComputedBoxCache {
    /// Creates an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves a computed box bundle for the given key and inputs.
    ///
    /// If any relevant box properties are set locally on this widget, this will skip the
    /// cache and return a freshly resolved bundle, to avoid "poisoning" a shared cache
    /// entry with per-widget values.
    #[must_use]
    pub fn resolve(
        &mut self,
        key: ComputedBoxKey,
        props: &masonry_core::core::PropertiesRef<'_>,
        style: &BoxStyle,
    ) -> Arc<ComputedBox> {
        if has_any_local_box_overrides(props) {
            return Arc::new(resolve_box_with_source(props, style));
        }

        if let Some(value) = self.map.get(&key) {
            return Arc::clone(value);
        }

        let value = Arc::new(resolve_box_with_source(props, style));
        self.map.insert(key, Arc::clone(&value));
        value
    }

    #[cfg(test)]
    fn resolve_with_source(
        &mut self,
        key: ComputedBoxKey,
        props: &impl BoxPropertySource,
        style: &BoxStyle,
    ) -> Arc<ComputedBox> {
        if has_any_local_box_overrides(props) {
            return Arc::new(resolve_box_with_source(props, style));
        }

        if let Some(value) = self.map.get(&key) {
            return Arc::clone(value);
        }

        let value = Arc::new(resolve_box_with_source(props, style));
        self.map.insert(key, Arc::clone(&value));
        value
    }
}

fn has_any_local_box_overrides(props: &impl BoxPropertySource) -> bool {
    props.contains::<Padding>()
        || props.contains::<BorderWidth>()
        || props.contains::<Background>()
        || props.contains::<BorderColor>()
        || props.contains::<CornerRadius>()
        || props.contains::<BoxShadow>()
}

#[cfg(test)]
mod tests {
    use masonry_core::core::AsDynWidget;
    use masonry_core::style::StylePseudos;

    use std::any::Any;
    use std::collections::HashMap;

    use crate::properties::{ClassId, Classes};
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
    fn computed_box_precedence_and_channels() {
        struct FakeProps {
            local: HashMap<TypeId, Box<dyn Any>>,
            defaults: HashMap<TypeId, Box<dyn Any>>,
        }

        impl BoxPropertySource for FakeProps {
            fn contains<P: Property>(&self) -> bool {
                self.local.contains_key(&TypeId::of::<P>())
            }

            fn get<P: Property>(&self) -> &P {
                if let Some(value) = self.local.get(&TypeId::of::<P>()) {
                    return value
                        .downcast_ref::<P>()
                        .expect("stored local value has wrong type");
                }
                if let Some(value) = self.defaults.get(&TypeId::of::<P>()) {
                    return value
                        .downcast_ref::<P>()
                        .expect("stored default value has wrong type");
                }
                P::static_default()
            }
        }

        let mut defaults = HashMap::<TypeId, Box<dyn Any>>::new();
        defaults.insert(TypeId::of::<Padding>(), Box::new(Padding::all(1.0)));
        defaults.insert(TypeId::of::<BorderWidth>(), Box::new(BorderWidth::all(0.0)));
        defaults.insert(TypeId::of::<Background>(), Box::new(Background::default()));
        defaults.insert(
            TypeId::of::<BorderColor>(),
            Box::new(BorderColor::default()),
        );
        defaults.insert(
            TypeId::of::<CornerRadius>(),
            Box::new(CornerRadius::default()),
        );
        defaults.insert(TypeId::of::<BoxShadow>(), Box::new(BoxShadow::default()));

        let props = FakeProps {
            local: HashMap::new(),
            defaults,
        };

        let style = BoxStyle {
            padding: Some(Padding::all(2.0)),
            ..BoxStyle::default()
        };

        let computed = resolve_box_with_source(&props, &style);
        assert_eq!(computed.padding, Padding::all(2.0));

        let mut props_local = FakeProps {
            local: HashMap::new(),
            defaults: props.defaults,
        };
        props_local
            .local
            .insert(TypeId::of::<Padding>(), Box::new(Padding::all(3.0)));
        let computed_local = resolve_box_with_source(&props_local, &style);
        assert_eq!(computed_local.padding, Padding::all(3.0));

        let channels = ComputedBox::changed_channels(&computed, &computed_local);
        assert!(channels.contains(StyleChannels::LAYOUT));
        assert!(channels.contains(StyleChannels::PAINT));

        let computed_bg = ComputedBox {
            background: Background::default(),
            ..computed.clone()
        };
        let non_default_bg =
            Background::Color(crate::peniko::color::AlphaColor::from_rgb8(1, 2, 3));
        let computed_bg2 = ComputedBox {
            background: non_default_bg,
            ..computed.clone()
        };
        let channels = ComputedBox::changed_channels(&computed_bg, &computed_bg2);
        assert!(!channels.contains(StyleChannels::LAYOUT));
        assert!(channels.contains(StyleChannels::PAINT));
    }

    #[test]
    fn computed_box_cache_shares_when_no_locals() {
        struct NoLocals;
        impl BoxPropertySource for NoLocals {
            fn contains<P: Property>(&self) -> bool {
                false
            }
            fn get<P: Property>(&self) -> &P {
                P::static_default()
            }
        }

        let signature = StyleSignature::new(
            TypeTag(1),
            StylePseudos::EMPTY,
            &Classes::from_ids([ClassId(1)]),
        );
        let key = ComputedBoxKey {
            signature,
            theme_epoch: 0,
            cascade_epoch: 0,
        };

        let props = NoLocals;
        let style = BoxStyle::default();

        let mut cache = ComputedBoxCache::new();
        let a = cache.resolve_with_source(key.clone(), &props, &style);
        let b = cache.resolve_with_source(key, &props, &style);
        assert!(Arc::ptr_eq(&a, &b));
    }
}
