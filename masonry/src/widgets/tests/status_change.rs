// Copyright 2021 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use assert_matches::assert_matches;
use vello::kurbo::Vec2;

use crate::core::{PointerButton, PointerEvent, Update, WidgetId};
use crate::testing::{
    PRIMARY_MOUSE, Record, Recording, TestHarness, TestWidgetExt as _, widget_ids,
};
use crate::theme::default_property_set;
use crate::widgets::{Button, Flex, SizedBox};

fn next_pointer_event(recording: &Recording) -> Option<PointerEvent> {
    while let Some(event) = recording.next() {
        match event {
            Record::PE(event) => {
                return Some(event);
            }
            _ => {}
        };
    }
    None
}

fn is_hovered(harness: &TestHarness, id: WidgetId) -> bool {
    harness.get_widget(id).ctx().is_hovered()
}

fn has_hovered(harness: &TestHarness, id: WidgetId) -> bool {
    harness.get_widget(id).ctx().has_hovered()
}

fn next_hovered_changed(recording: &Recording) -> Option<bool> {
    while let Some(event) = recording.next() {
        match event {
            Record::U(Update::HoveredChanged(hovered)) => return Some(hovered),
            _ => {}
        }
    }
    None
}

// TODO - Rewrite test to be more clear
#[test]
fn propagate_hovered() {
    let [button, pad, root, empty] = widget_ids();

    let root_rec = Recording::default();
    let padding_rec = Recording::default();
    let button_rec = Recording::default();

    let widget = Flex::column()
        .with_child_id(SizedBox::empty().width(10.0).height(10.0), empty)
        .with_child_id(
            Flex::column()
                .with_spacer(100.0)
                .with_child_id(Button::new("hovered").record(&button_rec), button)
                .with_spacer(10.0)
                .record(&padding_rec),
            pad,
        )
        .record(&root_rec)
        .with_id(root);

    let mut harness = TestHarness::create(default_property_set(), widget);

    // we don't care about setup events, so discard them now.
    root_rec.clear();
    padding_rec.clear();
    button_rec.clear();

    harness.inspect_widgets(|widget| assert!(!widget.ctx().is_hovered()));

    // What we are doing here is moving the mouse to different widgets,
    // and verifying both the widget's `is_hovered` status and also that
    // each widget received the expected hoveredChanged messages.

    // Move to empty box

    harness.mouse_move_to(empty);

    dbg!(harness.get_widget(button).ctx().bounding_rect());
    dbg!(harness.get_widget(pad).ctx().bounding_rect());
    dbg!(harness.get_widget(root).ctx().bounding_rect());
    dbg!(harness.get_widget(empty).ctx().bounding_rect());

    eprintln!("root: {root:?}");
    eprintln!("empty: {empty:?}");
    eprintln!("pad: {pad:?}");
    eprintln!("button: {button:?}");

    assert!(is_hovered(&harness, empty));

    assert!(!is_hovered(&harness, root));
    assert!(has_hovered(&harness, root));

    assert!(!has_hovered(&harness, pad));

    // TODO - Detect ChildHoveredChanged
    assert_eq!(next_hovered_changed(&root_rec), None);
    assert_eq!(next_hovered_changed(&padding_rec), None);
    assert_eq!(next_hovered_changed(&button_rec), None);
    root_rec.clear();

    // Move to padding spacer of Flex column

    // Because mouse_move_to moves to the center of the widget, and the Flex::column
    // starts with a big spacer, the mouse is moved to the padding area, not the Button
    harness.mouse_move_to(pad);

    assert!(is_hovered(&harness, pad));
    assert!(!is_hovered(&harness, empty));
    assert!(!is_hovered(&harness, button));

    assert_eq!(next_hovered_changed(&root_rec), None);
    assert_eq!(next_hovered_changed(&padding_rec), Some(true));
    assert_eq!(next_hovered_changed(&button_rec), None);
    padding_rec.clear();

    // Move to button

    harness.mouse_move_to(button);

    assert!(has_hovered(&harness, root));
    assert!(has_hovered(&harness, pad));
    assert!(!is_hovered(&harness, empty));
    assert!(is_hovered(&harness, button));

    // TODO - Detect ChildHoveredChanged
    assert_eq!(next_hovered_changed(&padding_rec), Some(false));
    assert_eq!(next_hovered_changed(&button_rec), Some(true));
    root_rec.clear();
    padding_rec.clear();
    button_rec.clear();

    // Move to empty box again

    harness.mouse_move_to(empty);

    assert!(has_hovered(&harness, root));
    assert!(is_hovered(&harness, empty));
    assert!(!has_hovered(&harness, pad));
    assert!(!is_hovered(&harness, button));

    // TODO - Detect ChildHoveredChanged
    assert_eq!(next_hovered_changed(&root_rec), None);
    assert_eq!(next_hovered_changed(&padding_rec), None);
    assert_eq!(next_hovered_changed(&button_rec), Some(false));
}

#[test]
fn update_hovered_on_mouse_leave() {
    let [button_id] = widget_ids();

    let button_rec = Recording::default();

    let widget = Button::new("hello").record(&button_rec).with_id(button_id);

    let mut harness = TestHarness::create(default_property_set(), widget);

    harness.mouse_move_to(button_id);
    assert!(is_hovered(&harness, button_id));

    button_rec.clear();
    println!("leaving");
    harness.process_pointer_event(PointerEvent::Leave(PRIMARY_MOUSE));

    assert!(!is_hovered(&harness, button_id));
    assert_eq!(next_hovered_changed(&button_rec), Some(false));
}

// TODO - https://github.com/linebender/xilem/issues/336
#[cfg(false)]
#[test]
fn update_hovered_from_layout() {
    pub const COLLAPSE: Selector = Selector::new("masonry-test.collapse");
    pub const BOX_SIZE: Size = Size::new(50.0, 50.0);

    let [collapsible_id, box_id] = widget_ids();

    let box_rec = Recording::default();

    let collapsible_box = ModularWidget::new(false)
        .event_fn(move |collapsed, ctx, event| {
            if let Event::Command(command) = event {
                if command.is(COLLAPSE) {
                    *collapsed = true;
                    ctx.request_layout();
                }
            }
        })
        .layout_fn(
            move |collapsed, _ctx, _bc| {
                if *collapsed { Size::ZERO } else { BOX_SIZE }
            },
        );

    let widget = Flex::row()
        .with_child(
            Flex::column()
                .with_child_id(collapsible_box, collapsible_id)
                .with_child_id(
                    SizedBox::empty().height(50.0).width(50.0).record(&box_rec),
                    box_id,
                )
                .with_flex_spacer(1.0),
        )
        .with_flex_spacer(1.0);

    let mut harness = TestHarness::create(widgets);

    harness.mouse_move_to(collapsible_id);
    assert!(is_hovered(&harness, collapsible_id));
    assert!(!is_hovered(&harness, box_id));

    box_rec.clear();
    harness.submit_command(COLLAPSE);
    assert!(!is_hovered(&harness, collapsible_id));
    assert!(is_hovered(&harness, box_id));

    assert_eq!(next_hovered_changed(&box_rec), Some(true));
}

#[test]
fn get_pointer_events_while_active() {
    let [button, root, empty, empty_2] = widget_ids();

    let button_rec = Recording::default();

    let widget = Flex::column()
        .with_child_id(SizedBox::empty().width(10.0).height(10.0), empty)
        .with_child_id(SizedBox::empty().width(10.0).height(10.0), empty_2)
        .with_child_id(Button::new("hello").record(&button_rec), button)
        .with_id(root);

    let mut harness = TestHarness::create(default_property_set(), widget);

    // First we check that the default state is clear: nothing active, no event recorded
    assert_eq!(harness.pointer_capture_target_id(), None);

    assert_matches!(next_pointer_event(&button_rec), None);

    // We press the button

    harness.mouse_move_to(button);
    harness.mouse_button_press(PointerButton::Primary);

    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Move(..))
    );
    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Down { .. })
    );
    assert_matches!(next_pointer_event(&button_rec), None);

    assert_eq!(harness.pointer_capture_target_id(), Some(button));

    // We move the cursor away without releasing the button

    harness.mouse_move_to(empty);

    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Move(..))
    );
    assert_matches!(next_pointer_event(&button_rec), None);

    assert_eq!(harness.pointer_capture_target_id(), Some(button));

    // We simulate the scroll wheel, still without releasing the button

    harness.mouse_wheel(Vec2::ZERO);

    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Scroll { .. })
    );
    assert_matches!(next_pointer_event(&button_rec), None);

    // We release the button

    harness.mouse_button_release(PointerButton::Primary);

    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Up { .. })
    );
    assert_matches!(next_pointer_event(&button_rec), None);

    assert_eq!(harness.pointer_capture_target_id(), None);

    // We move the mouse again to check movements aren't captured anymore
    harness.mouse_move_to(empty_2);
    assert_matches!(next_pointer_event(&button_rec), None);
}

#[test]
fn automatically_lose_pointer_on_pointer_lost() {
    let [button, root, empty] = widget_ids();

    let button_rec = Recording::default();

    let widget = Flex::column()
        .with_child_id(SizedBox::empty().width(10.0).height(10.0), empty)
        .with_child_id(Button::new("hello").record(&button_rec), button)
        .with_id(root);

    let mut harness = TestHarness::create(default_property_set(), widget);

    // The default state is that nothing has captured the pointer.
    assert_eq!(harness.pointer_capture_target_id(), None);

    // We press the button
    harness.mouse_move_to(button);
    harness.mouse_button_press(PointerButton::Primary);

    // The button should be notified of the move and pointer down events
    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Move(..))
    );
    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Down { .. })
    );

    // and should now hold the capture.
    assert_eq!(harness.pointer_capture_target_id(), Some(button));

    // The pointer moves to empty space. The button is notified and still holds the capture.
    harness.mouse_move_to(empty);
    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Move(..))
    );
    assert_eq!(harness.pointer_capture_target_id(), Some(button));

    // The pointer is lost, without releasing the primary button first
    harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));

    // The button holds the capture during this event and should be notified the pointer is lost
    assert_matches!(
        next_pointer_event(&button_rec),
        Some(PointerEvent::Cancel(..))
    );

    // The button should have lost the pointer capture
    assert_eq!(harness.pointer_capture_target_id(), None);

    // If the pointer enters and leaves again, the button should not be notified
    harness.process_pointer_event(PointerEvent::Enter(PRIMARY_MOUSE));
    harness.process_pointer_event(PointerEvent::Leave(PRIMARY_MOUSE));
    assert_matches!(next_pointer_event(&button_rec), None);
}
