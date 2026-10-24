//! Pixel equivalence and timing for highlight damage, including scrolling.
use super::{Canvas, DrawKey, Plan};
use crate::{
	parser::Parser,
	render::{
		glyphs::FontSet,
		grid::{GridView, draw_grid},
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
	ui::selection::CellPoint,
};

const KEY: DrawKey = DrawKey {
	session: 1,
	focused: true,
	selection: None,
	search_match: None,
	overlay: false,
};

fn fonts() -> FontSet {
	FontSet::new(
		crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap(),
		Vec::new(),
	)
}

fn draw(
	pixels: &mut [u32],
	fonts: &mut FontSet,
	frame: &Frame,
	key: DrawKey,
	rows: Option<&[bool]>,
) {
	let bg = Theme::from_kind(ThemeKind::Poop).bg;
	let width = frame.cols * 10;
	let height = frame.rows * 20;
	let mut painter = Painter::new(pixels, width, height);
	for row in 0..frame.rows {
		if rows.is_none_or(|rows| rows[row]) {
			painter.rect(0, row * 20, width, 20, bg);
		}
	}
	let view = GridView {
		frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 16.0,
		ascent: 15,
		scale: 1,
		default_bg: bg,
		link_color: 0x123456,
		selection_colors: (0x111111, 0xffffff),
		match_colors: (0, 0xffff00),
		selection: key.selection,
		search_match: key.search_match,
		focused: key.focused,
		padding: None,
	};
	draw_grid(&mut painter, fonts, &view, rows);
}

fn update(canvas: &mut Canvas, fonts: &mut FontSet, frame: &mut Frame, key: DrawKey) {
	let width = frame.cols * 10;
	let height = frame.rows * 20;
	let plan = canvas.plan(frame, key, (width, height), true);
	let rows = match &plan {
		Plan::Full => None,
		Plan::Partial(damage) => {
			if damage.scroll > 0 {
				canvas.pixels.copy_within(damage.scroll * 20 * width.., 0);
			}
			Some(damage.rows.as_slice())
		}
	};
	draw(&mut canvas.pixels, fonts, frame, key, rows);
	canvas.remember(frame, key);
}

#[test]
fn highlight_damage_and_scrolling_match_a_full_redraw_pixel_for_pixel() {
	let mut term = Term::new(12, 5, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	let mut fonts = fonts();
	let mut canvas = Canvas::new();
	let range = |row, col| Some((CellPoint { row, col: 0 }, CellPoint { row, col }));
	for (input, selection, search_match) in [
		("abc猫e\u{301}\r\ndef\r\nghi", None, None),
		("", range(1, 2), None),
		("", range(1, 4), None),
		("\r\nnew\r\nlast\r\nscroll", range(1, 4), range(4, 3)),
		("", None, range(2, 4)),
		("", None, None),
		("\x1b[H\x1b[K猫", range(2, 2), None),
	] {
		parser.advance(&mut term, input.as_bytes());
		let key = DrawKey {
			selection,
			search_match,
			..KEY
		};
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut expected = vec![0; frame.cols * 10 * frame.rows * 20];
		draw(&mut expected, &mut fonts, &frame, key, None);
		update(&mut canvas, &mut fonts, &mut frame, key);
		assert_eq!(canvas.pixels, expected, "input {input:?}, key {key:?}");
	}
}

/// A kitty command placing a `width`×`height` image whose pixels vary in
/// color and alpha, at the cursor and without moving it.
fn image(control: &str, width: usize, height: usize) -> String {
	let pixels: Vec<u8> = (0..width * height)
		.flat_map(|i| [(i * 7) as u8, (i * 3) as u8, 90, (i * 13 % 256) as u8])
		.collect();
	let payload = String::from_utf8(crate::bench::images::base64(&pixels)).unwrap();
	format!("\x1b_Ga=T,f=32,s={width},v={height},C=1,q=2,{control};{payload}\x1b\\")
}

#[test]
fn images_match_a_full_redraw_pixel_for_pixel() {
	let mut term = Term::new(12, 6, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	let mut fonts = fonts();
	let mut canvas = Canvas::new();
	let steps = [
		"\x1b[44mblue bg\x1b[0m\r\nplain text\r\n\x1b[41mred\x1b[0m".to_owned(),
		format!("\x1b[H{}", image("i=1,p=1,z=-1", 25, 50)),
		format!("\x1b[2;3H{}", image("i=2,z=-1073741825,X=3,Y=4", 30, 30)),
		format!("\x1b[3;6H{}", image("i=3,z=2,c=3,r=2", 8, 8)),
		"\x1b[1;2Hxy\x1b[3;1H\x1b[42mgreen\x1b[0m".to_owned(),
		"\x1b[5;5H\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\".to_owned(),
		"\x1b[6;1Hbottom".to_owned(),
		"\x1b_Ga=d,d=i,i=3\x1b\\".to_owned(),
		// An animation frame of image 1, then showing it.
		format!(
			"\x1b_Ga=f,i=1,f=24,x=2,y=3,s=2,v=2,q=2;{}\x1b\\",
			String::from_utf8(crate::bench::images::base64(&[200; 12])).unwrap()
		),
		"\x1b_Ga=a,i=1,c=2,q=2\x1b\\".to_owned(),
		"\r\n\r\nscrolled".to_owned(),
		"\x1b[2J".to_owned(),
		// Unicode placeholders: text showing a virtual placement, then the
		// image changing under unchanged text, then the placement going.
		format!("\x1b[H{}", image("i=4,U=1,c=3,r=2", 20, 30)),
		"\x1b[38;5;4m\u{10EEEE}\u{305}\u{10EEEE}\u{10EEEE}\r\n\u{10EEEE}\u{30d}\u{10EEEE}\x1b[m"
			.to_owned(),
		image("i=4,U=1,c=3,r=2", 10, 10),
		"\x1b_Ga=d,d=i,i=4\x1b\\".to_owned(),
	];
	for input in &steps {
		parser.advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut expected = vec![0; frame.cols * 10 * frame.rows * 20];
		draw(&mut expected, &mut fonts, &frame, KEY, None);
		update(&mut canvas, &mut fonts, &mut frame, KEY);
		assert!(canvas.pixels == expected, "input {input:?}");
	}
}

#[test]
fn negative_z_images_cover_backgrounds_unless_far_below_and_never_text() {
	let mut fonts = fonts();
	let mut pixel = |control: &str, text: &str| {
		let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
		let opaque = vec![255u8; 10 * 20 * 3];
		let payload = String::from_utf8(crate::bench::images::base64(&opaque)).unwrap();
		let input = format!(
			"\x1b[?25l\x1b[44m{text}\x1b[0m\x1b[H\x1b_Ga=T,f=24,s=10,v=20,C=1,q=2,{control};{payload}\x1b\\"
		);
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut pixels = vec![0; 4 * 10 * 2 * 20];
		draw(&mut pixels, &mut fonts, &frame, KEY, None);
		pixels
	};
	let blue = Theme::from_kind(ThemeKind::Poop).palette[4];
	// A blank cell with a blue background: the image is over it at z=-1 and
	// under it below INT32_MIN/2.
	assert_eq!(pixel("z=-1", " ")[0], 0xffffff);
	assert_eq!(pixel("z=-1073741825", " ")[0], blue);
	assert_eq!(pixel("z=0", " ")[0], 0xffffff);
	// Text stays on top of negative z, and is covered at z >= 0.
	let glyph = |pixels: Vec<u32>| {
		(0..20)
			.flat_map(|y| (0..10).map(move |x| y * 40 + x))
			.filter(|&i| pixels[i] != 0xffffff)
			.count()
	};
	assert!(glyph(pixel("z=-1", "█")) > 0);
	assert_eq!(glyph(pixel("z=1", "█")), 0);
}

#[test]
fn placeholders_show_the_image_fitted_and_centered_in_their_cells() {
	let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	// A 40×20 image, red on the left and blue on the right, fitted into 2×2
	// cells (20×40 pixels): 20×10, centered vertically at y = 15.
	let pixels: Vec<u8> = (0..40 * 20)
		.flat_map(|i| {
			if i % 40 < 20 {
				[255, 0, 0]
			} else {
				[0, 0, 255]
			}
		})
		.collect();
	let payload = String::from_utf8(crate::bench::images::base64(&pixels)).unwrap();
	let input = format!(
		"\x1b[?25l\x1b_Ga=T,U=1,i=1,f=24,s=40,v=20,c=2,r=2,q=2;{payload}\x1b\\\
		 \x1b[31m\u{10EEEE}\u{10EEEE}\r\n\u{10EEEE}\u{30d}\u{10EEEE}x"
	);
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	let bg = Theme::from_kind(ThemeKind::Poop).bg;
	let mut pixels = vec![0; 4 * 10 * 2 * 20];
	draw(&mut pixels, &mut fonts(), &frame, KEY, None);
	let at = |x: usize, y: usize| pixels[y * 40 + x];
	assert_eq!([at(5, 5), at(15, 5), at(5, 14), at(5, 30)], [bg; 4]);
	assert_eq!([at(5, 15), at(9, 24)], [0xff0000; 2]);
	assert_eq!([at(10, 15), at(19, 24)], [0x0000ff; 2]);
	// No glyph is drawn for the placeholder character itself.
	assert!((0..20).all(|y| (0..20).all(|x| [bg, 0xff0000, 0x0000ff].contains(&at(x, y)))));
}

#[test]
fn program_cursor_and_selection_colors_are_drawn_and_redraw_the_grid() {
	let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	let mut fonts = fonts();
	let mut canvas = Canvas::new();
	let key = DrawKey {
		selection: Some((CellPoint { row: 1, col: 0 }, CellPoint { row: 1, col: 1 })),
		..KEY
	};
	for input in [
		"",
		// Block cursor at (0, 0); selection on row 1.
		"\x1b]12;#ff0000\x07\x1b]17;#00ff00\x07",
		"\x1b]21;selection_background=;selection_foreground=\x07",
	] {
		parser.advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut expected = vec![0; 4 * 10 * 2 * 20];
		draw(&mut expected, &mut fonts, &frame, key, None);
		update(&mut canvas, &mut fonts, &mut frame, key);
		assert!(canvas.pixels == expected, "input {input:?}");
	}
	let mut frame = Frame::default();
	parser.advance(&mut term, b"\x1b]21;selection_background=#00ff00\x07");
	term.snapshot(&mut frame);
	let mut pixels = vec![0; 4 * 10 * 2 * 20];
	draw(&mut pixels, &mut fonts, &frame, key, None);
	assert_eq!(pixels[0], 0xff0000, "cursor");
	assert_eq!(pixels[20 * 40], 0x00ff00, "selection background");
}

#[test]
fn reverse_screen_paints_blanks_in_the_foreground_and_redraws_fully() {
	let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	let mut fonts = fonts();
	let mut canvas = Canvas::new();
	let theme = Theme::from_kind(ThemeKind::Poop);
	for (input, corner) in [
		("ab\x1b[?25l", theme.bg),
		("\x1b[?5h", theme.fg),
		("\x1b[2;3Hc", theme.fg),
		("\x1b[?5l", theme.bg),
	] {
		parser.advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let mut expected = vec![0; 4 * 10 * 2 * 20];
		draw(&mut expected, &mut fonts, &frame, KEY, None);
		update(&mut canvas, &mut fonts, &mut frame, KEY);
		assert!(canvas.pixels == expected, "input {input:?}");
		assert_eq!(expected[4 * 10 * 2 * 20 - 1], corner, "input {input:?}");
	}
}

#[test]
#[ignore = "rendering benchmark; run explicitly in release mode"]
fn highlight_rendering_throughput() {
	let mut term = Term::new(100, 30, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, "hello 猫 e\u{301}\r\n".repeat(30).as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	let mut fonts = fonts();
	for full in [true, false] {
		let mut canvas = Canvas::new();
		let start = std::time::Instant::now();
		for index in 0..300 {
			let row = index % 30 + frame.first_row;
			let key = DrawKey {
				selection: Some((CellPoint { row, col: 1 }, CellPoint { row, col: 5 })),
				..KEY
			};
			if full {
				canvas.invalidate();
			}
			update(&mut canvas, &mut fonts, &mut frame.clone(), key);
		}
		println!(
			"highlight full={full}: {:?} for 300 frames",
			start.elapsed()
		);
	}
}
