// Copyright 2023 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! A recreation of todomvc in Xilem Web
// TODO: Link

mod state;

use state::{AppState, Filter, Todo};
use wasm_bindgen::JsCast;
use xilem_web::core::{MessageResult, map_message, map_state};
use xilem_web::elements::html as el;
use xilem_web::interfaces::*;
use xilem_web::modifiers::style as s;
use xilem_web::{Action, App, DomView, get_element_by_id};

// All of these actions arise from within a `Todo`, but we need access to the full state to reduce
// them.
enum TodoAction {
    SetEditing(u64),
    CommitEdit,
    CancelEditing,
    Destroy(u64),
}

impl Action for TodoAction {}

fn todo_item(todo: &mut Todo, editing: bool) -> impl Element<Todo, TodoAction> + use<> {
    let checkbox = el::input(())
        .class("toggle")
        .type_("checkbox")
        .checked(todo.completed)
        .on_click(|state: &mut Todo, _| state.completed = !state.completed);

    el::li((
        el::div((
            checkbox,
            el::label(todo.title.clone())
                .on_dblclick(|state: &mut Todo, _| TodoAction::SetEditing(state.id)),
            el::button(())
                .class("destroy")
                .on_click(|state: &mut Todo, _| TodoAction::Destroy(state.id)),
        ))
        .class("view"),
        el::input(())
            .attr("value", todo.title_editing.clone())
            .class("edit")
            .on_keydown(|state: &mut Todo, evt| {
                let key = evt.key();
                if key == "Enter" {
                    state.save_editing();
                    Some(TodoAction::CommitEdit)
                } else if key == "Escape" {
                    Some(TodoAction::CancelEditing)
                } else {
                    None
                }
            })
            .on_input(|state: &mut Todo, evt| {
                // TODO There could/should be further checks, if this is indeed the right event (same DOM element)
                if let Some(element) = evt
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                {
                    evt.prevent_default();
                    state.title_editing = element.value();
                }
            })
            .passive(true)
            .on_blur(|_, _| TodoAction::CancelEditing),
    ))
    .class(todo.completed.then_some("completed"))
    .class(editing.then_some("editing"))
}

fn footer_view(state: &mut AppState, should_display: bool) -> impl Element<AppState> + use<> {
    let clear_button = (state.todos.iter().filter(|todo| todo.completed).count() > 0).then(|| {
        el::button("Clear completed")
            .class("clear-completed")
            .on_click(|state: &mut AppState, _| {
                state.todos.retain(|todo| !todo.completed);
            })
    });

    let filter_class = |filter| (state.filter == filter).then_some("selected");

    let items_left = state.todos.iter().filter(|todo| !todo.completed).count();
    let item_noun = if items_left == 1 { "item" } else { "items" };

    el::footer((
        el::span(format!("{items_left} {item_noun} left")).class("todo-count"),
        el::ul((
            el::li(Element::on_click(
                el::a("All")
                    .attr("href", "#/")
                    .class(filter_class(Filter::All)),
                |state: &mut AppState, _| {
                    state.filter = Filter::All;
                },
            )),
            " ",
            el::li(Element::on_click(
                el::a("Active")
                    .attr("href", "#/active")
                    .class(filter_class(Filter::Active)),
                |state: &mut AppState, _| {
                    state.filter = Filter::Active;
                },
            )),
            " ",
            el::li(Element::on_click(
                el::a("Completed")
                    .attr("href", "#/completed")
                    .class(filter_class(Filter::Completed)),
                |state: &mut AppState, _| {
                    state.filter = Filter::Completed;
                },
            )),
        ))
        .class("filters"),
        clear_button,
    ))
    .class("footer")
    .style((!should_display).then_some(s("display", "none")))
}

fn main_view(state: &mut AppState, should_display: bool) -> impl Element<AppState> + use<> {
    let editing_id = state.editing_id;
    let todos: Vec<_> = state
        .visible_todos()
        .map(|(idx, todo)| {
            map_message(
                map_state(
                    todo_item(todo, editing_id == Some(todo.id)),
                    move |data: &mut AppState| &mut data.todos[idx],
                ),
                move |data: &mut AppState, result| {
                    if let MessageResult::Action(action) = result {
                        match action {
                            TodoAction::SetEditing(id) => data.start_editing(id),
                            TodoAction::CommitEdit => {
                                data.save();
                                data.editing_id = None;
                            }
                            TodoAction::CancelEditing => data.editing_id = None,
                            TodoAction::Destroy(id) => data.todos.retain(|todo| todo.id != id),
                        }
                    }
                    MessageResult::Nop
                },
            )
        })
        .collect();
    let toggle_all = el::input(())
        .attr("id", "toggle-all")
        .class("toggle-all")
        .type_("checkbox")
        .checked(state.are_all_complete());

    el::section((
        toggle_all.on_click(|state: &mut AppState, _| state.toggle_all_complete()),
        el::label(()).attr("for", "toggle-all"),
        el::ul(todos).class("todo-list"),
    ))
    .class("main")
    .style((!should_display).then_some(s("display", "none")))
}

fn app_logic(state: &mut AppState) -> impl DomView<AppState> + use<> {
    tracing::debug!("render: {state:?}");
    let some_todos = !state.todos.is_empty();
    let main = main_view(state, some_todos);
    let footer = footer_view(state, some_todos);
    let input = el::input(())
        .class("new-todo")
        .attr("placeholder", "What needs to be done?")
        .attr("value", state.new_todo.clone())
        .attr("autofocus", true);
    el::div((
        el::header((
            el::h1("TODOs"),
            input
                .on_keydown(|state: &mut AppState, evt| {
                    if evt.key() == "Enter" {
                        state.create_todo();
                    }
                })
                .on_input(|state: &mut AppState, evt| {
                    // TODO There could/should be further checks, if this is indeed the right event (same DOM element)
                    if let Some(element) = evt
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                    {
                        state.update_new_todo(&element.value());
                        evt.prevent_default();
                    }
                })
                .passive(false),
        ))
        .class("header"),
        main,
        footer,
    ))
}

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();
    App::new(get_element_by_id("todoapp"), AppState::load(), app_logic).run();
}
