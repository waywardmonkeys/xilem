// Copyright 2018 the Xilem Authors and the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! A widget that arranges its children in a one-dimensional array.

use std::any::TypeId;

use accesskit::{Node, Role};
use masonry_core::core::RegisterCtx;
use smallvec::SmallVec;
use tracing::{Span, trace_span};
use vello::Scene;
use vello::kurbo::common::FloatExt;
use vello::kurbo::{Affine, Line, Point, Rect, Size, Stroke, Vec2};

use crate::core::{
    AccessCtx, BoxConstraints, LayoutCtx, PaintCtx, PropertiesMut, PropertiesRef, QueryCtx,
    UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use crate::debug_panic;
use crate::properties::{Background, BorderColor, BorderWidth, CornerRadius, Padding};
use crate::util::{fill, stroke};

/// A container with either horizontal or vertical layout.
///
/// This widget is the foundation of most layouts, and is highly configurable.
///
#[doc = crate::include_screenshot!("flex_col_main_axis_spaceAround.png", "Flex column with multiple labels.")]
pub struct Flex {
    direction: Axis,
    cross_alignment: CrossAxisAlignment,
    main_alignment: MainAxisAlignment,
    fill_major_axis: bool,
    children: Vec<Child>,
    old_bc: BoxConstraints,
    gap: Option<f64>,
}

/// Optional parameters for an item in a [`Flex`] container (row or column).
///
/// Generally, when you would like to add a flexible child to a container,
/// you can simply call [`with_flex_child`](Flex::with_flex_child) or [`add_flex_child`](Flex::add_flex_child),
/// passing the child and the desired flex factor as a `f64`, which has an impl of
/// `Into<FlexParams>`.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct FlexParams {
    flex: Option<f64>,
    alignment: Option<CrossAxisAlignment>,
}

/// An axis in visual space.
///
/// Most often used by widgets to describe
/// the direction in which they grow as their number of children increases.
/// Has some methods for manipulating geometry with respect to the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    /// The x axis
    Horizontal,
    /// The y axis
    Vertical,
}

/// The alignment of the widgets on the container's cross (or minor) axis.
///
/// If a widget is smaller than the container on the minor axis, this determines
/// where it is positioned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrossAxisAlignment {
    /// Top or left.
    Start,
    /// Widgets are centered in the container.
    Center,
    /// Bottom or right.
    End,
    /// Align on the baseline.
    Baseline,
    /// Fill the available space.
    Fill,
}

/// Arrangement of children on the main axis.
///
/// If there is surplus space on the main axis after laying out children, this
/// enum represents how children are laid out in this space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MainAxisAlignment {
    /// Top or left.
    Start,
    /// Children are centered, without padding.
    Center,
    /// Bottom or right.
    End,
    /// Extra space is divided evenly between each child.
    SpaceBetween,
    /// Extra space is divided evenly between each child, as well as at the ends.
    SpaceEvenly,
    /// Space between each child, with less at the start and end.
    SpaceAround,
}

struct Spacing {
    alignment: MainAxisAlignment,
    extra: f64,
    n_children: usize,
    index: usize,
    equal_space: f64,
    remainder: f64,
}

enum Child {
    Fixed {
        widget: WidgetPod<dyn Widget>,
        alignment: Option<CrossAxisAlignment>,
    },
    Flex {
        widget: WidgetPod<dyn Widget>,
        alignment: Option<CrossAxisAlignment>,
        flex: f64,
    },
    FixedSpacer(f64, f64),
    FlexedSpacer(f64, f64),
}

// --- MARK: IMPL FLEX
impl Flex {
    /// Create a new `Flex` oriented along the provided axis.
    pub fn for_axis(axis: Axis) -> Self {
        Self {
            direction: axis,
            children: Vec::new(),
            cross_alignment: CrossAxisAlignment::Center,
            main_alignment: MainAxisAlignment::Start,
            fill_major_axis: false,
            old_bc: BoxConstraints::tight(Size::ZERO),
            gap: None,
        }
    }

    /// Create a new horizontal stack.
    ///
    /// The child widgets are laid out horizontally, from left to right.
    ///
    pub fn row() -> Self {
        Self::for_axis(Axis::Horizontal)
    }

    /// Create a new vertical stack.
    ///
    /// The child widgets are laid out vertically, from top to bottom.
    pub fn column() -> Self {
        Self::for_axis(Axis::Vertical)
    }

    /// Builder-style method for specifying the childrens' [`CrossAxisAlignment`].
    pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_alignment = alignment;
        self
    }

    /// Builder-style method for specifying the childrens' [`MainAxisAlignment`].
    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_alignment = alignment;
        self
    }

    /// Builder-style method for setting whether the container must expand
    /// to fill the available space on its main axis.
    pub fn must_fill_main_axis(mut self, fill: bool) -> Self {
        self.fill_major_axis = fill;
        self
    }

    /// Builder-style method for setting the spacing along the
    /// major axis between any two elements in logical pixels.
    ///
    /// Equivalent to the css [gap] property.
    /// This gap is also present between spacers.
    ///
    /// See also [`default_gap`](Self::default_gap).
    ///
    /// ## Panics
    ///
    /// If `gap` is not a non-negative finite value.
    ///
    /// [gap]: https://developer.mozilla.org/en-US/docs/Web/CSS/gap
    // TODO: Semantics - should this include fixed spacers?
    pub fn gap(mut self, gap: f64) -> Self {
        if gap.is_finite() && gap >= 0.0 {
            self.gap = Some(gap);
        } else {
            panic!("Invalid `gap` {gap}, expected a non-negative finite value.")
        }
        self
    }

    /// Builder-style method to use the default gap value.
    ///
    /// This is [`WIDGET_PADDING_VERTICAL`] for a flex column and
    /// [`WIDGET_PADDING_HORIZONTAL`] for flex row.
    ///
    /// See also [`gap`](Self::gap)
    ///
    /// [`WIDGET_PADDING_VERTICAL`]: crate::theme::WIDGET_PADDING_VERTICAL
    /// [`WIDGET_PADDING_HORIZONTAL`]: crate::theme::WIDGET_PADDING_VERTICAL
    pub fn default_gap(mut self) -> Self {
        self.gap = None;
        self
    }

    /// Equivalent to [`gap`](Self::gap) if `gap` is `Some`, or
    /// [`default_gap`](Self::default_gap) otherwise.
    ///
    /// Does not perform validation of the provided value.
    pub fn raw_gap(mut self, gap: Option<f64>) -> Self {
        self.gap = gap;
        self
    }

    /// Builder-style variant of [`Flex::add_child`].
    ///
    /// Convenient for assembling a group of widgets in a single expression.
    pub fn with_child(self, child: impl Widget) -> Self {
        self.with_child_pod(WidgetPod::new(child).erased())
    }

    /// Builder-style variant of [`Flex::add_child`], that takes the id that the child will have.
    ///
    /// Useful for unit tests.
    pub fn with_child_id(self, child: impl Widget, id: WidgetId) -> Self {
        self.with_child_pod(WidgetPod::new_with_id(child, id).erased())
    }

    /// Builder-style method for [adding](Flex::add_child) a type-erased child to this.
    pub fn with_child_pod(mut self, widget: WidgetPod<dyn Widget>) -> Self {
        let child = Child::Fixed {
            widget,
            alignment: None,
        };
        self.children.push(child);
        self
    }

    /// Builder-style method to add a flexible child to the container.
    pub fn with_flex_child(self, child: impl Widget, params: impl Into<FlexParams>) -> Self {
        self.with_flex_child_pod(WidgetPod::new(child).erased(), params)
    }

    /// Builder-style method to add a flexible child to the container.
    pub fn with_flex_child_pod(
        mut self,
        widget: WidgetPod<dyn Widget>,
        params: impl Into<FlexParams>,
    ) -> Self {
        // TODO - dedup?
        let params: FlexParams = params.into();

        let child = new_flex_child(params, widget);
        self.children.push(child);
        self
    }

    /// Builder-style method to add a spacer widget with a standard size.
    ///
    /// The actual value of this spacer depends on whether this container is
    /// a row or column, as well as theme settings.
    pub fn with_default_spacer(self) -> Self {
        let key = axis_default_spacer(self.direction);
        self.with_spacer(key)
    }

    /// Builder-style method for adding a fixed-size spacer to the container.
    ///
    /// If you are laying out standard controls in this container, you should
    /// generally prefer to use [`add_default_spacer`].
    ///
    /// [`add_default_spacer`]: Flex::add_default_spacer
    pub fn with_spacer(mut self, mut len: f64) -> Self {
        if len < 0.0 {
            tracing::warn!("add_spacer called with negative length: {}", len);
        }
        len = len.clamp(0.0, f64::MAX);

        let new_child = Child::FixedSpacer(len, 0.0);
        self.children.push(new_child);
        self
    }

    /// Builder-style method for adding a `flex` spacer to the container.
    pub fn with_flex_spacer(mut self, flex: f64) -> Self {
        let flex = if flex >= 0.0 {
            flex
        } else {
            debug_panic!("add_spacer called with negative length: {}", flex);
            0.0
        };
        let new_child = Child::FlexedSpacer(flex, 0.0);
        self.children.push(new_child);
        self
    }

    /// Returns the number of children (widgets and spacers) this flex container has.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Returns `true` if this flex container has no children (widgets or spacers).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// --- MARK: WIDGETMUT---
impl Flex {
    /// Set the flex direction (see [`Axis`]).
    pub fn set_direction(this: &mut WidgetMut<'_, Self>, direction: Axis) {
        this.widget.direction = direction;
        this.ctx.request_layout();
    }

    /// Set the childrens' [`CrossAxisAlignment`].
    pub fn set_cross_axis_alignment(this: &mut WidgetMut<'_, Self>, alignment: CrossAxisAlignment) {
        this.widget.cross_alignment = alignment;
        this.ctx.request_layout();
    }

    /// Set the childrens' [`MainAxisAlignment`].
    pub fn set_main_axis_alignment(this: &mut WidgetMut<'_, Self>, alignment: MainAxisAlignment) {
        this.widget.main_alignment = alignment;
        this.ctx.request_layout();
    }

    /// Set whether the container must expand to fill the available space on
    /// its main axis.
    pub fn set_must_fill_main_axis(this: &mut WidgetMut<'_, Self>, fill: bool) {
        this.widget.fill_major_axis = fill;
        this.ctx.request_layout();
    }

    /// Set the spacing along the major axis between any two elements in logical pixels.
    ///
    /// Equivalent to the css [gap] property.
    /// This gap is also present between spacers.
    ///
    /// [gap]: https://developer.mozilla.org/en-US/docs/Web/CSS/gap
    ///
    /// ## Panics
    ///
    /// If `gap` is not a non-negative finite value.
    ///
    /// See also [`use_default_gap`](Self::use_default_gap).
    pub fn set_gap(this: &mut WidgetMut<'_, Self>, gap: f64) {
        if gap.is_finite() && gap >= 0.0 {
            this.widget.gap = Some(gap);
        } else {
            panic!("Invalid `gap` {gap}, expected a non-negative finite value.")
        }
        this.ctx.request_layout();
    }

    /// Use the default gap value.
    ///
    /// This is [`WIDGET_PADDING_VERTICAL`] for a flex column and
    /// [`WIDGET_PADDING_HORIZONTAL`] for flex row.
    ///
    /// See also [`set_gap`](Self::set_gap)
    ///
    /// [`WIDGET_PADDING_VERTICAL`]: crate::theme::WIDGET_PADDING_VERTICAL
    /// [`WIDGET_PADDING_HORIZONTAL`]: crate::theme::WIDGET_PADDING_VERTICAL
    pub fn use_default_gap(this: &mut WidgetMut<'_, Self>) {
        this.widget.gap = None;
        this.ctx.request_layout();
    }

    /// Equivalent to [`set_gap`](Self::set_gap) if `gap` is `Some`, or
    /// [`use_default_gap`](Self::use_default_gap) otherwise.
    ///
    /// Does not perform validation of the provided value.
    pub fn set_raw_gap(this: &mut WidgetMut<'_, Self>, gap: Option<f64>) {
        this.widget.gap = gap;
        this.ctx.request_layout();
    }

    /// Add a non-flex child widget.
    ///
    /// See also [`with_child`].
    ///
    /// [`with_child`]: Flex::with_child
    pub fn add_child(this: &mut WidgetMut<'_, Self>, child: impl Widget) {
        let child = Child::Fixed {
            widget: WidgetPod::new(child).erased(),
            alignment: None,
        };
        this.widget.children.push(child);
        this.ctx.children_changed();
    }

    /// Add a non-flex child widget with a pre-assigned id.
    ///
    /// See also [`with_child_id`].
    ///
    /// [`with_child_id`]: Flex::with_child_id
    pub fn add_child_id(this: &mut WidgetMut<'_, Self>, child: impl Widget, id: WidgetId) {
        let child = Child::Fixed {
            widget: WidgetPod::new_with_id(child, id).erased(),
            alignment: None,
        };
        this.widget.children.push(child);
        this.ctx.children_changed();
    }

    /// Add a flexible child widget.
    pub fn add_flex_child(
        this: &mut WidgetMut<'_, Self>,
        child: impl Widget,
        params: impl Into<FlexParams>,
    ) {
        let params = params.into();
        let child = new_flex_child(params, WidgetPod::new(child).erased());

        this.widget.children.push(child);
        this.ctx.children_changed();
    }

    /// Add a spacer widget with a standard size.
    ///
    /// The actual value of this spacer depends on whether this container is
    /// a row or column, as well as theme settings.
    pub fn add_default_spacer(this: &mut WidgetMut<'_, Self>) {
        let key = axis_default_spacer(this.widget.direction);
        Self::add_spacer(this, key);
        this.ctx.request_layout();
    }

    /// Add an empty spacer widget with the given size.
    ///
    /// If you are laying out standard controls in this container, you should
    /// generally prefer to use [`add_default_spacer`].
    ///
    /// [`add_default_spacer`]: Flex::add_default_spacer
    pub fn add_spacer(this: &mut WidgetMut<'_, Self>, mut len: f64) {
        if len < 0.0 {
            tracing::warn!("add_spacer called with negative length: {}", len);
        }
        len = len.clamp(0.0, f64::MAX);

        let new_child = Child::FixedSpacer(len, 0.0);
        this.widget.children.push(new_child);
        this.ctx.request_layout();
    }

    /// Add an empty spacer widget with a specific `flex` factor.
    pub fn add_flex_spacer(this: &mut WidgetMut<'_, Self>, flex: f64) {
        let flex = if flex >= 0.0 {
            flex
        } else {
            debug_panic!("add_spacer called with negative length: {}", flex);
            0.0
        };
        let new_child = Child::FlexedSpacer(flex, 0.0);
        this.widget.children.push(new_child);
        this.ctx.request_layout();
    }

    /// Insert a non-flex child widget at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_child(this: &mut WidgetMut<'_, Self>, idx: usize, child: impl Widget) {
        Self::insert_child_pod(this, idx, WidgetPod::new(child).erased());
    }

    /// Insert a non-flex child widget wrapped in a [`WidgetPod`] at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_child_pod(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        widget: WidgetPod<dyn Widget>,
    ) {
        let child = Child::Fixed {
            widget,
            alignment: None,
        };
        this.widget.children.insert(idx, child);
        this.ctx.children_changed();
    }

    /// Insert a flex child widget at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_flex_child(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: impl Widget,
        params: impl Into<FlexParams>,
    ) {
        Self::insert_flex_child_pod(this, idx, WidgetPod::new(child).erased(), params);
    }

    /// Insert a flex child widget wrapped in a [`WidgetPod`] at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_flex_child_pod(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: WidgetPod<dyn Widget>,
        params: impl Into<FlexParams>,
    ) {
        let child = new_flex_child(params.into(), child);
        this.widget.children.insert(idx, child);
        this.ctx.children_changed();
    }

    // TODO - remove
    /// Insert a spacer widget with a standard size at the given index.
    ///
    /// The actual value of this spacer depends on whether this container is
    /// a row or column, as well as theme settings.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_default_spacer(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let key = axis_default_spacer(this.widget.direction);
        Self::insert_spacer(this, idx, key);
        this.ctx.request_layout();
    }

    /// Insert an empty spacer widget with the given size at the given index.
    ///
    /// If you are laying out standard controls in this container, you should
    /// generally prefer to use [`add_default_spacer`].
    ///
    /// [`add_default_spacer`]: Flex::add_default_spacer
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_spacer(this: &mut WidgetMut<'_, Self>, idx: usize, mut len: f64) {
        if len < 0.0 {
            tracing::warn!("add_spacer called with negative length: {}", len);
        }
        len = len.clamp(0.0, f64::MAX);

        let new_child = Child::FixedSpacer(len, 0.0);
        this.widget.children.insert(idx, new_child);
        this.ctx.request_layout();
    }

    /// Add an empty spacer widget with a specific `flex` factor.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn insert_flex_spacer(this: &mut WidgetMut<'_, Self>, idx: usize, flex: f64) {
        let flex = if flex >= 0.0 {
            flex
        } else {
            debug_panic!("add_spacer called with negative length: {}", flex);
            0.0
        };
        let new_child = Child::FlexedSpacer(flex, 0.0);
        this.widget.children.insert(idx, new_child);
        this.ctx.request_layout();
    }

    /// Remove the child at `idx`.
    ///
    /// This child can be a widget or a spacer.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn remove_child(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let child = this.widget.children.remove(idx);
        if let Child::Fixed { widget, .. } | Child::Flex { widget, .. } = child {
            this.ctx.remove_child(widget);
        }
        this.ctx.request_layout();
    }

    /// Returns a mutable reference to the child widget at `idx`.
    ///
    /// Returns `None` if the child at `idx` is a spacer.
    ///
    /// # Panics
    ///
    /// Panics if the index is larger than the number of children.
    pub fn child_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        idx: usize,
    ) -> Option<WidgetMut<'t, dyn Widget>> {
        let child = match &mut this.widget.children[idx] {
            Child::Fixed { widget, .. } | Child::Flex { widget, .. } => widget,
            Child::FixedSpacer(..) => return None,
            Child::FlexedSpacer(..) => return None,
        };

        Some(this.ctx.get_mut(child))
    }

    /// Updates the flex parameters for the child at `idx`,
    ///
    /// # Panics
    ///
    /// Panics if the element at `idx` is not a widget.
    pub fn update_child_flex_params(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        params: impl Into<FlexParams>,
    ) {
        let child = &mut this.widget.children[idx];
        let child_val = std::mem::replace(child, Child::FixedSpacer(0.0, 0.0));
        let widget = match child_val {
            Child::Fixed { widget, .. } | Child::Flex { widget, .. } => widget,
            _ => {
                panic!("Can't update flex parameters of a spacer element");
            }
        };
        let new_child = new_flex_child(params.into(), widget);
        *child = new_child;
        this.ctx.children_changed();
    }

    /// Updates the spacer at `idx`, if the spacer was a fixed spacer, it will be overwritten with a flex spacer
    ///
    /// # Panics
    ///
    /// Panics if the element at `idx` is not a spacer.
    pub fn update_spacer_flex(this: &mut WidgetMut<'_, Self>, idx: usize, flex: f64) {
        let child = &mut this.widget.children[idx];

        match *child {
            Child::FixedSpacer(_, _) | Child::FlexedSpacer(_, _) => {
                *child = Child::FlexedSpacer(flex, 0.0);
            }
            _ => {
                panic!("Can't update spacer parameters of a non-spacer element");
            }
        };
        this.ctx.children_changed();
    }

    /// Updates the spacer at `idx`, if the spacer was a flex spacer, it will be overwritten with a fixed spacer
    ///
    /// # Panics
    ///
    /// Panics if the element at `idx` is not a spacer.
    pub fn update_spacer_fixed(this: &mut WidgetMut<'_, Self>, idx: usize, len: f64) {
        let child = &mut this.widget.children[idx];

        match *child {
            Child::FixedSpacer(_, _) | Child::FlexedSpacer(_, _) => {
                *child = Child::FixedSpacer(len, 0.0);
            }
            _ => {
                panic!("Can't update spacer parameters of a non-spacer element");
            }
        };
        this.ctx.children_changed();
    }

    /// Remove all children from the container.
    pub fn clear(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.children.is_empty() {
            this.ctx.request_layout();

            for child in this.widget.children.drain(..) {
                if let Child::Fixed { widget, .. } | Child::Flex { widget, .. } = child {
                    this.ctx.remove_child(widget);
                }
            }
        }
    }
}

// --- MARK: OTHER IMPLS---
impl Axis {
    /// Get the axis perpendicular to this one.
    pub fn cross(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    /// Extract from the argument the magnitude along this axis
    pub fn major(self, size: Size) -> f64 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    /// Extract from the argument the magnitude along the perpendicular axis
    pub fn minor(self, size: Size) -> f64 {
        self.cross().major(size)
    }

    /// Extract the extent of the argument in this axis as a pair.
    pub fn major_span(self, rect: Rect) -> (f64, f64) {
        match self {
            Self::Horizontal => (rect.x0, rect.x1),
            Self::Vertical => (rect.y0, rect.y1),
        }
    }

    /// Extract the extent of the argument in the minor axis as a pair.
    pub fn minor_span(self, rect: Rect) -> (f64, f64) {
        self.cross().major_span(rect)
    }

    /// Extract the coordinate locating the argument with respect to this axis.
    pub fn major_pos(self, pos: Point) -> f64 {
        match self {
            Self::Horizontal => pos.x,
            Self::Vertical => pos.y,
        }
    }

    /// Extract the coordinate locating the argument with respect to this axis.
    pub fn major_vec(self, vec: Vec2) -> f64 {
        match self {
            Self::Horizontal => vec.x,
            Self::Vertical => vec.y,
        }
    }

    /// Extract the coordinate locating the argument with respect to the perpendicular axis.
    pub fn minor_pos(self, pos: Point) -> f64 {
        self.cross().major_pos(pos)
    }

    /// Extract the coordinate locating the argument with respect to the perpendicular axis.
    pub fn minor_vec(self, vec: Vec2) -> f64 {
        self.cross().major_vec(vec)
    }

    // TODO - make_pos, make_size, make_rect
    /// Arrange the major and minor measurements with respect to this axis such that it forms
    /// an (x, y) pair.
    pub fn pack(self, major: f64, minor: f64) -> (f64, f64) {
        match self {
            Self::Horizontal => (major, minor),
            Self::Vertical => (minor, major),
        }
    }

    /// Generate constraints with new values on the major axis.
    pub(crate) fn constraints(
        self,
        bc: &BoxConstraints,
        min_major: f64,
        major: f64,
    ) -> BoxConstraints {
        match self {
            Self::Horizontal => BoxConstraints::new(
                Size::new(min_major, bc.min().height),
                Size::new(major, bc.max().height),
            ),
            Self::Vertical => BoxConstraints::new(
                Size::new(bc.min().width, min_major),
                Size::new(bc.max().width, major),
            ),
        }
    }
}

impl FlexParams {
    /// Create custom `FlexParams` with a specific `flex_factor` and an optional
    /// [`CrossAxisAlignment`].
    ///
    /// You likely only need to create these manually if you need to specify
    /// a custom alignment; if you only need to use a custom `flex_factor` you
    /// can pass an `f64` to any of the functions that take `FlexParams`.
    ///
    /// By default, the widget uses the alignment of its parent [`Flex`] container.
    pub fn new(
        flex: impl Into<Option<f64>>,
        alignment: impl Into<Option<CrossAxisAlignment>>,
    ) -> Self {
        let flex = match flex.into() {
            Some(flex) if flex <= 0.0 => {
                debug_panic!("Flex value should be > 0.0. Flex given was: {}", flex);
                Some(0.0)
            }
            other => other,
        };

        Self {
            flex,
            alignment: alignment.into(),
        }
    }
}

impl CrossAxisAlignment {
    /// Given the difference between the size of the container and the size
    /// of the child (on their minor axis) return the necessary offset for
    /// this alignment.
    fn align(self, val: f64) -> f64 {
        match self {
            Self::Start => 0.0,
            // in vertical layout, baseline is equivalent to center
            Self::Center | Self::Baseline => (val / 2.0).round(),
            Self::End => val,
            Self::Fill => 0.0,
        }
    }
}

impl Spacing {
    /// Given the provided extra space and children count,
    /// this returns an iterator of `f64` spacing,
    /// where the first element is the spacing before any children
    /// and all subsequent elements are the spacing after children.
    fn new(alignment: MainAxisAlignment, extra: f64, n_children: usize) -> Self {
        let extra = if extra.is_finite() { extra } else { 0. };
        let equal_space = if n_children > 0 {
            match alignment {
                MainAxisAlignment::Center => extra / 2.,
                MainAxisAlignment::SpaceBetween => extra / (n_children - 1).max(1) as f64,
                MainAxisAlignment::SpaceEvenly => extra / (n_children + 1) as f64,
                MainAxisAlignment::SpaceAround => extra / (2 * n_children) as f64,
                _ => 0.,
            }
        } else {
            0.
        };
        Self {
            alignment,
            extra,
            n_children,
            index: 0,
            equal_space,
            remainder: 0.,
        }
    }

    fn next_space(&mut self) -> f64 {
        let desired_space = self.equal_space + self.remainder;
        let actual_space = desired_space.round();
        self.remainder = desired_space - actual_space;
        actual_space
    }
}

impl Iterator for Spacing {
    type Item = f64;

    fn next(&mut self) -> Option<f64> {
        if self.index > self.n_children {
            return None;
        }
        let result = {
            if self.n_children == 0 {
                self.extra
            } else {
                #[allow(clippy::match_bool, reason = "readability")]
                match self.alignment {
                    MainAxisAlignment::Start => match self.index == self.n_children {
                        true => self.extra,
                        false => 0.,
                    },
                    MainAxisAlignment::End => match self.index == 0 {
                        true => self.extra,
                        false => 0.,
                    },
                    MainAxisAlignment::Center => match self.index {
                        0 => self.next_space(),
                        i if i == self.n_children => self.next_space(),
                        _ => 0.,
                    },
                    MainAxisAlignment::SpaceBetween => match self.index {
                        0 => 0.,
                        i if i != self.n_children => self.next_space(),
                        _ => match self.n_children {
                            1 => self.next_space(),
                            _ => 0.,
                        },
                    },
                    MainAxisAlignment::SpaceEvenly => self.next_space(),
                    MainAxisAlignment::SpaceAround => {
                        if self.index == 0 || self.index == self.n_children {
                            self.next_space()
                        } else {
                            self.next_space() + self.next_space()
                        }
                    }
                }
            }
        };
        self.index += 1;
        Some(result)
    }
}

impl From<f64> for FlexParams {
    fn from(flex: f64) -> Self {
        Self::new(flex, None)
    }
}

impl From<CrossAxisAlignment> for FlexParams {
    fn from(alignment: CrossAxisAlignment) -> Self {
        Self::new(None, alignment)
    }
}

impl Child {
    fn widget_mut(&mut self) -> Option<&mut WidgetPod<dyn Widget>> {
        match self {
            Self::Fixed { widget, .. } | Self::Flex { widget, .. } => Some(widget),
            _ => None,
        }
    }
    fn widget(&self) -> Option<&WidgetPod<dyn Widget>> {
        match self {
            Self::Fixed { widget, .. } | Self::Flex { widget, .. } => Some(widget),
            _ => None,
        }
    }
}

/// The size in logical pixels of the default spacer for an axis.
fn axis_default_spacer(axis: Axis) -> f64 {
    match axis {
        Axis::Vertical => crate::theme::WIDGET_PADDING_VERTICAL,
        Axis::Horizontal => crate::theme::WIDGET_PADDING_HORIZONTAL,
    }
}

fn new_flex_child(params: FlexParams, widget: WidgetPod<dyn Widget>) -> Child {
    if let Some(flex) = params.flex {
        if flex.is_normal() && flex > 0.0 {
            Child::Flex {
                widget,
                alignment: params.alignment,
                flex,
            }
        } else {
            tracing::warn!(
                "Flex value should be > 0.0 (was {flex}). See the docs for masonry::widgets::Flex for more information"
            );
            Child::Fixed {
                widget,
                alignment: params.alignment,
            }
        }
    } else {
        Child::Fixed {
            widget,
            alignment: params.alignment,
        }
    }
}

// --- MARK: IMPL WIDGET---
impl Widget for Flex {
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in self.children.iter_mut().filter_map(|x| x.widget_mut()) {
            ctx.register_child(child);
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        Background::prop_changed(ctx, property_type);
        BorderColor::prop_changed(ctx, property_type);
        BorderWidth::prop_changed(ctx, property_type);
        CornerRadius::prop_changed(ctx, property_type);
        Padding::prop_changed(ctx, property_type);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let border = props.get::<BorderWidth>();
        let padding = props.get::<Padding>();

        let bc = *bc;
        let bc = border.layout_down(bc);
        let bc = padding.layout_down(bc);

        // Indicates that the box constrains for the following children have changed.
        // Therefore they have to calculate layout again.
        let bc_changed = self.old_bc != bc;
        self.old_bc = bc;

        // we loosen our constraints when passing to children.
        let loosened_bc = bc.loosen();

        // minor-axis values for all children
        let mut minor = self.direction.minor(bc.min());
        // these two are calculated but only used if we're baseline aligned
        let mut max_above_baseline = 0_f64;
        let mut max_below_baseline = 0_f64;
        let mut any_use_baseline = false;

        let mut any_changed = bc_changed || ctx.needs_layout();

        let gap = self.gap.unwrap_or(axis_default_spacer(self.direction));
        // The gaps are only between the items, so 2 children means 1 gap.
        let total_gap = self.children.len().saturating_sub(1) as f64 * gap;
        // Measure non-flex children.
        let mut major_non_flex = total_gap;
        // We start with a small value to avoid divide-by-zero errors.
        const MIN_FLEX_SUM: f64 = 0.0001;
        let mut flex_sum = MIN_FLEX_SUM;
        for child in &mut self.children {
            match child {
                Child::Fixed { widget, alignment } => {
                    // The BoxConstraints of fixed-children only depends on the BoxConstraints of the
                    // Flex widget.
                    let child_size = if bc_changed || ctx.child_needs_layout(widget) {
                        let alignment = alignment.unwrap_or(self.cross_alignment);
                        any_use_baseline |= alignment == CrossAxisAlignment::Baseline;

                        let old_size = ctx.old_size();
                        let child_size = ctx.run_layout(widget, &loosened_bc);

                        if child_size.width.is_infinite() {
                            tracing::warn!("A non-Flex child has an infinite width.");
                        }

                        if child_size.height.is_infinite() {
                            tracing::warn!("A non-Flex child has an infinite height.");
                        }

                        if old_size != child_size {
                            any_changed = true;
                        }

                        child_size
                    } else {
                        ctx.skip_layout(widget);
                        ctx.child_layout_rect(widget).size()
                    };

                    let baseline_offset = ctx.child_baseline_offset(widget);

                    major_non_flex += self.direction.major(child_size).expand();
                    minor = minor.max(self.direction.minor(child_size).expand());
                    max_above_baseline =
                        max_above_baseline.max(child_size.height - baseline_offset);
                    max_below_baseline = max_below_baseline.max(baseline_offset);
                }
                Child::FixedSpacer(kv, calculated_size) => {
                    *calculated_size = *kv;
                    if *calculated_size < 0.0 {
                        tracing::warn!("Length provided to fixed spacer was less than 0");
                    }
                    *calculated_size = calculated_size.max(0.0);
                    major_non_flex += *calculated_size;
                }
                Child::Flex { flex, .. } | Child::FlexedSpacer(flex, _) => flex_sum += *flex,
            }
        }

        let total_major = self.direction.major(bc.max());
        let remaining = (total_major - major_non_flex).max(0.0);
        let mut remainder: f64 = 0.0;

        let mut major_flex: f64 = 0.0;
        let px_per_flex = remaining / flex_sum;
        // Measure flex children.
        for child in &mut self.children {
            match child {
                Child::Flex {
                    widget,
                    flex,
                    alignment,
                } => {
                    // The BoxConstraints of flex-children depends on the size of every sibling, which
                    // received layout earlier. Therefore we use any_changed.
                    let child_size = if any_changed || ctx.child_needs_layout(widget) {
                        let alignment = alignment.unwrap_or(self.cross_alignment);
                        any_use_baseline |= alignment == CrossAxisAlignment::Baseline;

                        let desired_major = (*flex) * px_per_flex + remainder;
                        let actual_major = desired_major.round();
                        remainder = desired_major - actual_major;

                        let old_size = ctx.old_size();
                        let child_bc = self.direction.constraints(&loosened_bc, 0.0, actual_major);
                        let child_size = ctx.run_layout(widget, &child_bc);

                        if old_size != child_size {
                            any_changed = true;
                        }

                        child_size
                    } else {
                        ctx.skip_layout(widget);
                        ctx.child_layout_rect(widget).size()
                    };

                    let baseline_offset = ctx.child_baseline_offset(widget);

                    major_flex += self.direction.major(child_size).expand();
                    minor = minor.max(self.direction.minor(child_size).expand());
                    max_above_baseline =
                        max_above_baseline.max(child_size.height - baseline_offset);
                    max_below_baseline = max_below_baseline.max(baseline_offset);
                }
                Child::FlexedSpacer(flex, calculated_size) => {
                    let desired_major = (*flex) * px_per_flex + remainder;
                    *calculated_size = desired_major.round();
                    remainder = desired_major - *calculated_size;
                    major_flex += *calculated_size;
                }
                _ => {}
            }
        }

        // figure out if we have extra space on major axis, and if so how to use it
        let extra = if self.fill_major_axis {
            (remaining - major_flex).max(0.0)
        } else {
            // if we are *not* expected to fill our available space this usually
            // means we don't have any extra, unless dictated by our constraints.
            (self.direction.major(bc.min()) - (major_non_flex + major_flex)).max(0.0)
        };

        let mut spacing = Spacing::new(self.main_alignment, extra, self.children.len());

        // the actual size needed to tightly fit the children on the minor axis.
        // Unlike the 'minor' var, this ignores the incoming constraints.
        let minor_dim = match self.direction {
            Axis::Horizontal if any_use_baseline => max_below_baseline + max_above_baseline,
            _ => minor,
        };

        let extra_height = minor - minor_dim.min(minor);

        let mut major = spacing.next().unwrap_or(0.);

        for child in &mut self.children {
            match child {
                Child::Fixed { widget, alignment }
                | Child::Flex {
                    widget, alignment, ..
                } => {
                    let child_size = ctx.child_size(widget);
                    let alignment = alignment.unwrap_or(self.cross_alignment);
                    let child_minor_offset = match alignment {
                        // This will ignore baseline alignment if it is overridden on children,
                        // but is not the default for the container. Is this okay?
                        CrossAxisAlignment::Baseline
                            if matches!(self.direction, Axis::Horizontal) =>
                        {
                            let child_baseline = ctx.child_baseline_offset(widget);
                            let child_above_baseline = child_size.height - child_baseline;
                            extra_height + (max_above_baseline - child_above_baseline)
                        }
                        CrossAxisAlignment::Fill => {
                            let fill_size: Size = self
                                .direction
                                .pack(self.direction.major(child_size), minor_dim)
                                .into();
                            if ctx.old_size() != fill_size {
                                let child_bc = BoxConstraints::tight(fill_size);
                                //TODO: this is the second call of layout on the same child, which
                                // is bad, because it can lead to exponential increase in layout calls
                                // when used multiple times in the widget hierarchy.
                                ctx.run_layout(widget, &child_bc);
                            }
                            0.0
                        }
                        _ => {
                            let extra_minor = minor_dim - self.direction.minor(child_size);
                            alignment.align(extra_minor)
                        }
                    };

                    let child_pos: Point = self.direction.pack(major, child_minor_offset).into();
                    let child_pos = border.place_down(child_pos);
                    let child_pos = padding.place_down(child_pos);
                    ctx.place_child(widget, child_pos);
                    major += self.direction.major(child_size).expand();
                    major += spacing.next().unwrap_or(0.);
                    major += gap;
                }
                Child::FlexedSpacer(_, calculated_size)
                | Child::FixedSpacer(_, calculated_size) => {
                    major += *calculated_size;
                    major += gap;
                }
            }
        }

        if flex_sum > 0.0 && total_major.is_infinite() {
            tracing::warn!("A child of Flex is flex, but Flex is unbounded.");
        }

        if !self.children.is_empty() {
            // If we have at least one child, the last child added `gap` to `major`, which means that `major` is
            // not the total size of the flex in the major axis, it's instead where the "next widget" will be placed.
            // However, for the rest of this value, we need the total size of the widget in the major axis.
            major -= gap;
        }

        if flex_sum > MIN_FLEX_SUM {
            major = total_major;
        }

        // my_size may be larger than the given constraints.
        // In which case, the Flex widget will either overflow its parent
        // or be clipped (e.g. if its parent is a Portal).
        let my_size: Size = self.direction.pack(major, minor_dim).into();

        let baseline = match self.direction {
            Axis::Horizontal => max_below_baseline,
            Axis::Vertical => self
                .children
                .last()
                .map(|last| {
                    let child = last.widget();
                    if let Some(widget) = child {
                        let child_bl = ctx.child_baseline_offset(widget);
                        let child_max_y = ctx.child_layout_rect(widget).max_y();
                        let extra_bottom_padding = my_size.height - child_max_y;
                        child_bl + extra_bottom_padding
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
        };

        let (my_size, baseline) = padding.layout_up(my_size, baseline);
        let (my_size, baseline) = border.layout_up(my_size, baseline);
        ctx.set_baseline_offset(baseline);
        my_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, props: &PropertiesRef<'_>, scene: &mut Scene) {
        let border_width = props.get::<BorderWidth>();
        let border_radius = props.get::<CornerRadius>();
        let bg = props.get::<Background>();
        let border_color = props.get::<BorderColor>();

        let bg_rect = border_width.bg_rect(ctx.size(), border_radius);
        let border_rect = border_width.border_rect(ctx.size(), border_radius);

        let brush = bg.get_peniko_brush_for_rect(bg_rect.rect());
        fill(scene, &bg_rect, &brush);
        stroke(scene, &border_rect, border_color.color, border_width.width);

        // paint the baseline if we're debugging layout
        if ctx.debug_paint_enabled() && ctx.baseline_offset() != 0.0 {
            let color = ctx.debug_color();
            let my_baseline = ctx.size().height - ctx.baseline_offset();
            let line = Line::new((0.0, my_baseline), (ctx.size().width, my_baseline));

            let stroke_style = Stroke::new(1.0).with_dashes(0., [4.0, 4.0]);
            scene.stroke(&stroke_style, Affine::IDENTITY, color, None, &line);
        }
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
        self.children
            .iter()
            .filter_map(|child| child.widget())
            .map(|widget_pod| widget_pod.id())
            .collect()
    }

    fn make_trace_span(&self, ctx: &QueryCtx<'_>) -> Span {
        trace_span!("Flex", id = ctx.widget_id().trace())
    }
}

// --- MARK: TESTS
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_render_snapshot;
    use crate::testing::TestHarness;
    use crate::theme::default_property_set;
    use crate::widgets::Label;

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn test_main_axis_alignment_spacing() {
        // The following alignment strategy is based on how
        // Chrome 80 handles it with CSS flex.

        let vec = |a, e, n| -> Vec<f64> { Spacing::new(a, e, n).collect() };

        let a = MainAxisAlignment::Start;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![0., 10.]);
        assert_eq!(vec(a, 10., 2), vec![0., 0., 10.]);
        assert_eq!(vec(a, 10., 3), vec![0., 0., 0., 10.]);

        let a = MainAxisAlignment::End;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![10., 0.]);
        assert_eq!(vec(a, 10., 2), vec![10., 0., 0.]);
        assert_eq!(vec(a, 10., 3), vec![10., 0., 0., 0.]);

        let a = MainAxisAlignment::Center;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![5., 5.]);
        assert_eq!(vec(a, 10., 2), vec![5., 0., 5.]);
        assert_eq!(vec(a, 10., 3), vec![5., 0., 0., 5.]);
        assert_eq!(vec(a, 1., 0), vec![1.]);
        assert_eq!(vec(a, 3., 1), vec![2., 1.]);
        assert_eq!(vec(a, 5., 2), vec![3., 0., 2.]);
        assert_eq!(vec(a, 17., 3), vec![9., 0., 0., 8.]);

        let a = MainAxisAlignment::SpaceBetween;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![0., 10.]);
        assert_eq!(vec(a, 10., 2), vec![0., 10., 0.]);
        assert_eq!(vec(a, 10., 3), vec![0., 5., 5., 0.]);
        assert_eq!(vec(a, 33., 5), vec![0., 8., 9., 8., 8., 0.]);
        assert_eq!(vec(a, 34., 5), vec![0., 9., 8., 9., 8., 0.]);
        assert_eq!(vec(a, 35., 5), vec![0., 9., 9., 8., 9., 0.]);
        assert_eq!(vec(a, 36., 5), vec![0., 9., 9., 9., 9., 0.]);
        assert_eq!(vec(a, 37., 5), vec![0., 9., 10., 9., 9., 0.]);
        assert_eq!(vec(a, 38., 5), vec![0., 10., 9., 10., 9., 0.]);
        assert_eq!(vec(a, 39., 5), vec![0., 10., 10., 9., 10., 0.]);

        let a = MainAxisAlignment::SpaceEvenly;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![5., 5.]);
        assert_eq!(vec(a, 10., 2), vec![3., 4., 3.]);
        assert_eq!(vec(a, 10., 3), vec![3., 2., 3., 2.]);
        assert_eq!(vec(a, 33., 5), vec![6., 5., 6., 5., 6., 5.]);
        assert_eq!(vec(a, 34., 5), vec![6., 5., 6., 6., 5., 6.]);
        assert_eq!(vec(a, 35., 5), vec![6., 6., 5., 6., 6., 6.]);
        assert_eq!(vec(a, 36., 5), vec![6., 6., 6., 6., 6., 6.]);
        assert_eq!(vec(a, 37., 5), vec![6., 6., 7., 6., 6., 6.]);
        assert_eq!(vec(a, 38., 5), vec![6., 7., 6., 6., 7., 6.]);
        assert_eq!(vec(a, 39., 5), vec![7., 6., 7., 6., 7., 6.]);

        let a = MainAxisAlignment::SpaceAround;
        assert_eq!(vec(a, 10., 0), vec![10.]);
        assert_eq!(vec(a, 10., 1), vec![5., 5.]);
        assert_eq!(vec(a, 10., 2), vec![3., 5., 2.]);
        assert_eq!(vec(a, 10., 3), vec![2., 3., 3., 2.]);
        assert_eq!(vec(a, 33., 5), vec![3., 7., 6., 7., 7., 3.]);
        assert_eq!(vec(a, 34., 5), vec![3., 7., 7., 7., 7., 3.]);
        assert_eq!(vec(a, 35., 5), vec![4., 7., 7., 7., 7., 3.]);
        assert_eq!(vec(a, 36., 5), vec![4., 7., 7., 7., 7., 4.]);
        assert_eq!(vec(a, 37., 5), vec![4., 7., 8., 7., 7., 4.]);
        assert_eq!(vec(a, 38., 5), vec![4., 7., 8., 8., 7., 4.]);
        assert_eq!(vec(a, 39., 5), vec![4., 8., 7., 8., 8., 4.]);
    }

    // TODO - fix this test
    #[test]
    #[ignore = "Unclear what test is trying to validate"]
    fn test_invalid_flex_params() {
        use float_cmp::approx_eq;
        let params = FlexParams::new(0.0, None);
        approx_eq!(f64, params.flex.unwrap(), 1.0, ulps = 2);

        let params = FlexParams::new(-0.0, None);
        approx_eq!(f64, params.flex.unwrap(), 1.0, ulps = 2);

        let params = FlexParams::new(-1.0, None);
        approx_eq!(f64, params.flex.unwrap(), 1.0, ulps = 2);
    }

    // TODO - Reduce copy-pasting?
    #[test]
    fn flex_row_cross_axis_snapshots() {
        let widget = Flex::row()
            .with_child(Label::new("hello"))
            .with_flex_child(Label::new("world"), 1.0)
            .with_child(Label::new("foo"))
            .with_flex_child(
                Label::new("bar"),
                FlexParams::new(2.0, CrossAxisAlignment::Start),
            );

        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Start);
        });
        assert_render_snapshot!(harness, "flex_row_cross_axis_start");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Center);
        });
        assert_render_snapshot!(harness, "flex_row_cross_axis_center");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::End);
        });
        assert_render_snapshot!(harness, "flex_row_cross_axis_end");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Baseline);
        });
        assert_render_snapshot!(harness, "flex_row_cross_axis_baseline");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Fill);
        });
        assert_render_snapshot!(harness, "flex_row_cross_axis_fill");
    }

    #[test]
    fn flex_row_main_axis_snapshots() {
        let widget = Flex::row()
            .with_child(Label::new("hello"))
            .with_flex_child(Label::new("world"), 1.0)
            .with_child(Label::new("foo"))
            .with_flex_child(
                Label::new("bar"),
                FlexParams::new(2.0, CrossAxisAlignment::Start),
            );

        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        // MAIN AXIS ALIGNMENT

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::Start);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_start");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::Center);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_center");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::End);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_end");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceBetween);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_spaceBetween");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceEvenly);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_spaceEvenly");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceAround);
        });
        assert_render_snapshot!(harness, "flex_row_main_axis_spaceAround");

        // FILL MAIN AXIS
        // TODO - This doesn't seem to do anything?

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_must_fill_main_axis(&mut flex, true);
        });
        assert_render_snapshot!(harness, "flex_row_fill_main_axis");
    }

    #[test]
    fn flex_col_cross_axis_snapshots() {
        let widget = Flex::column()
            .with_child(Label::new("hello"))
            .with_flex_child(Label::new("world"), 1.0)
            .with_child(Label::new("foo"))
            .with_flex_child(
                Label::new("bar"),
                FlexParams::new(2.0, CrossAxisAlignment::Start),
            );

        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Start);
        });
        assert_render_snapshot!(harness, "flex_col_cross_axis_start");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Center);
        });
        assert_render_snapshot!(harness, "flex_col_cross_axis_center");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::End);
        });
        assert_render_snapshot!(harness, "flex_col_cross_axis_end");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Baseline);
        });
        assert_render_snapshot!(harness, "flex_col_cross_axis_baseline");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_cross_axis_alignment(&mut flex, CrossAxisAlignment::Fill);
        });
        assert_render_snapshot!(harness, "flex_col_cross_axis_fill");
    }

    #[test]
    fn flex_col_main_axis_snapshots() {
        let widget = Flex::column()
            .with_child(Label::new("hello"))
            .with_flex_child(Label::new("world"), 1.0)
            .with_child(Label::new("foo"))
            .with_flex_child(
                Label::new("bar"),
                FlexParams::new(2.0, CrossAxisAlignment::Start),
            );

        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);

        // MAIN AXIS ALIGNMENT

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::Start);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_start");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::Center);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_center");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::End);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_end");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceBetween);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_spaceBetween");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceEvenly);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_spaceEvenly");

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_main_axis_alignment(&mut flex, MainAxisAlignment::SpaceAround);
        });
        assert_render_snapshot!(harness, "flex_col_main_axis_spaceAround");

        // FILL MAIN AXIS
        // TODO - This doesn't seem to do anything?

        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();
            Flex::set_must_fill_main_axis(&mut flex, true);
        });
        assert_render_snapshot!(harness, "flex_col_fill_main_axis");
    }

    #[test]
    fn edit_flex_container() {
        let image_1 = {
            let widget = Flex::column()
                .with_child(Label::new("a"))
                .with_child(Label::new("b"))
                .with_child(Label::new("c"))
                .with_child(Label::new("d"));
            // -> abcd

            let window_size = Size::new(200.0, 150.0);
            let mut harness =
                TestHarness::create_with_size(default_property_set(), widget, window_size);

            harness.edit_root_widget(|mut flex| {
                let mut flex = flex.downcast::<Flex>();

                Flex::remove_child(&mut flex, 1);
                // -> acd
                Flex::add_child(&mut flex, Label::new("x"));
                // -> acdx
                Flex::add_flex_child(&mut flex, Label::new("y"), 2.0);
                // -> acdxy
                Flex::add_default_spacer(&mut flex);
                // -> acdxy_
                Flex::add_spacer(&mut flex, 5.0);
                // -> acdxy__
                Flex::add_flex_spacer(&mut flex, 1.0);
                // -> acdxy___
                Flex::insert_child(&mut flex, 2, Label::new("i"));
                // -> acidxy___
                Flex::insert_flex_child(&mut flex, 2, Label::new("j"), 2.0);
                // -> acjidxy___
                Flex::insert_default_spacer(&mut flex, 2);
                // -> ac_jidxy___
                Flex::insert_spacer(&mut flex, 2, 5.0);
                // -> ac__jidxy___
                Flex::insert_flex_spacer(&mut flex, 2, 1.0);
            });

            harness.render()
        };

        let image_2 = {
            let widget = Flex::column()
                .with_child(Label::new("a"))
                .with_child(Label::new("c"))
                .with_flex_spacer(1.0)
                .with_spacer(5.0)
                .with_default_spacer()
                .with_flex_child(Label::new("j"), 2.0)
                .with_child(Label::new("i"))
                .with_child(Label::new("d"))
                .with_child(Label::new("x"))
                .with_flex_child(Label::new("y"), 2.0)
                .with_default_spacer()
                .with_spacer(5.0)
                .with_flex_spacer(1.0);

            let window_size = Size::new(200.0, 150.0);
            let mut harness =
                TestHarness::create_with_size(default_property_set(), widget, window_size);
            harness.render()
        };

        // We don't use assert_eq because we don't want rich assert
        assert!(image_1 == image_2);
    }

    #[test]
    fn get_flex_child() {
        let widget = Flex::column()
            .with_child(Label::new("hello"))
            .with_child(Label::new("world"))
            .with_spacer(1.0);

        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);
        harness.edit_root_widget(|mut flex| {
            let mut flex = flex.downcast::<Flex>();

            let mut child = Flex::child_mut(&mut flex, 1).unwrap();
            assert_eq!(
                child
                    .try_downcast::<Label>()
                    .unwrap()
                    .widget
                    .text()
                    .to_string(),
                "world"
            );
            std::mem::drop(child);

            assert!(Flex::child_mut(&mut flex, 2).is_none());
        });

        // TODO - test out-of-bounds access?
    }

    #[test]
    fn divide_by_zero() {
        let widget = Flex::column().with_flex_spacer(0.0);

        // Running layout should not panic when the flex sum is zero.
        let window_size = Size::new(200.0, 150.0);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget, window_size);
        harness.render();
    }
}
