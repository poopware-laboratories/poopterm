//! Tests of acquiring and presenting swapchain images against the fake
//! driver of `driver_tests`: which semaphores each frame signals and waits
//! for, reusing them, what each answer of the driver comes to, and waiting
//! for the queue before a swapchain goes.
use super::{
	call::Error,
	choose::{GRAPHICS, Kind},
	commands::Commands,
	device::Device,
	driver_tests::{Driver, finish, install, with, with_devices},
	image::Format,
	present::{Acquired, Presented},
	present_driver_tests::{NOT_READY, OUT_OF_DATE, SUBOPTIMAL, SURFACE_LOST, TIMEOUT},
	surface::{Platform, Surface},
	surface_driver_tests::{WAYLAND, gpu, present, surface, window_instance},
	surface_query_tests::mesa_wayland,
	swapchain::Swapchain,
	swapchain_plan::{MAILBOX, OPAQUE, Plan},
	timeline::Timeline,
};
use std::{rc::Rc, time::Duration};

/// A device for windows on `driver`, its timeline and commands, and a
/// Mesa Wayland surface with a 4-image swapchain on it.
pub(super) struct Window {
	pub commands: Commands,
	pub swapchain: Swapchain,
	pub device: Rc<Device>,
	pub surface: Rc<Surface>,
}

pub(super) fn window(driver: Driver) -> Window {
	let instance = window_instance(install(driver), Platform::Wayland);
	let physical = instance.physical_devices().unwrap()[0];
	let device = Rc::new(Device::for_windows(instance.clone(), physical, 0).unwrap());
	let surface = Rc::new(surface(&instance, WAYLAND).unwrap());
	let swapchain = Swapchain::new(&device, &surface, plan()).expect("swapchain");
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let commands = Commands::new(&timeline).unwrap();
	Window {
		commands,
		swapchain,
		device,
		surface,
	}
}

pub(super) fn showing() -> Driver {
	with_devices(&[gpu(Kind::Integrated, vec![present(GRAPHICS, true)])])
}

fn plan() -> Plan {
	Plan {
		format: Format::Bgra,
		mode: MAILBOX,
		extent: [640, 480],
		images: 4,
		transform: 1,
		alpha: OPAQUE,
	}
}

impl Window {
	/// Acquires an image and presents it with nothing drawn: what each
	/// came to.
	pub(super) fn frame(&mut self) -> Result<Presented, Error> {
		let Acquired::Frame(frame) = self.swapchain.acquire(Duration::MAX)? else {
			panic!("no frame");
		};
		self.commands.record()?.present(frame)
	}

	pub(super) fn close(self) -> Driver {
		drop(self);
		finish()
	}
}

/// The semaphore operations so far, emptied.
pub(super) fn operations() -> Vec<String> {
	with(|driver| std::mem::take(&mut driver.semaphore_ops))
}

#[test]
fn a_frame_waits_for_its_acquisition_and_is_shown_once_drawn() {
	let mut window = window(showing());
	assert_eq!(window.frame(), Ok(Presented::Shown));
	// The swapchain's four drawn semaphores come first, then the timeline,
	// then the acquire semaphore.
	let operations = [
		"acquire 0 signals 0xc00006",
		// The image is first written by drawing or copying, after the wait.
		"submit waits [\"0xc00006 at 0x1400\"] signals [\"0xc00001 at 0x10000\"]",
		"present 0 waits [\"0xc00001\"]",
	];
	assert_eq!(super::present_tests::operations(), operations);
	let driver = with(|driver| (driver.presents.clone(), driver.semaphores.clone()));
	assert_eq!(driver.0, [(0x6_0001, 0)]);
	assert_eq!(driver.1, [(0, 0), (0, 0), (0, 0), (0, 0), (1, 0), (0, 0)]);
	let barrier = with(|driver| {
		driver
			.recorded
			.iter()
			.find(|line| line.starts_with("barrier"))
			.cloned()
	});
	assert_eq!(barrier.as_deref(), Some("barrier 0xa00001 0 to 1000001002"));
	window.close();
}

#[test]
fn an_acquire_semaphore_is_reused_once_its_frame_is_done() {
	let mut window = window(showing());
	for _ in 0..3 {
		assert_eq!(window.frame(), Ok(Presented::Shown));
	}
	// The first frame is still drawing when the second is acquired, so
	// that takes another semaphore; recording the second waits for the
	// first, which frees its semaphore for the third.
	let acquired: Vec<_> = (operations().into_iter())
		.filter(|operation| operation.starts_with("acquire"))
		.collect();
	let semaphores = ["0xc00006", "0xc00007", "0xc00006"];
	let expected = semaphores.map(|semaphore| format!("acquire 0 signals {semaphore}"));
	assert_eq!(acquired, expected);
	assert_eq!(with(|driver| driver.semaphores.len()), 7);
	window.close();
}

#[test]
fn the_second_frame_into_an_image_moves_it_from_the_present_layout() {
	let mut window = window(showing());
	window.frame().unwrap();
	window.frame().unwrap();
	let barriers: Vec<_> = with(|driver| {
		(driver.recorded.iter())
			.filter(|line| line.starts_with("barrier"))
			.cloned()
			.collect()
	});
	assert_eq!(
		barriers,
		[
			"barrier 0xa00001 0 to 1000001002",
			"barrier 0xa00001 1000001002 to 1000001002"
		]
	);
	window.close();
}

#[test]
fn an_acquisition_out_of_date_or_timed_out_leaves_the_semaphore_free() {
	let mut driver = showing();
	driver.acquire_answers = vec![OUT_OF_DATE, TIMEOUT, NOT_READY];
	let mut window = window(driver);
	for _ in 0..2 {
		let acquired = window.swapchain.acquire(Duration::MAX).unwrap();
		assert!(matches!(acquired, Acquired::OutOfDate | Acquired::Timeout));
	}
	let acquired = window.swapchain.acquire(Duration::ZERO).unwrap();
	assert!(matches!(acquired, Acquired::Timeout));
	let acquired = window.swapchain.acquire(Duration::from_millis(5)).unwrap();
	assert!(matches!(acquired, Acquired::Frame(_)));
	// Timeouts reach the driver in nanoseconds, too long ones as forever.
	let timeouts = with(|driver| driver.acquire_timeouts.clone());
	assert_eq!(timeouts, [u64::MAX, u64::MAX, 0, 5_000_000]);
	// One acquire semaphore for all four tries: the three that failed left
	// it free.
	assert_eq!(with(|driver| driver.semaphores.len()), 6);
	window.close();
}

#[test]
fn a_suboptimal_acquisition_says_so_and_still_draws() {
	let mut driver = showing();
	driver.acquire_answers = vec![SUBOPTIMAL];
	let mut window = window(driver);
	let Acquired::Frame(frame) = window.swapchain.acquire(Duration::MAX).unwrap() else {
		panic!("no frame");
	};
	assert!(frame.suboptimal);
	assert_eq!(
		window.commands.record().unwrap().present(frame),
		Ok(Presented::Shown)
	);
	window.close();
}

#[test]
fn a_lost_surface_on_acquiring_is_an_error() {
	let mut driver = showing();
	driver.acquire_answers = vec![SURFACE_LOST];
	let mut window = window(driver);
	let acquired = window.swapchain.acquire(Duration::MAX).err();
	assert_eq!(acquired, Some(Error::Failed(SURFACE_LOST)));
	window.close();
}

#[test]
fn presenting_says_whether_the_swapchain_still_suits() {
	let mut driver = showing();
	driver.present_answers = vec![SUBOPTIMAL, OUT_OF_DATE, SURFACE_LOST];
	let mut window = window(driver);
	assert_eq!(window.frame(), Ok(Presented::Suboptimal));
	assert_eq!(window.frame(), Ok(Presented::OutOfDate));
	assert_eq!(window.frame(), Err(Error::Failed(SURFACE_LOST)));
	// Each was handed to the presentation engine all the same.
	assert_eq!(with(|driver| driver.presents.len()), 3);
	window.close();
}

#[test]
fn a_frame_dropped_unpresented_keeps_its_image_and_semaphore() {
	let mut driver = showing();
	driver.surface_capabilities = Some(super::swapchain_plan::Capabilities {
		min_images: 2,
		..mesa_wayland()
	});
	let mut window = window(driver);
	// Dropped at the end of the match, never presented.
	match window.swapchain.acquire(Duration::MAX).unwrap() {
		Acquired::Frame(frame) => {
			assert_eq!((frame.index(), frame.image().handle()), (0, 0xa0_0001));
		}
		_ => panic!("no frame"),
	}
	// The next frame gets another image and another semaphore.
	let Acquired::Frame(frame) = window.swapchain.acquire(Duration::MAX).unwrap() else {
		panic!("no frame");
	};
	assert_eq!((frame.index(), frame.image().handle()), (1, 0xa0_0002));
	let presented = window.commands.record().unwrap().present(frame);
	assert_eq!(presented, Ok(Presented::Shown));
	assert_eq!(with(|driver| driver.presents.clone()), [(0x6_0001, 1)]);
	let acquired: Vec<_> = (operations().into_iter())
		.filter(|operation| operation.starts_with("acquire"))
		.collect();
	assert_eq!(
		acquired,
		["acquire 0 signals 0xc00006", "acquire 1 signals 0xc00007"]
	);
	assert_eq!(
		with(|driver| driver.live_swapchains[0].acquired.clone()),
		[0]
	);
	window.close();
}

#[test]
fn a_failed_submission_presents_nothing() {
	let mut window = window(showing());
	let Acquired::Frame(frame) = window.swapchain.acquire(Duration::MAX).unwrap() else {
		panic!("no frame");
	};
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", -4)]);
	let recording = window.commands.record().unwrap();
	assert_eq!(recording.present(frame), Err(Error::Failed(-4)));
	assert!(with(|driver| driver.presents.is_empty()));
	window.close();
}

#[test]
fn presenting_with_another_devices_commands_is_refused() {
	let mut window = window(showing());
	let instance = window.device.instance.clone();
	let other = Rc::new(Device::for_windows(instance, window.device.physical, 0).unwrap());
	let mut commands = Commands::new(&Rc::new(Timeline::new(&other).unwrap())).unwrap();
	let Acquired::Frame(frame) = window.swapchain.acquire(Duration::MAX).unwrap() else {
		panic!("no frame");
	};
	assert_eq!(
		commands.record().unwrap().present(frame).err(),
		Some(Error::Foreign)
	);
	// The fake checks every object is gone before either device goes.
	let frame_device = window.device.clone();
	drop((window, commands));
	drop((other, frame_device));
	finish();
}

#[test]
fn a_swapchain_waits_for_the_queue_before_it_or_its_semaphores_go() {
	let mut window = window(showing());
	window.frame().unwrap();
	let plan = Plan {
		extent: [800, 600],
		..plan()
	};
	let Window {
		commands,
		swapchain,
		device,
		surface,
	} = window;
	// The fake reports a swapchain destroyed while presenting may be
	// pending, and semaphores destroyed while work signals them.
	let swapchain = swapchain.recreate(plan).unwrap();
	assert_eq!(with(|driver| driver.idle_waits), 1);
	drop(swapchain);
	assert_eq!(with(|driver| driver.idle_waits), 2);
	drop((commands, device, surface));
	finish();
}

#[test]
fn an_acquire_semaphore_that_cannot_be_made_is_an_error_and_acquires_nothing() {
	let mut window = window(showing());
	// VK_ERROR_OUT_OF_HOST_MEMORY.
	with(|driver| driver.fails = vec![(c"vkCreateSemaphore", -1)]);
	let acquired = window.swapchain.acquire(Duration::MAX).err();
	assert_eq!(acquired, Some(Error::Failed(-1)));
	assert!(with(|driver| driver.acquire_timeouts.is_empty()));
	// Nothing was left half made: the next try starts afresh.
	with(|driver| driver.fails.clear());
	assert_eq!(window.frame(), Ok(Presented::Shown));
	assert_eq!(with(|driver| driver.semaphores.len()), 6);
	window.close();
}

#[test]
fn a_swapchain_whose_semaphores_cannot_be_made_releases_its_views_and_itself() {
	let instance = window_instance(install(showing()), Platform::Wayland);
	let physical = instance.physical_devices().unwrap()[0];
	let device = Rc::new(Device::for_windows(instance.clone(), physical, 0).unwrap());
	let surface = Rc::new(surface(&instance, WAYLAND).unwrap());
	with(|driver| driver.fails = vec![(c"vkCreateSemaphore", -2)]);
	let made = Swapchain::new(&device, &surface, plan()).err();
	assert_eq!(made, Some(Error::Failed(-2)));
	// The views first, as they hold the swapchain, and no idle wait, as
	// nothing could have used it.
	let views = (1..=4).map(|n| format!("view {:#x}", 0xf0_0000 + n));
	let expected: Vec<_> = views.chain(["swapchain 0x60001".into()]).collect();
	let driver = with(|driver| (driver.released.clone(), driver.idle_waits));
	assert_eq!(driver, (expected, 0));
	drop((device, surface));
	assert!(finish().live_swapchains.is_empty());
}
