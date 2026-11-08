//! Tests of messages and ownership against classes of our own, registered
//! with the real runtime: methods of every arity we send, whose arguments
//! are integers of each width, floats, pointers and structures by value,
//! record what arrived and where; an `init` that refuses; and weak
//! references that say exactly when an object is deallocated.
use super::{Bool, Class, NIL, Object, Pool, Raw, Sel, class, sel};
use std::{
	cell::RefCell,
	ffi::{CStr, c_char, c_void},
	sync::OnceLock,
};

#[link(name = "objc")]
unsafe extern "C" {
	fn objc_allocateClassPair(superclass: Raw, name: *const c_char, extra: usize) -> Raw;
	fn objc_registerClassPair(class: Raw);
	fn class_addMethod(class: Raw, name: Sel, imp: *const c_void, types: *const c_char) -> Bool;
	fn objc_initWeak(location: *mut Raw, object: Raw) -> Raw;
	fn objc_loadWeakRetained(location: *mut Raw) -> Raw;
	fn objc_destroyWeak(location: *mut Raw);
}

/// A structure of six words, as `MTLRegion` is: passed by reference to a
/// copy, as the C ABI passes structures over 16 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Six {
	words: [usize; 6],
}

/// Four doubles, as `NSRect` is: passed in floating-point registers on
/// arm64, in memory on x86-64.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Quad {
	values: [f64; 4],
}

/// What a probe method received, in order, as text.
#[derive(Debug, PartialEq)]
enum Arrived {
	Nothing,
	Byte(u8),
	Pair(i16, u32),
	Three(i8, u64, f32),
	Four(f64, *const c_void, i64, Bool),
	Five(usize, f32, u16, f64, i32),
	Six(u8, f64, Six, i64, f32, Quad),
}

thread_local! {
	/// What this thread's probe calls received.
	static ARRIVED: RefCell<Vec<Arrived>> = const { RefCell::new(Vec::new()) };
}

fn arrived(what: Arrived) {
	ARRIVED.with(|arrived| arrived.borrow_mut().push(what));
}

/// Takes what the probe calls on this thread received.
fn taken() -> Vec<Arrived> {
	ARRIVED.with(|arrived| std::mem::take(&mut *arrived.borrow_mut()))
}

extern "C" fn nothing(_: Raw, _: Sel) -> u64 {
	arrived(Arrived::Nothing);
	0xfeed_face_cafe_beef
}

extern "C" fn byte(_: Raw, _: Sel, a: u8) -> u8 {
	arrived(Arrived::Byte(a));
	a.wrapping_add(1)
}

extern "C" fn pair(_: Raw, _: Sel, a: i16, b: u32) -> i64 {
	arrived(Arrived::Pair(a, b));
	i64::from(a) - i64::from(b)
}

extern "C" fn three(_: Raw, _: Sel, a: i8, b: u64, c: f32) -> f32 {
	arrived(Arrived::Three(a, b, c));
	c * 2.0
}

extern "C" fn four(_: Raw, _: Sel, a: f64, b: *const c_void, c: i64, d: Bool) -> *const c_void {
	arrived(Arrived::Four(a, b, c, d));
	b.wrapping_byte_add(1)
}

extern "C" fn five(_: Raw, _: Sel, a: usize, b: f32, c: u16, d: f64, e: i32) -> f64 {
	arrived(Arrived::Five(a, b, c, d, e));
	d + f64::from(b)
}

extern "C" fn six(_: Raw, _: Sel, a: u8, b: f64, c: Six, d: i64, e: f32, f: Quad) -> Bool {
	arrived(Arrived::Six(a, b, c, d, e, f));
	1
}

/// An `init` that gives up its object and refuses, as one does when it
/// cannot make what it was asked for.
extern "C" fn refuse(this: Raw, _: Sel) -> Raw {
	// SAFETY: `init` owns the object `alloc` made, given up here.
	unsafe { super::objc_release(this) };
	NIL
}

/// A method that returns nil, or a null C string.
extern "C" fn silent(_: Raw, _: Sel) -> Raw {
	NIL
}

/// Registers `name`, a subclass of `NSObject` with `methods`, once: the
/// caller registers each name once.
pub(crate) fn register(name: &'static CStr, methods: &[(&CStr, *const c_void, &CStr)]) -> Class {
	let object = class(c"NSObject").unwrap();
	// SAFETY: the name is new to the runtime (each is registered once, by
	// the `OnceLock`s below), the superclass is a class, and each method's
	// implementation is an `extern "C"` function taking the receiver and
	// selector first, as the runtime calls it.
	unsafe {
		let made = objc_allocateClassPair(object.raw(), name.as_ptr(), 0);
		assert!(!made.is_null(), "{name:?} taken");
		for &(selector, imp, types) in methods {
			assert_ne!(class_addMethod(made, sel(selector), imp, types.as_ptr()), 0);
		}
		objc_registerClassPair(made);
	}
	class(name).unwrap()
}

/// The probe class, whose methods record what they receive.
fn probe() -> Class {
	static PROBE: OnceLock<usize> = OnceLock::new();
	let raw = *PROBE.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 7] = [
			(c"nothing", nothing as *const c_void, c"Q@:"),
			(c"byte:", byte as *const c_void, c"C@:C"),
			(c"pair:b:", pair as *const c_void, c"q@:sI"),
			(c"three:b:c:", three as *const c_void, c"f@:cQf"),
			(c"four:b:c:d:", four as *const c_void, c"^v@:d^vqc"),
			(c"five:b:c:d:e:", five as *const c_void, c"d@:QfSdi"),
			(
				c"six:b:c:d:e:f:",
				six as *const c_void,
				c"c@:Cd{Six=QQQQQQ}qf{Quad=dddd}",
			),
		];
		register(c"PooptermProbe", &methods).raw() as usize
	});
	class_named(raw)
}

/// A class whose `init` refuses.
fn refusing() -> Class {
	static REFUSING: OnceLock<usize> = OnceLock::new();
	let raw = *REFUSING.get_or_init(|| {
		register(
			c"PooptermRefusing",
			&[(c"init", refuse as *const c_void, c"@@:")],
		)
		.raw() as usize
	});
	class_named(raw)
}

/// A new probe, owned.
fn probed() -> Object {
	// SAFETY: a subclass of `NSObject`, whose `init` is `NSObject`'s.
	unsafe { probe().instance() }.unwrap()
}

/// An object that says nothing: its `localizedDescription` is nil and its
/// `UTF8String` null, as an error or string may be.
fn silent_object() -> Object {
	static SILENT: OnceLock<usize> = OnceLock::new();
	let raw = *SILENT.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 2] = [
			(c"localizedDescription", silent as *const c_void, c"@@:"),
			(c"UTF8String", silent as *const c_void, c"r*@:"),
		];
		register(c"PooptermSilent", &methods).raw() as usize
	});
	// SAFETY: a subclass of `NSObject`, whose `init` is `NSObject`'s.
	unsafe { class_named(raw).instance() }.unwrap()
}

pub(crate) fn class_named(raw: usize) -> Class {
	Class(std::ptr::NonNull::new(raw as Raw).unwrap())
}

/// A weak reference to an object, which reads nil once it is deallocated.
pub(crate) struct Weak(Box<Raw>);

impl Weak {
	pub(crate) fn new(object: &Object) -> Self {
		// SAFETY: a live object.
		unsafe { Self::of(object.raw()) }
	}

	/// A weak reference to `raw`.
	///
	/// # Safety
	///
	/// `raw` is a live object.
	pub(crate) unsafe fn of(raw: Raw) -> Self {
		let mut location = Box::new(NIL);
		// SAFETY: the location is boxed, so it stays put until destroyed.
		unsafe { objc_initWeak(&mut *location, raw) };
		Self(location)
	}

	/// Whether the object still lives.
	pub(crate) fn alive(&mut self) -> bool {
		// SAFETY: the location was initialized and not yet destroyed; the
		// object read is retained, and released as the `Object` drops.
		unsafe { Object::owned(objc_loadWeakRetained(&mut *self.0)) }.is_some()
	}
}

impl Drop for Weak {
	fn drop(&mut self) {
		// SAFETY: initialized, and destroyed once.
		unsafe { objc_destroyWeak(&mut *self.0) };
	}
}

#[test]
fn a_message_of_no_arguments_returns_all_sixty_four_bits() {
	let probe = probed();
	taken();
	// SAFETY: `nothing` takes nothing and returns a `u64`.
	let got: u64 = unsafe { probe.send(c"nothing", ()) };
	assert_eq!(got, 0xfeed_face_cafe_beef);
	assert_eq!(taken(), [Arrived::Nothing]);
}

#[test]
fn bytes_and_small_integers_arrive_at_their_extremes() {
	let probe = probed();
	taken();
	// SAFETY: `byte:` takes and returns a `u8`; `pair:b:` takes an `i16` and
	// a `u32` and returns an `i64`.
	unsafe {
		assert_eq!(probe.send::<u8>(c"byte:", (0u8,)), 1);
		assert_eq!(probe.send::<u8>(c"byte:", (u8::MAX,)), 0);
		let low: i64 = probe.send(c"pair:b:", (i16::MIN, u32::MAX));
		assert_eq!(low, i64::from(i16::MIN) - i64::from(u32::MAX));
		let high: i64 = probe.send(c"pair:b:", (i16::MAX, 0u32));
		assert_eq!(high, i64::from(i16::MAX));
	}
	assert_eq!(
		taken(),
		[
			Arrived::Byte(0),
			Arrived::Byte(u8::MAX),
			Arrived::Pair(i16::MIN, u32::MAX),
			Arrived::Pair(i16::MAX, 0),
		]
	);
}

#[test]
fn integers_and_floats_mixed_keep_their_order_and_bits() {
	let probe = probed();
	taken();
	// SAFETY: `three:b:c:` takes an `i8`, a `u64` and an `f32`, and returns an
	// `f32`; `five:b:c:d:e:` takes a `usize`, `f32`, `u16`, `f64` and `i32`,
	// and returns an `f64`.
	unsafe {
		let doubled: f32 = probe.send(c"three:b:c:", (-128i8, u64::MAX, 1.5f32));
		assert_eq!(doubled, 3.0);
		let sum: f64 = probe.send(
			c"five:b:c:d:e:",
			(usize::MAX, 0.25f32, 0xbeefu16, -1e300f64, i32::MIN),
		);
		assert_eq!(sum, -1e300 + 0.25);
	}
	assert_eq!(
		taken(),
		[
			Arrived::Three(-128, u64::MAX, 1.5),
			Arrived::Five(usize::MAX, 0.25, 0xbeef, -1e300, i32::MIN),
		]
	);
}

#[test]
fn pointers_and_bools_arrive_and_pointers_return_whole() {
	let probe = probed();
	taken();
	let target = 0u64;
	let at = (&raw const target).cast::<c_void>();
	// SAFETY: `four:b:c:d:` takes an `f64`, a pointer, an `i64` and a `BOOL`,
	// and returns a pointer.
	let back: *const c_void =
		unsafe { probe.send(c"four:b:c:d:", (f64::MIN_POSITIVE, at, i64::MIN, 1 as Bool)) };
	assert_eq!(back, at.wrapping_byte_add(1));
	assert_eq!(taken(), [Arrived::Four(f64::MIN_POSITIVE, at, i64::MIN, 1)]);
}

#[test]
fn six_arguments_with_structures_by_value_arrive_whole() {
	let probe = probed();
	taken();
	let region = Six {
		words: [1, 2, 3, usize::MAX - 1, 5, usize::MAX],
	};
	let rect = Quad {
		values: [0.5, -0.0, f64::MAX, 1e-310],
	};
	// SAFETY: `six:b:c:d:e:f:` takes a `u8`, an `f64`, the six-word structure,
	// an `i64`, an `f32` and the four-double one, and returns a `BOOL`.
	let done: Bool = unsafe {
		probe.send(
			c"six:b:c:d:e:f:",
			(7u8, 2.5f64, region, -9i64, -0.5f32, rect),
		)
	};
	assert_eq!(done, 1);
	assert_eq!(taken(), [Arrived::Six(7, 2.5, region, -9, -0.5, rect)]);
}

#[test]
fn an_object_answers_for_the_methods_its_class_was_given_and_no_others() {
	// The class registered is the one looked up by its name.
	let registered = probe();
	assert_eq!(class(c"PooptermProbe"), Ok(registered));
	let object = probed();
	assert!(object.responds(c"six:b:c:d:e:f:"));
	assert!(object.responds(c"description"));
	assert!(!object.responds(c"six:b:c:d:e:"));
	assert!(!object.responds(c"seven"));
}

#[test]
fn an_instance_whose_init_refuses_is_none_and_leaks_nothing() {
	let refusing = refusing();
	// SAFETY: a subclass of `NSObject`; its `init` takes nothing.
	assert!(unsafe { refusing.instance() }.is_none());
	// SAFETY: as above.
	// Twice: the refused objects were released, not left half made.
	assert!(unsafe { refusing.instance() }.is_none());
}

#[test]
fn an_object_is_deallocated_when_its_last_reference_drops_and_not_before() {
	// SAFETY: a subclass of `NSObject`.
	let object = unsafe { probe().instance() }.unwrap();
	let mut weak = Weak::new(&object);
	let clone = object.clone();
	drop(object);
	assert!(weak.alive());
	// SAFETY: a live object.
	let retained = unsafe { Object::retained(clone.raw()) }.unwrap();
	drop(clone);
	assert!(weak.alive());
	drop(retained);
	assert!(!weak.alive());
}

#[test]
fn an_object_autoreleased_in_a_pool_lives_until_the_pool_drops() {
	let mut weak;
	{
		let _pool = Pool::new();
		// SAFETY: a subclass of `NSObject`.
		let object = unsafe { probe().instance() }.unwrap();
		weak = Weak::new(&object);
		// SAFETY: `retain` and `autorelease` take nothing and return the
		// object; the pool releases the reference `retain` added.
		unsafe {
			let _: Raw = object.send(c"retain", ());
			let _: Raw = object.send(c"autorelease", ());
		}
		drop(object);
		assert!(weak.alive());
	}
	assert!(!weak.alive());
}

#[test]
fn a_string_of_many_pages_and_every_plane_survives_the_round_trip() {
	let long: String = (0..100_000u32)
		.filter_map(|i| char::from_u32(i * 11 % 0x11_0000))
		.filter(|&c| c != '\0')
		.collect();
	let _pool = Pool::new();
	let made = super::string(&long).unwrap();
	// SAFETY: an `NSString`.
	assert_eq!(unsafe { super::text(&made) }, long);
}

#[test]
fn an_error_without_a_description_is_described_by_the_fallback() {
	let silent = silent_object();
	// SAFETY: answers `localizedDescription`, with nil.
	let said = unsafe { super::describe(silent.raw(), "said nothing") };
	assert_eq!(said, "said nothing");
	// The error was retained and released once: it still lives.
	let mut weak = Weak::new(&silent);
	assert!(weak.alive());
	drop(silent);
	assert!(!weak.alive());
}

#[test]
fn a_string_without_text_reads_as_empty() {
	// SAFETY: answers `UTF8String`, with null.
	assert_eq!(unsafe { super::text(&silent_object()) }, "");
}

#[test]
fn class_methods_carry_integers_and_floats_through_foundation_at_their_extremes() {
	let _pool = Pool::new();
	let number = class(c"NSNumber").unwrap();
	for value in [i64::MIN, -1, 0, 1, i64::MAX] {
		// SAFETY: `numberWithLongLong:` takes an `i64` and returns an
		// autoreleased number, whose `longLongValue` returns it.
		let back: i64 = unsafe {
			let made = Object::retained(number.send(c"numberWithLongLong:", (value,))).unwrap();
			made.send(c"longLongValue", ())
		};
		assert_eq!(back, value);
	}
	for value in [f64::MIN, -0.5, f64::MIN_POSITIVE, 1e300, f64::INFINITY] {
		// SAFETY: as above, with `numberWithDouble:` and `doubleValue`.
		let back: f64 = unsafe {
			let made = Object::retained(number.send(c"numberWithDouble:", (value,))).unwrap();
			made.send(c"doubleValue", ())
		};
		assert_eq!(back.to_bits(), value.to_bits());
	}
	// SAFETY: `numberWithFloat:` takes an `f32`; `floatValue` returns one.
	let back: f32 = unsafe {
		let made = Object::retained(number.send(c"numberWithFloat:", (f32::MAX,))).unwrap();
		made.send(c"floatValue", ())
	};
	assert_eq!(back, f32::MAX);
}

#[test]
fn an_object_made_in_a_pool_and_owned_outlives_it() {
	let (kept, mut weak) = {
		let _pool = Pool::new();
		let class = class(c"NSMutableArray").unwrap();
		// SAFETY: `array` returns an autoreleased array, retained here.
		let made = unsafe { Object::retained(class.send(c"array", ())) }.unwrap();
		let weak = Weak::new(&made);
		(made, weak)
	};
	assert!(weak.alive());
	// SAFETY: `count` takes nothing and returns an `NSUInteger`.
	assert_eq!(unsafe { kept.send::<usize>(c"count", ()) }, 0);
	drop(kept);
	assert!(!weak.alive());
}

#[test]
fn a_pool_releases_every_object_autoreleased_in_it_across_its_pages() {
	// Far more than one page of the runtime's pool holds (4 KiB).
	const MANY: usize = 10_000;
	let (mut first, mut last);
	{
		let _pool = Pool::new();
		let mut weaks = Vec::new();
		for _ in 0..MANY {
			let object = probed();
			// SAFETY: as in the test above: the pool releases what `retain` added.
			unsafe {
				let _: Raw = object.send(c"retain", ());
				let _: Raw = object.send(c"autorelease", ());
			}
			weaks.push(Weak::new(&object));
		}
		last = weaks.pop().unwrap();
		first = weaks.swap_remove(0);
		assert!(first.alive() && last.alive());
	}
	assert!(!first.alive());
	assert!(!last.alive());
}

#[test]
fn an_error_s_own_description_comes_back_exactly_in_any_script() {
	let _pool = Pool::new();
	let said = "the GPU stopped — ハングした 💥";
	let (key, value) = (
		super::string("NSLocalizedDescription").unwrap(),
		super::string(said).unwrap(),
	);
	let (dictionary, error) = (class(c"NSDictionary").unwrap(), class(c"NSError").unwrap());
	// SAFETY: `dictionaryWithObject:forKey:` takes two objects and returns an
	// autoreleased dictionary; `errorWithDomain:code:userInfo:` a string, an
	// `NSInteger` and that dictionary, returning an autoreleased error.
	let described = unsafe {
		let info: Raw = dictionary.send(c"dictionaryWithObject:forKey:", (value.raw(), key.raw()));
		let domain = super::string("Poopterm").unwrap();
		let made: Raw = error.send(
			c"errorWithDomain:code:userInfo:",
			(domain.raw(), -1isize, info),
		);
		super::describe(made, "nothing")
	};
	assert_eq!(described, said);
}
