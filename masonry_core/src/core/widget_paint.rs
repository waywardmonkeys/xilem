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

/// A reference to a value which may be owned or borrowed.
#[derive(Clone, Debug)]
pub enum Resolved<'a, T> {
    /// Borrowed from widget properties/defaults.
    Borrowed(&'a T),
    /// Owned override computed by a style resolver.
    Owned(T),
}

impl<T> AsRef<T> for Resolved<'_, T> {
    fn as_ref(&self) -> &T {
        match self {
            Self::Borrowed(v) => v,
            Self::Owned(v) => v,
        }
    }
}

/// References to common pre-paint properties.
pub struct PrePaintProps<'a> {
    /// Box shadow.
    pub box_shadow: &'a BoxShadow,
    /// Background.
    ///
    /// Considers disabled and active state.
    pub background: Resolved<'a, Background>,
    /// Border width.
    pub border_width: &'a BorderWidth,
    /// Border color.
    ///
    /// Considers focus and hovered state.
    pub border_color: Resolved<'a, BorderColor>,
    /// Corner radius,
    pub corner_radius: &'a CornerRadius,
}

impl<'a> PrePaintProps<'a> {
    /// Returns common pre-paint properties based on widget state.
    pub fn fetch(ctx: &mut PaintCtx<'_>, props: &'a PropertiesRef<'_>) -> Self {
        let box_shadow = props.get::<BoxShadow>();
        let pseudos = ctx.style_pseudos();
        let classes = props.get::<Classes>();
        let style = ctx
            .global_state
            .box_style_resolver
            .as_deref()
            .map(|r| r.resolve_box_paint(ctx.widget_type, pseudos, classes.as_slice()))
            .unwrap_or_default();

        let background = if props.contains::<Background>() {
            Resolved::Borrowed(props.get::<Background>())
        } else if let Some(bg) = style.background {
            Resolved::Owned(bg)
        } else if ctx.is_disabled()
            && let Some(db) = props.get_defined::<DisabledBackground>()
        {
            Resolved::Borrowed(&db.0)
        } else if ctx.is_active()
            && let Some(ab) = props.get_defined::<ActiveBackground>()
        {
            Resolved::Borrowed(&ab.0)
        } else {
            Resolved::Borrowed(props.get::<Background>())
        };

        let border_color = if props.contains::<BorderColor>() {
            Resolved::Borrowed(props.get::<BorderColor>())
        } else if let Some(color) = style.border_color {
            Resolved::Owned(color)
        } else if ctx.is_focus_target()
            && let Some(fb) = props.get_defined::<FocusedBorderColor>()
        {
            Resolved::Borrowed(&fb.0)
        } else if ctx.is_hovered()
            && let Some(hb) = props.get_defined::<HoveredBorderColor>()
        {
            Resolved::Borrowed(&hb.0)
        } else {
            Resolved::Borrowed(props.get::<BorderColor>())
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
