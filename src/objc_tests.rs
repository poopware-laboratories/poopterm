//! Tests of the Objective-C runtime bindings on the real runtime: lookups
//! that fail, messages of every arity we send, and objects owned exactly
//! once, so nothing leaks and nothing is released twice.
use super::{
	Bool, Missing, NIL, Object, Pool, Raw, Sel, action_class, class, describe, sel, string, text,
};

/// How many references `object` has.
fn references(object: &Object) -> usize {
	// SAFETY: `retainCount` takes nothing and returns an `NSUInteger`.
	unsafe { object.send(c"retainCount", ()) }
}

/// A new `NSObject`, owned.
fn object() -> Object {
	let class = class(c"NSObject").unwrap();
	// SAFETY: `NSObject` takes `new`.
	unsafe { class.instance() }.unwrap()
}

#[test]
fn a_class_the_runtime_lacks_is_missing_by_name() {
	let missing = class(c"PooptermNoSuchClass").unwrap_err();
	assert_eq!(missing.0, c"PooptermNoSuchClass");
	assert_eq!(
		missing.to_string(),
		"no Objective-C class PooptermNoSuchClass"
	);
	assert!(class(c"NSObject").is_ok());
	// Names are exact, case included.
	assert_eq!(class(c"nsobject"), Err(Missing(c"nsobject")));
}

#[test]
fn a_name_is_always_the_same_selector_and_two_names_two() {
	assert_eq!(sel(c"description"), sel(c"description"));
	assert_ne!(sel(c"description"), sel(c"hash"));
	// A colon is part of the name: `count` takes nothing, `count:` one.
	assert_ne!(sel(c"count"), sel(c"count:"));
	assert!(!sel(c"PooptermNoSuchSelector").0.is_null());
	// Registering a name twice gives the selector registered first.
	assert_eq!(
		sel(c"PooptermNoSuchSelector"),
		sel(c"PooptermNoSuchSelector")
	);
}

#[test]
fn an_object_answers_whether_it_has_a_method() {
	let object = object();
	assert!(object.responds(c"description"));
	assert!(object.responds(c"respondsToSelector:"));
	// Sending it would raise an exception, which must never cross into Rust.
	assert!(!object.responds(c"pooptermNoSuchMethod"));
	assert!(!object.responds(c"description:"));
}

#[test]
fn a_new_object_is_owned_once_and_each_clone_owns_another_reference() {
	let object = object();
	assert_eq!(references(&object), 1);
	let clone = object.clone();
	assert_eq!(clone, object);
	assert_eq!(references(&object), 2);
	drop(clone);
	assert_eq!(references(&object), 1);
}

#[test]
fn an_object_retained_from_a_borrowed_pointer_owns_its_own_reference() {
	let object = object();
	// SAFETY: a live `NSObject`.
	let retained = unsafe { Object::retained(object.raw()) }.unwrap();
	assert_eq!(references(&object), 2);
	drop(retained);
	assert_eq!(references(&object), 1);
}

#[test]
fn nil_is_no_object_whether_owned_or_retained() {
	// SAFETY: nil is allowed.
	assert!(unsafe { Object::owned(NIL) }.is_none());
	// SAFETY: as above.
	assert!(unsafe { Object::retained(NIL) }.is_none());
}

#[test]
fn an_object_autoreleased_in_a_pool_loses_that_reference_when_it_drops() {
	let object = object();
	{
		let _pool = Pool::new();
		// SAFETY: `retain` and `autorelease` take nothing and return the
		// object; the reference `retain` added is the pool's to release.
		unsafe {
			let _: Raw = object.send(c"retain", ());
			let _: Raw = object.send(c"autorelease", ());
		}
		assert_eq!(references(&object), 2);
		{
			// An inner pool releases only what was autoreleased in it.
			let _inner = Pool::new();
		}
		assert_eq!(references(&object), 2);
	}
	assert_eq!(references(&object), 1);
}

#[test]
fn strings_carry_their_text_both_ways() {
	for words in ["", "poopterm", "ünïcødé 💩 and\0nul"] {
		let made = string(words).unwrap();
		// SAFETY: an `NSString`.
		let back = unsafe { text(&made) };
		// `UTF8String` ends at the first nul.
		assert_eq!(back, words.split('\0').next().unwrap(), "{words:?}");
		// SAFETY: `length` takes nothing and returns an `NSUInteger`.
		let units: usize = unsafe { made.send(c"length", ()) };
		assert_eq!(units, words.encode_utf16().count(), "{words:?}");
	}
}

#[test]
fn messages_carry_arguments_and_return_objects_the_caller_retains() {
	let _pool = Pool::new();
	let (left, right) = (string("poop").unwrap(), string("term").unwrap());
	// SAFETY: `stringByAppendingString:` takes a string and returns an
	// autoreleased one; `isEqualToString:` takes a string and returns a
	// `BOOL`; `characterAtIndex:` an index and returns a `unichar`.
	unsafe {
		let joined: Raw = left.send(c"stringByAppendingString:", (right.raw(),));
		let joined = Object::retained(joined).unwrap();
		assert_eq!(text(&joined), "poopterm");
		let same: Bool = joined.send(c"isEqualToString:", (string("poopterm").unwrap().raw(),));
		assert_ne!(same, 0);
		let other: Bool = joined.send(c"isEqualToString:", (left.raw(),));
		assert_eq!(other, 0);
		let fourth: u16 = joined.send(c"characterAtIndex:", (4usize,));
		assert_eq!(fourth, u16::from(b't'));
	}
}

#[test]
fn an_error_is_described_by_its_own_words_and_nil_by_the_fallback() {
	// SAFETY: nil is allowed.
	assert_eq!(unsafe { describe(NIL, "nothing said") }, "nothing said");
	let _pool = Pool::new();
	let domain = string("PooptermTests").unwrap();
	let class = class(c"NSError").unwrap();
	// SAFETY: `errorWithDomain:code:userInfo:` takes a string, an
	// `NSInteger` and a dictionary or nil, and returns an autoreleased error.
	let error: Raw = unsafe {
		class.send(
			c"errorWithDomain:code:userInfo:",
			(domain.raw(), 42isize, NIL),
		)
	};
	// SAFETY: an `NSError`.
	let said = unsafe { describe(error, "nothing said") };
	assert!(said.contains("PooptermTests"), "{said}");
	assert!(said.contains("42"), "{said}");
}

/// Counts calls of `counted:` on `PooptermCountedTarget`.
static COUNTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "C" fn counted(_receiver: Raw, selector: Sel, sender: Raw) {
	assert_eq!(selector, sel(c"counted:"));
	assert!(!sender.is_null());
	COUNTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[test]
fn an_action_class_answers_its_selector_with_its_action() {
	let made = action_class(c"PooptermCountedTarget", c"counted:", counted).unwrap();
	assert_eq!(class(c"PooptermCountedTarget"), Ok(made));
	// SAFETY: the class descends from `NSObject`.
	let target = unsafe { made.instance() }.unwrap();
	assert!(target.responds(c"counted:"));
	assert!(!target.responds(c"uncounted:"));
	let sender = object();
	// SAFETY: `counted:` takes an object and returns nothing.
	unsafe { target.send::<()>(c"counted:", (sender.raw(),)) };
	assert_eq!(COUNTED.load(std::sync::atomic::Ordering::Relaxed), 1);
}

extern "C" fn ignored(_receiver: Raw, _selector: Sel, _sender: Raw) {}

#[test]
fn making_an_action_class_again_gives_the_class_made_first() {
	let first = action_class(c"PooptermTwiceTarget", c"twice:", ignored).unwrap();
	let second = action_class(c"PooptermTwiceTarget", c"other:", ignored).unwrap();
	assert_eq!(first, second);
	// SAFETY: the class descends from `NSObject`.
	let target = unsafe { second.instance() }.unwrap();
	// The second call added nothing to the registered class.
	assert!(target.responds(c"twice:"));
	assert!(!target.responds(c"other:"));
}

#[test]
fn an_action_class_descends_from_nsobject() {
	let made = action_class(c"PooptermDescendant", c"descend:", ignored).unwrap();
	// SAFETY: the class descends from `NSObject`.
	let target = unsafe { made.instance() }.unwrap();
	assert!(target.responds(c"description"));
	// SAFETY: `isKindOfClass:` takes a class and returns a `BOOL`.
	let kind: Bool = unsafe { target.send(c"isKindOfClass:", (class(c"NSObject").unwrap().raw(),)) };
	assert_ne!(kind, 0);
}

#[test]
fn the_null_selector_is_no_registered_name() {
	assert!(Sel::NULL.0.is_null());
	assert_ne!(sel(c"description"), Sel::NULL);
	assert_eq!(Sel::NULL, Sel::NULL);
}
