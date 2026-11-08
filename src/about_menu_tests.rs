//! Tests of pointing the app menu's About item at our own page, on real
//! `NSMenu`s built here: the item found wherever it sits, other items left
//! alone, and the item's action reaching the callback the app installed.
use super::{OURS, STANDARD, redirect, set_wake, target};
use crate::objc::{NIL, Object, Raw, Sel, class, sel, string};
use std::sync::{
	Arc,
	atomic::{AtomicUsize, Ordering},
};

/// A new, empty `NSMenu`, owned.
fn menu() -> Object {
	// SAFETY: `NSMenu` descends from `NSObject` and `init` takes nothing.
	unsafe { class(c"NSMenu").unwrap().instance() }.unwrap()
}

/// A new `NSMenuItem` titled `title` that sends `action`, added to `menu`.
fn item(menu: &Object, title: &str, action: Option<Sel>) -> Object {
	let title = string(title).unwrap();
	let empty = string("").unwrap();
	let item = class(c"NSMenuItem").unwrap();
	let action = action.unwrap_or(Sel::NULL);
	// SAFETY: `alloc` returns an item the caller owns, which
	// `initWithTitle:action:keyEquivalent:` takes two strings and a
	// selector or null to initialize; `addItem:` takes an item.
	unsafe {
		let blank: Raw = item.send(c"alloc", ());
		let made: Raw = crate::objc::Arguments::send(
			(title.raw(), action, empty.raw()),
			blank,
			sel(c"initWithTitle:action:keyEquivalent:"),
		);
		let item = Object::owned(made).unwrap();
		let () = menu.send(c"addItem:", (item.raw(),));
		item
	}
}

/// Makes `submenu` the submenu of `item`.
fn nest(item: &Object, submenu: &Object) {
	// SAFETY: `setSubmenu:` takes an `NSMenu`.
	unsafe { item.send::<()>(c"setSubmenu:", (submenu.raw(),)) }
}

/// The selector `item` sends, if any.
fn action(item: &Object) -> Option<Sel> {
	// SAFETY: `action` takes nothing and returns a selector or null.
	let action: Sel = unsafe { item.send(c"action", ()) };
	(action != Sel::NULL).then_some(action)
}

/// The object `item` sends its action to; nil for the first responder.
fn receiver(item: &Object) -> Raw {
	// SAFETY: `target` takes nothing and returns an object or nil.
	unsafe { item.send(c"target", ()) }
}

/// A menu bar like winit's: an app menu holding About, a separator, Hide
/// and Quit.
fn menu_bar() -> (Object, Object) {
	let bar = menu();
	let app = item(&bar, "", None);
	let app_menu = menu();
	nest(&app, &app_menu);
	let about = item(&app_menu, "About poopterm", Some(sel(STANDARD)));
	// SAFETY: `separatorItem` takes nothing and returns an autoreleased item.
	let separator: Raw = unsafe { class(c"NSMenuItem").unwrap().send(c"separatorItem", ()) };
	// SAFETY: `addItem:` takes an item.
	unsafe { app_menu.send::<()>(c"addItem:", (separator,)) };
	item(&app_menu, "Hide poopterm", Some(sel(c"hide:")));
	item(&app_menu, "Quit poopterm", Some(sel(c"terminate:")));
	(bar, about)
}

#[test]
fn the_about_item_of_the_app_menu_sends_our_action_to_our_target() {
	let (bar, about) = menu_bar();
	assert_eq!(receiver(&about), NIL);
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&about), Some(sel(OURS)));
	assert_eq!(receiver(&about), target().unwrap());
}

#[test]
fn other_items_keep_their_actions_and_the_first_responder() {
	let bar = menu();
	let hide = item(&bar, "Hide poopterm", Some(sel(c"hide:")));
	let quit = item(&bar, "Quit poopterm", Some(sel(c"terminate:")));
	let title = item(&bar, "poopterm", None);
	// SAFETY: an `NSMenu`.
	assert!(!unsafe { redirect(&bar) });
	assert_eq!(action(&hide), Some(sel(c"hide:")));
	assert_eq!(action(&quit), Some(sel(c"terminate:")));
	assert_eq!(action(&title), None);
	for item in [&hide, &quit, &title] {
		assert_eq!(receiver(item), NIL);
	}
}

#[test]
fn an_empty_menu_has_no_about_item() {
	// SAFETY: an `NSMenu`.
	assert!(!unsafe { redirect(&menu()) });
}

#[test]
fn the_about_item_is_found_however_deep_it_is_nested() {
	let bar = menu();
	let (outer, inner) = (menu(), menu());
	nest(&item(&bar, "Outer", None), &outer);
	nest(&item(&outer, "Inner", None), &inner);
	item(&inner, "Help", Some(sel(c"showHelp:")));
	let about = item(&inner, "About", Some(sel(STANDARD)));
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&about), Some(sel(OURS)));
}

#[test]
fn redirecting_again_keeps_the_item_ours() {
	let (bar, about) = menu_bar();
	// SAFETY: an `NSMenu`, twice.
	assert!(unsafe { redirect(&bar) });
	// SAFETY: as above.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&about), Some(sel(OURS)));
	assert_eq!(receiver(&about), target().unwrap());
}

#[test]
fn every_about_item_is_pointed_at_us() {
	// Two menus may both offer About, as the app and Help menus can.
	let bar = menu();
	let first = item(&bar, "About", Some(sel(STANDARD)));
	let second = item(&bar, "About poopterm", Some(sel(STANDARD)));
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&first), Some(sel(OURS)));
	assert_eq!(action(&second), Some(sel(OURS)));
}

#[test]
fn the_target_is_one_object_that_answers_only_our_action() {
	let target = target().unwrap();
	assert_eq!(super::target(), Some(target));
	// SAFETY: the target lives for the whole process.
	let target = unsafe { Object::retained(target) }.unwrap();
	assert!(target.responds(OURS));
	// It does not show AppKit's panel itself.
	assert!(!target.responds(STANDARD));
}

#[test]
fn choosing_the_item_runs_the_installed_callback_each_time() {
	let (bar, about) = menu_bar();
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	// SAFETY: the target lives for the whole process.
	let target = unsafe { Object::retained(receiver(&about)) }.unwrap();
	// SAFETY: our action takes the sender, an object, and returns nothing,
	// as `performSelector:withObject:` sends it, returning nil.
	let choose = || unsafe {
		let _: Raw = target.send(
			c"performSelector:withObject:",
			(action(&about).unwrap(), about.raw()),
		);
	};
	// Before the app installs one, choosing the item does nothing.
	set_wake(None);
	choose();
	let chosen = Arc::new(AtomicUsize::new(0));
	let count = chosen.clone();
	set_wake(Some(Box::new(move || {
		count.fetch_add(1, Ordering::Relaxed);
	})));
	choose();
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 2);
	set_wake(None);
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 2);
}
