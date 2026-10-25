//! Kitty animation emulation tests: frames sent with `a=f`, composed with
//! `a=c`, chosen and played with `a=a`, deleted with `d=f`.
use super::kitty_tests::{command, feed, replies, send, terminal};
use crate::term::Term;

/// Sends `a=f` frame data for image 1: RGB pixels `[r, g, b]` repeated.
fn frame(term: &mut Term, control: &str, pixels: &[[u8; 3]]) {
	let data: Vec<u8> = pixels.concat();
	feed(term, &command(&format!("a=f,i=1,f=24,{control}"), &data));
}

fn control(term: &mut Term, control: &str) {
	feed(term, format!("\x1b_G{control}\x1b\\").as_bytes());
}

/// The pixels the first placement shows.
fn shown(term: &Term) -> Vec<u32> {
	term.images[0].bitmap.pixels.clone()
}

/// Image 1: a placed 2×1 image with pixels 0xff000203 and 0xff010203.
fn image() -> Term {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,i=1,q=1", 2, 1);
	t
}

const ROOT: [u32; 2] = [0xff00_0203, 0xff01_0203];

#[test]
fn frames_are_composed_from_data_and_chosen_by_number() {
	let mut t = image();
	frame(&mut t, "", &[[9, 9, 9], [8, 8, 8]]);
	assert_eq!(replies(&mut t), ["\x1b_Gi=1;OK\x1b\\"]);
	// Partial data on top of frame 1, then on a background color.
	frame(&mut t, "c=1,x=1,s=1,v=1,q=1", &[[7, 7, 7]]);
	frame(&mut t, "x=1,s=1,v=1,Y=4278190335,q=1", &[[6, 6, 6]]);
	assert_eq!(shown(&t), ROOT);
	control(&mut t, "a=a,i=1,c=2");
	assert_eq!(shown(&t), [0xff09_0909, 0xff08_0808]);
	control(&mut t, "a=a,i=1,c=3");
	assert_eq!(shown(&t), [0xff00_0203, 0xff07_0707]);
	control(&mut t, "a=a,i=1,c=4");
	assert_eq!(shown(&t), [0xffff_0000, 0xff06_0606]);
	replies(&mut t);
	// Editing the shown frame shows the edit; `X=1` overwrites alpha too.
	let data = [1, 2, 3, 0];
	feed(&mut t, &command("a=f,i=1,r=4,s=1,v=1,X=1,q=1", &data));
	assert_eq!(shown(&t), [0x0001_0203, 0xff06_0606]);
	assert!(replies(&mut t).is_empty());
}

#[test]
fn frame_errors_name_the_problem() {
	let mut t = image();
	let code = |t: &mut Term| {
		let reply = replies(t).pop().unwrap();
		reply.split([';', ':']).nth(1).unwrap().to_owned()
	};
	feed(&mut t, &command("a=f,i=9,f=24,s=1,v=1", &[0; 3]));
	assert_eq!(code(&mut t), "ENOENT");
	frame(&mut t, "x=2,s=1,v=1", &[[0; 3]]);
	assert_eq!(code(&mut t), "EINVAL");
	frame(&mut t, "r=3,s=1,v=1", &[[0; 3]]);
	assert_eq!(code(&mut t), "ENOENT");
	frame(&mut t, "c=3,s=1,v=1", &[[0; 3]]);
	assert_eq!(code(&mut t), "ENOENT");
	control(&mut t, "a=a,i=1,c=2");
	assert_eq!(code(&mut t), "ENOENT");
	control(&mut t, "a=a,i=9,s=3");
	assert_eq!(code(&mut t), "ENOENT");
}

#[test]
fn frames_can_be_sent_for_image_numbers() {
	let mut t = terminal(20, 10);
	send(&mut t, "a=T,I=4,q=1", 2, 1);
	feed(&mut t, &command("a=f,I=4,f=24,s=2,v=1", &[0; 6]));
	let reply = replies(&mut t).pop().unwrap();
	assert!(reply.starts_with("\x1b_Gi=") && reply.ends_with(",I=4;OK\x1b\\"));
}

#[test]
fn running_animations_follow_their_gaps_on_the_clock() {
	let mut t = image();
	frame(&mut t, "z=100,q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=a,i=1,r=1,z=50");
	assert_eq!(t.animation_deadline(), None, "stopped");
	control(&mut t, "a=a,i=1,s=3,v=1");
	let deadline = t.animation_deadline().unwrap();
	assert!(!t.advance_animations(deadline - std::time::Duration::from_millis(1)));
	assert!(t.advance_animations(deadline));
	assert_eq!(shown(&t)[0], 0xff09_0909);
	let next = t.animation_deadline().unwrap();
	assert_eq!(next - deadline, std::time::Duration::from_millis(100));
	assert!(t.advance_animations(next));
	assert_eq!(shown(&t), ROOT);
	control(&mut t, "a=a,i=1,s=1");
	assert_eq!(t.animation_deadline(), None);
}

#[test]
fn hidden_animations_do_not_wake_the_terminal() {
	let mut t = image();
	frame(&mut t, "q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=a,i=1,s=3");
	assert!(t.animation_deadline().is_some());
	control(&mut t, "a=d,d=i,i=1");
	assert_eq!(t.animation_deadline(), None);
	// A virtual placement may be shown through placeholders.
	control(&mut t, "a=p,i=1,U=1");
	assert!(t.animation_deadline().is_some());
}

#[test]
fn composing_copies_rectangles_between_frames() {
	let mut t = image();
	frame(&mut t, "q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=c,i=1,r=2,c=1,w=1,h=1,X=1,x=0,C=1,q=1");
	assert_eq!(shown(&t), [0xff08_0808, 0xff01_0203]);
	// Within one frame, when the rectangles do not overlap.
	control(&mut t, "a=c,i=1,r=1,c=1,w=1,h=1,x=1,q=1");
	assert_eq!(shown(&t), [0xff08_0808; 2]);
	for (bad, code) in [
		("r=3,c=1", "ENOENT"),
		("r=2,c=1,w=3", "EINVAL"),
		("r=2,c=1,w=1,x=2", "EINVAL"),
		("r=1,c=1,w=2", "EINVAL"),
	] {
		control(&mut t, &format!("a=c,i=1,{bad}"));
		let reply = replies(&mut t).pop().unwrap();
		assert!(reply.contains(&format!(";{code}:")), "{bad}: {reply}");
	}
}

#[test]
fn deleting_frames_keeps_the_root_frame() {
	let mut t = image();
	frame(&mut t, "q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=a,i=1,c=2,s=3");
	control(&mut t, "a=d,d=f,i=1");
	assert_eq!(shown(&t), ROOT);
	assert_eq!(t.animation_deadline(), None);
	assert_eq!(t.images.len(), 1, "placements stay");
	control(&mut t, "a=a,i=1,c=2");
	assert!(replies(&mut t).pop().unwrap().contains("ENOENT"));
}

#[test]
fn reduced_motion_holds_animations_on_their_current_frame() {
	let mut t = image();
	t.reduce_motion = true;
	frame(&mut t, "z=100,q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=a,i=1,s=3,v=1");
	assert_eq!(t.animation_deadline(), None, "nothing to wake for");
	let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
	assert!(!t.advance_animations(later));
	assert_eq!(shown(&t), ROOT);
	// A program choosing a frame itself still shows it: that is not motion
	// the terminal starts on its own.
	control(&mut t, "a=a,i=1,c=2");
	assert_eq!(shown(&t)[0], 0xff09_0909);
}

#[test]
fn turning_reduced_motion_off_resumes_the_animation() {
	let mut t = image();
	frame(&mut t, "z=100,q=1", &[[9, 9, 9], [8, 8, 8]]);
	control(&mut t, "a=a,i=1,s=3,v=1");
	t.reduce_motion = true;
	assert_eq!(t.animation_deadline(), None);
	t.reduce_motion = false;
	let deadline = t.animation_deadline().unwrap();
	assert!(t.advance_animations(deadline));
	assert_eq!(shown(&t)[0], 0xff09_0909);
}

#[test]
fn reduced_motion_also_holds_animations_still_loading() {
	// `s=2` plays frames as they arrive; held, new frames do not move it.
	let mut t = image();
	t.reduce_motion = true;
	control(&mut t, "a=a,i=1,s=2");
	frame(&mut t, "z=10,q=1", &[[9, 9, 9], [8, 8, 8]]);
	frame(&mut t, "z=10,q=1", &[[7, 7, 7], [6, 6, 6]]);
	assert_eq!(t.animation_deadline(), None);
	let later = std::time::Instant::now() + std::time::Duration::from_secs(1);
	assert!(!t.advance_animations(later));
	assert_eq!(shown(&t), ROOT);
}
