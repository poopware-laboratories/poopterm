//! Tests of pointing the app menu's About item at our own page, on real
//! `NSMenu`s built here: the item found wherever it sits, other items left
//! alone, and the item's action reaching the callback the app installed.
use super::{OURS, STANDARD, WAKE, redirect, set_wake, show_about, target};
use crate::objc::{NIL, Object, Raw, Sel, class, sel, string};
use std::sync::{
	Arc, Mutex, MutexGuard,
	atomic::{AtomicUsize, Ordering},
};

/// Tests that install a callback take turns: there is one per process.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
	SERIAL
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A callback counting its calls into `counter`.
fn counting(counter: &Arc<AtomicUsize>) -> super::Wake {
	let counter = counter.clone();
	Box::new(move || {
		counter.fetch_add(1, Ordering::Relaxed);
	})
}

/// Sends our action straight to the target, as AppKit does for the item.
fn choose() {
	show_about(target().unwrap(), sel(OURS), NIL);
}

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
	let _serial = serial();
	// Before the app installs one, choosing the item does nothing.
	set_wake(None);
	choose();
	let chosen = Arc::new(AtomicUsize::new(0));
	set_wake(Some(counting(&chosen)));
	choose();
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 2);
	set_wake(None);
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 2);
}

#[test]
fn installing_a_callback_replaces_the_one_before() {
	let _serial = serial();
	let (first, second) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
	set_wake(Some(counting(&first)));
	choose();
	set_wake(Some(counting(&second)));
	choose();
	choose();
	assert_eq!(first.load(Ordering::Relaxed), 1);
	assert_eq!(second.load(Ordering::Relaxed), 2);
	set_wake(None);
}

#[test]
fn a_callback_that_panicked_elsewhere_does_not_stop_the_item() {
	let _serial = serial();
	// A thread that panics holding the lock poisons it.
	let poisoner = std::thread::spawn(|| {
		let _held = WAKE.lock();
		panic!("poison the callback's lock");
	});
	assert!(poisoner.join().is_err());
	let chosen = Arc::new(AtomicUsize::new(0));
	set_wake(Some(counting(&chosen)));
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 1);
	set_wake(None);
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 1);
}

#[test]
fn the_callback_runs_on_whichever_thread_chooses_the_item() {
	let _serial = serial();
	let chosen = Arc::new(AtomicUsize::new(0));
	set_wake(Some(counting(&chosen)));
	let threads: Vec<_> = (0..4).map(|_| std::thread::spawn(choose)).collect();
	for thread in threads {
		thread.join().unwrap();
	}
	assert_eq!(chosen.load(Ordering::Relaxed), 4);
	set_wake(None);
}

#[test]
fn an_item_titled_about_that_sends_something_else_is_left_alone() {
	// Items are matched by what they do, not by their title, which is
	// localized and names the process.
	let bar = menu();
	let help = item(&bar, "About poopterm", Some(sel(c"showHelp:")));
	// SAFETY: an `NSMenu`.
	assert!(!unsafe { redirect(&bar) });
	assert_eq!(action(&help), Some(sel(c"showHelp:")));
	assert_eq!(receiver(&help), NIL);
}

#[test]
fn an_about_item_with_a_target_of_its_own_is_taken_over() {
	let bar = menu();
	let about = item(&bar, "About", Some(sel(STANDARD)));
	// SAFETY: `NSObject` takes `new`.
	let other = unsafe { class(c"NSObject").unwrap().instance() }.unwrap();
	// SAFETY: `setTarget:` takes an object, which the item does not retain;
	// `other` outlives the item's use here.
	unsafe { about.send::<()>(c"setTarget:", (other.raw(),)) };
	assert_eq!(receiver(&about), other.raw());
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(receiver(&about), target().unwrap());
	assert_eq!(action(&about), Some(sel(OURS)));
}

#[test]
fn the_item_keeps_its_title_shortcut_and_tag() {
	let (bar, about) = menu_bar();
	// SAFETY: `setTag:` takes an `NSInteger`.
	unsafe { about.send::<()>(c"setTag:", (7isize,)) };
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	// SAFETY: `title` returns an `NSString`, `tag` an `NSInteger`, and
	// `isEnabled` a `BOOL`.
	unsafe {
		let title = Object::retained(about.send(c"title", ())).unwrap();
		assert_eq!(crate::objc::text(&title), "About poopterm");
		assert_eq!(about.send::<isize>(c"tag", ()), 7);
		assert_ne!(about.send::<crate::objc::Bool>(c"isEnabled", ()), 0);
	}
}

#[test]
fn items_after_a_submenu_are_still_searched() {
	let bar = menu();
	let nested = menu();
	nest(&item(&bar, "Edit", None), &nested);
	item(&nested, "Copy", Some(sel(c"copy:")));
	let about = item(&bar, "About", Some(sel(STANDARD)));
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&about), Some(sel(OURS)));
}

#[test]
fn the_last_item_of_a_long_menu_is_searched() {
	let bar = menu();
	let others: Vec<_> = (0..40)
		.map(|index| item(&bar, &format!("Item {index}"), Some(sel(c"copy:"))))
		.collect();
	let about = item(&bar, "About", Some(sel(STANDARD)));
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	assert_eq!(action(&about), Some(sel(OURS)));
	assert!(
		others
			.iter()
			.all(|other| action(other) == Some(sel(c"copy:")))
	);
}

#[test]
fn the_target_is_an_instance_of_our_own_registered_class() {
	let target = target().unwrap();
	// SAFETY: the target lives for the whole process.
	let target = unsafe { Object::retained(target) }.unwrap();
	let ours = class(c"PooptermAboutTarget").unwrap();
	// SAFETY: `isMemberOfClass:` takes a class and returns a `BOOL`.
	let member: crate::objc::Bool = unsafe { target.send(c"isMemberOfClass:", (ours.raw(),)) };
	assert_ne!(member, 0);
}

#[test]
fn the_target_outlives_the_menus_pointed_at_it() {
	let _serial = serial();
	let (bar, about) = menu_bar();
	// SAFETY: an `NSMenu`.
	assert!(unsafe { redirect(&bar) });
	let target = receiver(&about);
	drop((bar, about));
	// Items do not retain their targets, so ours must not go with them.
	assert_eq!(super::target(), Some(target));
	// SAFETY: the target lives for the whole process.
	let target = unsafe { Object::retained(target) }.unwrap();
	assert!(target.responds(OURS));
	let chosen = Arc::new(AtomicUsize::new(0));
	set_wake(Some(counting(&chosen)));
	choose();
	assert_eq!(chosen.load(Ordering::Relaxed), 1);
	set_wake(None);
}
