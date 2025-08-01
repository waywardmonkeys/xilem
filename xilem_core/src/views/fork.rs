// Copyright 2024 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    AppendVec, Mut, NoElement, View, ViewId, ViewMarker, ViewPathTracker, ViewSequence,
    sequence::NoElements,
};

/// Create a view which acts as `active_view`, whilst also running `alongside_view`, without inserting it into the tree.
///
/// `alongside_view` must be a `ViewSequence` with an element type of [`NoElement`].
pub fn fork<Active, Alongside>(
    active_view: Active,
    alongside_view: Alongside,
) -> Fork<Active, Alongside> {
    Fork {
        active_view,
        alongside_view,
    }
}

/// The view for [`fork`].
#[derive(Debug)]
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Fork<Active, Alongside> {
    active_view: Active,
    alongside_view: Alongside,
}

impl<Active, Alongside> ViewMarker for Fork<Active, Alongside> {}
impl<State, Action, Context, Active, Alongside, Message> View<State, Action, Context, Message>
    for Fork<Active, Alongside>
where
    Active: View<State, Action, Context, Message>,
    Alongside: ViewSequence<State, Action, Context, NoElement, Message>,
    Context: ViewPathTracker,
{
    type Element = Active::Element;

    type ViewState = (Active::ViewState, Alongside::SeqState);

    fn build(&self, ctx: &mut Context, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (element, active_state) =
            ctx.with_id(ViewId::new(0), |ctx| self.active_view.build(ctx, app_state));
        let alongside_state = ctx.with_id(ViewId::new(1), |ctx| {
            self.alongside_view
                .seq_build(ctx, &mut AppendVec::default(), app_state)
        });
        (element, (active_state, alongside_state))
    }

    fn rebuild(
        &self,
        prev: &Self,
        (active_state, alongside_state): &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            self.active_view
                .rebuild(&prev.active_view, active_state, ctx, element, app_state);
        });
        ctx.with_id(ViewId::new(1), |ctx| {
            self.alongside_view.seq_rebuild(
                &prev.alongside_view,
                alongside_state,
                ctx,
                &mut NoElements,
                app_state,
            );
        });
    }

    fn teardown(
        &self,
        (active_state, alongside_state): &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            self.alongside_view
                .seq_teardown(alongside_state, ctx, &mut NoElements, app_state);
        });
        ctx.with_id(ViewId::new(1), |ctx| {
            self.active_view
                .teardown(active_state, ctx, element, app_state);
        });
    }

    fn message(
        &self,
        (active_state, alongside_state): &mut Self::ViewState,
        id_path: &[ViewId],
        message: Message,
        app_state: &mut State,
    ) -> crate::MessageResult<Action, Message> {
        let (first, id_path) = id_path
            .split_first()
            .expect("Id path has elements for Fork");
        match first.routing_id() {
            0 => self
                .active_view
                .message(active_state, id_path, message, app_state),
            1 => self
                .alongside_view
                .seq_message(alongside_state, id_path, message, app_state),
            _ => unreachable!(),
        }
    }
}
