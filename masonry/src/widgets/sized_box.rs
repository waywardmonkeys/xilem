// Copyright 2019 the Xilem Authors and the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! A widget with predefined size.

use std::any::TypeId;

use accesskit::{Node, Role};
use smallvec::{SmallVec, smallvec};
use tracing::{Span, trace_span, warn};
use vello::Scene;
use vello::kurbo::{Affine, Point, RoundedRectRadii, Size};
use vello::peniko::{Brush, Fill};

use crate::core::{
    AccessCtx, AccessEvent, BoxConstraints, EventCtx, LayoutCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, UpdateCtx, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use crate::properties::{Background, Padding};
use crate::util::stroke;

// FIXME - Improve all doc in this module ASAP.

/// Something that can be used as the border for a widget.
struct BorderStyle {
    width: f64,
    brush: Brush,
}

// TODO - Have Widget type as generic argument

/// A widget with predefined size.
///
/// If given a child, this widget forces its child to have a specific width and/or height
/// (assuming values are permitted by this widget's parent). If either the width or height is not
/// set, this widget will size itself to match the child's size in that dimension.
///
/// If not given a child, `SizedBox` will try to size itself as close to the specified height
/// and width as possible given the parent's constraints. If height or width is not set,
/// it will be treated as zero.
///
#[doc = crate::include_screenshot!("sized_box_label_box_with_outer_padding.png", "Box with blue border, pink background and a child label.")]
pub struct SizedBox {
    child: Option<WidgetPod<dyn Widget>>,
    width: Option<f64>,
    height: Option<f64>,
    background: Option<Brush>,
    border: Option<BorderStyle>,
    corner_radius: RoundedRectRadii,
    padding: Padding,
}

// --- MARK: BUILDERS
impl SizedBox {
    /// Construct container with child, and both width and height not set.
    pub fn new(child: impl Widget) -> Self {
        Self {
            child: Some(WidgetPod::new(child).erased()),
            width: None,
            height: None,
            background: None,
            border: None,
            corner_radius: RoundedRectRadii::from_single_radius(0.0),
            padding: Padding::ZERO,
        }
    }

    /// Construct container with child, and both width and height not set.
    pub fn new_with_id(child: impl Widget, id: WidgetId) -> Self {
        Self {
            child: Some(WidgetPod::new_with_id(child, id).erased()),
            width: None,
            height: None,
            background: None,
            border: None,
            corner_radius: RoundedRectRadii::from_single_radius(0.0),
            padding: Padding::ZERO,
        }
    }

    /// Construct container with child in a pod, and both width and height not set.
    pub fn new_pod(child: WidgetPod<dyn Widget>) -> Self {
        Self {
            child: Some(child),
            width: None,
            height: None,
            background: None,
            border: None,
            corner_radius: RoundedRectRadii::from_single_radius(0.0),
            padding: Padding::ZERO,
        }
    }

    /// Construct container without child, and both width and height not set.
    ///
    /// If the widget is unchanged, it will render nothing, which can be useful if you want to draw a
    /// widget some of the time.
    #[doc(alias = "null")]
    pub fn empty() -> Self {
        Self {
            child: None,
            width: None,
            height: None,
            background: None,
            border: None,
            corner_radius: RoundedRectRadii::from_single_radius(0.0),
            padding: Padding::ZERO,
        }
    }

    /// Set container's width.
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    /// Set container's height.
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    /// Expand container to fit the parent.
    ///
    /// Only call this method if you want your widget to occupy all available
    /// space. If you only care about expanding in one of width or height, use
    /// [`expand_width`] or [`expand_height`] instead.
    ///
    /// [`expand_height`]: Self::expand_height
    /// [`expand_width`]: Self::expand_width
    pub fn expand(mut self) -> Self {
        self.width = Some(f64::INFINITY);
        self.height = Some(f64::INFINITY);
        self
    }

    /// Expand the container on the x-axis.
    ///
    /// This will force the child to have maximum width.
    pub fn expand_width(mut self) -> Self {
        self.width = Some(f64::INFINITY);
        self
    }

    /// Expand the container on the y-axis.
    ///
    /// This will force the child to have maximum height.
    pub fn expand_height(mut self) -> Self {
        self.height = Some(f64::INFINITY);
        self
    }

    /// Builder-style method for setting the background for this widget.
    ///
    /// This can be passed anything which can be represented by a [`Brush`];
    /// notably, it can be any [`Color`], any gradient, or an [`Image`].
    ///
    /// [`Image`]: vello::peniko::Image
    /// [`Color`]: crate::peniko::Color
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    /// Builder-style method for painting a border around the widget with a brush and width.
    pub fn border(mut self, brush: impl Into<Brush>, width: impl Into<f64>) -> Self {
        self.border = Some(BorderStyle {
            brush: brush.into(),
            width: width.into(),
        });
        self
    }

    /// Builder style method for rounding off corners of this container by setting a corner radius
    pub fn rounded(mut self, radius: impl Into<RoundedRectRadii>) -> Self {
        self.corner_radius = radius.into();
        self
    }

    /// Set the width directly. Intended for toolkits abstracting over `SizedBox`
    pub fn raw_width(mut self, value: Option<f64>) -> Self {
        self.width = value;
        self
    }

    /// Set the height directly. Intended for toolkits abstracting over `SizedBox`
    pub fn raw_height(mut self, value: Option<f64>) -> Self {
        self.height = value;
        self
    }

    /// Builder style method for specifying the padding added by the box.
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }
}

// --- MARK: WIDGETMUT
impl SizedBox {
    /// Give this container a child widget.
    ///
    /// If this container already has a child, it will be overwritten.
    pub fn set_child(this: &mut WidgetMut<'_, Self>, child: impl Widget) {
        if let Some(child) = this.widget.child.take() {
            this.ctx.remove_child(child);
        }
        this.widget.child = Some(WidgetPod::new(child).erased());
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Remove the child widget.
    ///
    /// (If this widget has no child, this method does nothing.)
    pub fn remove_child(this: &mut WidgetMut<'_, Self>) {
        if let Some(child) = this.widget.child.take() {
            this.ctx.remove_child(child);
        }
    }

    /// Set container's width.
    pub fn set_width(this: &mut WidgetMut<'_, Self>, width: f64) {
        this.widget.width = Some(width);
        this.ctx.request_layout();
    }

    /// Set container's height.
    pub fn set_height(this: &mut WidgetMut<'_, Self>, height: f64) {
        this.widget.height = Some(height);
        this.ctx.request_layout();
    }

    /// Set container's width.
    pub fn unset_width(this: &mut WidgetMut<'_, Self>) {
        this.widget.width = None;
        this.ctx.request_layout();
    }

    /// Set container's height.
    pub fn unset_height(this: &mut WidgetMut<'_, Self>) {
        this.widget.height = None;
        this.ctx.request_layout();
    }

    /// Set the background for this widget.
    ///
    /// This can be passed anything which can be represented by a [`Brush`];
    /// notably, it can be any [`Color`], any gradient, or an [`Image`].
    ///
    /// [`Image`]: vello::peniko::Image
    /// [`Color`]: crate::peniko::Color
    pub fn set_background(this: &mut WidgetMut<'_, Self>, brush: impl Into<Brush>) {
        this.widget.background = Some(brush.into());
        this.ctx.request_paint_only();
    }

    /// Clears background.
    pub fn clear_background(this: &mut WidgetMut<'_, Self>) {
        this.widget.background = None;
        this.ctx.request_paint_only();
    }

    /// Paint a border around the widget with a brush and width.
    pub fn set_border(
        this: &mut WidgetMut<'_, Self>,
        brush: impl Into<Brush>,
        width: impl Into<f64>,
    ) {
        this.widget.border = Some(BorderStyle {
            brush: brush.into(),
            width: width.into(),
        });
        this.ctx.request_layout();
    }

    /// Clears border.
    pub fn clear_border(this: &mut WidgetMut<'_, Self>) {
        this.widget.border = None;
        this.ctx.request_layout();
    }

    /// Round off corners of this container by setting a corner radius
    pub fn set_rounded(this: &mut WidgetMut<'_, Self>, radius: impl Into<RoundedRectRadii>) {
        this.widget.corner_radius = radius.into();
        this.ctx.request_paint_only();
    }

    /// Clears padding.
    pub fn clear_padding(this: &mut WidgetMut<'_, Self>) {
        Self::set_padding(this, Padding::ZERO);
    }

    /// Set the padding around this widget.
    pub fn set_padding(this: &mut WidgetMut<'_, Self>, padding: impl Into<Padding>) {
        this.widget.padding = padding.into();
        this.ctx.request_layout();
    }

    /// Get mutable reference to the child widget, if any.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> Option<WidgetMut<'t, dyn Widget>> {
        let child = this.widget.child.as_mut()?;
        Some(this.ctx.get_mut(child))
    }
}

// --- MARK: INTERNALS
impl SizedBox {
    fn child_constraints(&self, bc: &BoxConstraints) -> BoxConstraints {
        // if we don't have a width/height, we don't change that axis.
        // if we have a width/height, we clamp it on that axis.
        let (min_width, max_width) = match self.width {
            Some(width) => {
                let w = width.max(bc.min().width).min(bc.max().width);
                (w, w)
            }
            None => (bc.min().width, bc.max().width),
        };

        let (min_height, max_height) = match self.height {
            Some(height) => {
                let h = height.max(bc.min().height).min(bc.max().height);
                (h, h)
            }
            None => (bc.min().height, bc.max().height),
        };

        BoxConstraints::new(
            Size::new(min_width, min_height),
            Size::new(max_width, max_height),
        )
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SizedBox {
    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let Some(ref mut child) = self.child {
            ctx.register_child(child);
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        Background::prop_changed(ctx, property_type);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Shrink constraints by border offset
        let border_width = match &self.border {
            Some(border) => border.width,
            None => 0.0,
        };

        let child_bc = self.child_constraints(bc);
        let child_bc = child_bc.shrink((2.0 * border_width, 2.0 * border_width));
        let origin = Point::new(border_width, border_width);

        // Shrink constraints by padding inset
        let padding_size = Size::new(
            self.padding.left + self.padding.right,
            self.padding.top + self.padding.bottom,
        );
        let child_bc = child_bc.shrink(padding_size);
        let origin = origin + (self.padding.left, self.padding.top);

        let mut size;
        match self.child.as_mut() {
            Some(child) => {
                size = ctx.run_layout(child, &child_bc);
                ctx.place_child(child, origin);
                size = Size::new(
                    size.width + 2.0 * border_width,
                    size.height + 2.0 * border_width,
                ) + padding_size;
            }
            None => size = bc.constrain((self.width.unwrap_or(0.0), self.height.unwrap_or(0.0))),
        };

        // TODO - figure out paint insets
        // TODO - figure out baseline offset

        if size.width.is_infinite() {
            warn!("SizedBox is returning an infinite width.");
        }
        if size.height.is_infinite() {
            warn!("SizedBox is returning an infinite height.");
        }

        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, props: &PropertiesRef<'_>, scene: &mut Scene) {
        let corner_radius = self.corner_radius;

        // TODO - Handle properties more gracefully.
        // This is more of a proof of concept.
        let background = self.background.clone().or_else(|| {
            // TODO - bg_rect should account for border width
            let bg_rect = ctx.size().to_rect();
            // TODO - Remove `Some()`
            Some(props.get::<Background>().get_peniko_brush_for_rect(bg_rect))
        });

        if let Some(background) = background {
            let panel = ctx.size().to_rounded_rect(corner_radius);

            trace_span!("paint background").in_scope(|| {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    &background,
                    Some(Affine::IDENTITY),
                    &panel,
                );
            });
        }

        if let Some(border) = &self.border {
            let border_width = border.width;
            let border_rect = ctx
                .size()
                .to_rect()
                .inset(border_width / -2.0)
                .to_rounded_rect(corner_radius);
            stroke(scene, &border_rect, &border.brush, border_width);
        };
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> SmallVec<[WidgetId; 16]> {
        if let Some(child) = &self.child {
            smallvec![child.id()]
        } else {
            smallvec![]
        }
    }

    fn make_trace_span(&self, ctx: &QueryCtx<'_>) -> Span {
        trace_span!("SizedBox", id = ctx.widget_id().trace())
    }
}

// --- MARK: TESTS
#[cfg(test)]
mod tests {

    use vello::peniko::Gradient;

    use super::*;
    use crate::palette;
    use crate::testing::{TestHarness, assert_failing_render_snapshot, assert_render_snapshot};
    use crate::theme::default_property_set;
    use crate::widgets::Label;

    // TODO - Add WidgetMut tests

    #[test]
    fn expand() {
        let expand = SizedBox::new(Label::new("hello!")).expand();
        let bc = BoxConstraints::tight(Size::new(400., 400.)).loosen();
        let child_bc = expand.child_constraints(&bc);
        assert_eq!(child_bc.min(), Size::new(400., 400.,));
    }

    #[test]
    fn no_width() {
        let expand = SizedBox::new(Label::new("hello!")).height(200.);
        let bc = BoxConstraints::tight(Size::new(400., 400.)).loosen();
        let child_bc = expand.child_constraints(&bc);
        assert_eq!(child_bc.min(), Size::new(0., 200.,));
        assert_eq!(child_bc.max(), Size::new(400., 200.,));
    }

    #[test]
    fn empty_box() {
        let widget = SizedBox::empty()
            .width(20.0)
            .height(20.0)
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_empty_box");
    }

    #[test]
    fn label_box_no_size() {
        let widget = SizedBox::new(Label::new("hello"))
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_no_size");
    }

    #[test]
    fn label_box_with_size() {
        let widget = SizedBox::new(Label::new("hello"))
            .width(20.0)
            .height(20.0)
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_with_size");
    }

    #[test]
    fn label_box_with_padding() {
        let widget = SizedBox::new(Label::new("hello"))
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0)
            .padding(Padding::from_vh(15., 10.));

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_with_padding");
    }

    #[test]
    fn label_box_with_solid_background() {
        let widget = SizedBox::new(Label::new("hello"))
            .width(20.0)
            .height(20.0)
            .background(palette::css::PLUM);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_with_solid_background");
    }

    #[test]
    fn empty_box_with_gradient_background() {
        let widget = SizedBox::empty()
            .width(20.)
            .height(20.)
            .rounded(10.)
            .border(palette::css::LIGHT_SKY_BLUE, 5.)
            .background(
                Gradient::new_sweep((30., 30.), 0., std::f32::consts::TAU).with_stops([
                    (0., palette::css::WHITE),
                    (0.25, palette::css::BLACK),
                    (0.5, palette::css::RED),
                    (0.75, palette::css::GREEN),
                    (1., palette::css::WHITE),
                ]),
            );

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_empty_box_with_gradient_background");
    }

    #[test]
    fn label_box_with_padding_and_background() {
        let widget = SizedBox::new(Label::new("hello"))
            .width(20.0)
            .height(20.0)
            .background(palette::css::PLUM)
            .border(palette::css::LIGHT_SKY_BLUE, 5.)
            .padding(25.);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_with_background_and_padding");
    }

    #[test]
    fn label_box_with_padding_outside() {
        let widget = SizedBox::new(
            SizedBox::new(Label::new("hello"))
                .width(20.0)
                .height(20.0)
                .background(palette::css::PLUM)
                .border(palette::css::LIGHT_SKY_BLUE, 5.),
        )
        .padding(25.);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_render_snapshot!(harness, "sized_box_label_box_with_outer_padding");
    }

    // TODO - add screenshot tests for different brush types

    // --- MARK: PROP TESTS

    #[test]
    fn background_brush_property() {
        let widget = SizedBox::empty().width(20.).height(20.).rounded(10.);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        harness.edit_root_widget(|mut sized_box| {
            let brush = Background::Color(palette::css::RED);
            sized_box.insert_prop(brush);
        });
        assert_render_snapshot!(harness, "sized_box_background_brush_red");

        harness.edit_root_widget(|mut sized_box| {
            let brush = Background::Color(palette::css::GREEN);
            sized_box.insert_prop(brush);
        });
        assert_render_snapshot!(harness, "sized_box_background_brush_green");

        harness.edit_root_widget(|mut sized_box| {
            let brush = Background::Color(palette::css::BLUE);
            sized_box.insert_prop(brush);
        });
        assert_render_snapshot!(harness, "sized_box_background_brush_blue");

        harness.edit_root_widget(|mut sized_box| {
            sized_box.remove_prop::<Background>();
        });
        assert_render_snapshot!(harness, "sized_box_background_brush_removed");
    }

    #[test]
    fn invalid_screenshot() {
        // Copy-pasted from empty_box
        let widget = SizedBox::empty()
            .width(20.0)
            .height(20.0)
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0);

        // This is the difference
        let widget = widget.border(palette::css::BLUE, 5.2);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_failing_render_snapshot!(harness, "sized_box_empty_box");
    }

    #[test]
    fn invalid_screenshot_2() {
        // Copy-pasted from label_box_with_size
        let widget = SizedBox::new(Label::new("hello"))
            .width(20.0)
            .height(20.0)
            .border(palette::css::BLUE, 5.0)
            .rounded(5.0);

        // This is the difference
        let widget = widget.padding(0.2);

        let window_size = Size::new(100.0, 100.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        assert_failing_render_snapshot!(harness, "sized_box_label_box_with_size");
    }
}
