// Copyright 2026 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use vello::Scene;

use crate::core::{PaintCtx, PropertiesRef};
use crate::kurbo::{Affine, Join, Rect, Stroke};
use crate::peniko::Fill;
use crate::properties::{
    ActiveBackground, Background, BorderColor, BorderWidth, BoxShadow, Classes, CornerRadius,
    DisabledBackground, FocusedBorderColor, HoveredBorderColor,
};
use crate::style::StyleValue;

/// References to common pre-paint properties.
pub struct PrePaintProps<'a> {
    /// Box shadow.
    pub box_shadow: &'a BoxShadow,
    /// Background.
    ///
    /// Considers disabled and active state.
    pub background: StyleValue<'a, Background>,
    /// Border width.
    pub border_width: &'a BorderWidth,
    /// Border color.
    ///
    /// Considers focus and hovered state.
    pub border_color: StyleValue<'a, BorderColor>,
    /// Corner radius,
    pub corner_radius: &'a CornerRadius,
}

impl<'a> PrePaintProps<'a> {
    /// Returns common pre-paint properties based on widget state.
    pub fn fetch(ctx: &'a PaintCtx<'_>, props: &'a PropertiesRef<'_>) -> Self {
        let box_shadow = props.get::<BoxShadow>();
        let pseudos = ctx.style_pseudos_for_style();
        let classes = props.get::<Classes>();
        let classes = classes.as_arc_slice();
        let global_state = &*ctx.global_state;
        let has_style_resolver = global_state.style_resolver.is_some();
        let style = global_state
            .style_resolver
            .as_deref()
            .map(|r| r.resolve_box_paint(ctx.widget_type, pseudos, &classes))
            .unwrap_or_default();

        let background = if props.contains::<Background>() {
            StyleValue::Borrowed(props.get::<Background>())
        } else if let Some(bg) = style.background {
            bg
        } else if !has_style_resolver
            && ctx.is_disabled()
            && let Some(db) = props.get_defined::<DisabledBackground>()
        {
            StyleValue::Borrowed(&db.0)
        } else if !has_style_resolver
            && ctx.is_active()
            && let Some(ab) = props.get_defined::<ActiveBackground>()
        {
            StyleValue::Borrowed(&ab.0)
        } else {
            StyleValue::Borrowed(props.get::<Background>())
        };

        let border_color = if props.contains::<BorderColor>() {
            StyleValue::Borrowed(props.get::<BorderColor>())
        } else if let Some(color) = style.border_color {
            color
        } else if !has_style_resolver
            && ctx.is_focus_target()
            && let Some(fb) = props.get_defined::<FocusedBorderColor>()
        {
            StyleValue::Borrowed(&fb.0)
        } else if !has_style_resolver
            && ctx.is_hovered()
            && let Some(hb) = props.get_defined::<HoveredBorderColor>()
        {
            StyleValue::Borrowed(&hb.0)
        } else {
            StyleValue::Borrowed(props.get::<BorderColor>())
        };

        let border_width = props.get::<BorderWidth>();
        let corner_radius = props.get::<CornerRadius>();

        Self {
            box_shadow,
            background,
            border_width,
            border_color,
            corner_radius,
        }
    }
}

/// Paints the widget's box shadow, background, and border.
pub fn pre_paint(ctx: &mut PaintCtx<'_>, props: &PropertiesRef<'_>, scene: &mut Scene) {
    let bbox = ctx.border_box();
    let p = PrePaintProps::fetch(ctx, props);

    paint_box_shadow(scene, bbox, p.box_shadow, p.corner_radius);
    paint_background(
        scene,
        bbox,
        p.background.as_ref(),
        p.border_width,
        p.corner_radius,
    );
    paint_border(
        scene,
        bbox,
        p.border_color.as_ref(),
        p.border_width,
        p.corner_radius,
    );
}

/// Paints the widget's box shadow.
pub fn paint_box_shadow(
    scene: &mut Scene,
    border_box: Rect,
    box_shadow: &BoxShadow,
    corner_radius: &CornerRadius,
) {
    if !box_shadow.is_visible() {
        return;
    }
    let box_shadow_rect = border_box.to_rounded_rect(corner_radius.radius);
    box_shadow.paint(scene, Affine::IDENTITY, box_shadow_rect);
}

/// Paints the widget's background.
pub fn paint_background(
    scene: &mut Scene,
    border_box: Rect,
    background: &Background,
    border_width: &BorderWidth,
    corner_radius: &CornerRadius,
) {
    if !background.is_visible() {
        return;
    }
    // TODO: Fix remaining issues, see https://github.com/linebender/xilem/issues/1592
    //    1. Don't subtract the border from the background rect. Will need solution for border
    //       painting, as background should go exactly to the outer border and not beyond.
    let bg_rect = border_width.bg_rect(border_box, corner_radius);
    let bg_brush = background.get_peniko_brush_for_rect(bg_rect.rect());
    scene.fill(Fill::NonZero, Affine::IDENTITY, &bg_brush, None, &bg_rect);
}

/// Paints the widget's border.
pub fn paint_border(
    scene: &mut Scene,
    border_box: Rect,
    border_color: &BorderColor,
    border_width: &BorderWidth,
    corner_radius: &CornerRadius,
) {
    if border_width.width == 0. || !border_color.is_visible() {
        return;
    }
    let border_rect = border_width.border_rect(border_box, corner_radius);
    // Using Join::Miter avoids rounding corners when a widget has a wide border.
    let border_style = Stroke {
        width: border_width.width,
        join: Join::Miter,
        ..Default::default()
    };
    scene.stroke(
        &border_style,
        Affine::IDENTITY,
        border_color.color,
        None,
        &border_rect,
    );
}

#[cfg(test)]
mod tests {
    use super::PrePaintProps;

    use std::any::TypeId;
    use std::rc::Rc;
    use std::sync::Arc;

    use tree_arena::TreeArena;

    use crate::app::test_render_root_state_for_paint;
    use crate::core::{
        DefaultProperties, PaintCtx, Properties, PropertiesRef, WidgetArenaNode, WidgetId,
        WidgetOptions, WidgetState,
    };
    use crate::properties::{
        ActiveBackground, Background, BorderColor, ClassId, Classes, DisabledBackground,
        HoveredBorderColor,
    };
    use crate::style::{BoxPaintStyle, StylePseudos, StyleResolver};

    #[derive(Debug)]
    struct EmptyResolver;

    impl StyleResolver for EmptyResolver {
        fn resolve_box_paint(
            &self,
            _widget_type: TypeId,
            _pseudos: StylePseudos,
            _classes: &Arc<[ClassId]>,
        ) -> BoxPaintStyle<'_> {
            BoxPaintStyle::default()
        }
    }

    #[test]
    fn style_resolver_disables_legacy_state_fallbacks() {
        let widget_id = WidgetId::next();

        let mut widget_state = WidgetState::new(
            widget_id,
            "dummy",
            WidgetOptions::default(),
            TypeId::of::<()>(),
            #[cfg(debug_assertions)]
            "()",
        );
        widget_state.is_disabled = true;
        widget_state.is_hovered = true;
        widget_state.is_active = true;

        let base_bg = Background::Color(crate::peniko::Color::from_rgb8(1, 2, 3));
        let base_border = BorderColor {
            color: crate::peniko::Color::from_rgb8(4, 5, 6),
        };

        let legacy_disabled_bg = Background::Color(crate::peniko::Color::from_rgb8(10, 11, 12));
        let legacy_active_bg = Background::Color(crate::peniko::Color::from_rgb8(13, 14, 15));
        let legacy_hover_border = BorderColor {
            color: crate::peniko::Color::from_rgb8(16, 17, 18),
        };

        let mut props = Properties::new();
        props.insert(DisabledBackground(legacy_disabled_bg.clone()));
        props.insert(ActiveBackground(legacy_active_bg.clone()));
        props.insert(HoveredBorderColor(legacy_hover_border));
        // Ensure Classes exists so `PrePaintProps::fetch` doesn't depend on defaults.
        props.insert(Classes::default());

        let mut defaults = DefaultProperties::new();
        defaults.dummy_map.insert(base_bg.clone());
        defaults.dummy_map.insert(base_border);

        let props_ref = PropertiesRef {
            map: &props.map,
            default_map: &defaults.dummy_map,
        };

        let mut arena: TreeArena<WidgetArenaNode> = TreeArena::new();
        let children = arena.roots_mut();

        // With a resolver installed, legacy state properties should be ignored.
        let mut state_with_resolver =
            test_render_root_state_for_paint(Some(Rc::new(EmptyResolver)));
        let ctx = PaintCtx {
            global_state: &mut state_with_resolver,
            widget_state: &widget_state,
            widget_type: TypeId::of::<()>(),
            style_pseudos_extra: StylePseudos::EMPTY,
            children,
        };

        let p = PrePaintProps::fetch(&ctx, &props_ref);
        assert_eq!(p.background.as_ref(), &base_bg);
        assert_ne!(p.background.as_ref(), &legacy_disabled_bg);
        assert_ne!(p.background.as_ref(), &legacy_active_bg);
        assert_eq!(
            p.border_color.as_ref(),
            &BorderColor {
                color: crate::peniko::Color::from_rgb8(4, 5, 6)
            }
        );

        // Without a resolver, the legacy state fallbacks apply.
        let mut state_without_resolver = test_render_root_state_for_paint(None);
        let children = arena.roots_mut();
        let ctx = PaintCtx {
            global_state: &mut state_without_resolver,
            widget_state: &widget_state,
            widget_type: TypeId::of::<()>(),
            style_pseudos_extra: StylePseudos::EMPTY,
            children,
        };
        let p = PrePaintProps::fetch(&ctx, &props_ref);
        assert_eq!(p.background.as_ref(), &legacy_disabled_bg);
        assert_eq!(
            p.border_color.as_ref(),
            &BorderColor {
                color: crate::peniko::Color::from_rgb8(16, 17, 18)
            }
        );
    }

    #[test]
    fn style_resolver_still_allows_legacy_local_values() {
        let widget_id = WidgetId::next();
        let widget_state = WidgetState::new(
            widget_id,
            "dummy",
            WidgetOptions::default(),
            TypeId::of::<()>(),
            #[cfg(debug_assertions)]
            "()",
        );

        let local_bg = Background::Color(crate::peniko::Color::from_rgb8(50, 51, 52));
        let local_border = BorderColor {
            color: crate::peniko::Color::from_rgb8(53, 54, 55),
        };

        let mut props = Properties::new();
        props.insert(local_bg.clone());
        props.insert(local_border);
        props.insert(Classes::default());

        let defaults = DefaultProperties::new();
        let props_ref = PropertiesRef {
            map: &props.map,
            default_map: &defaults.dummy_map,
        };

        let mut arena: TreeArena<WidgetArenaNode> = TreeArena::new();
        let children = arena.roots_mut();
        let mut state = test_render_root_state_for_paint(Some(Rc::new(EmptyResolver)));
        let ctx = PaintCtx {
            global_state: &mut state,
            widget_state: &widget_state,
            widget_type: TypeId::of::<()>(),
            style_pseudos_extra: StylePseudos::EMPTY,
            children,
        };

        let p = PrePaintProps::fetch(&ctx, &props_ref);
        assert_eq!(p.background.as_ref(), &local_bg);
    }
}
