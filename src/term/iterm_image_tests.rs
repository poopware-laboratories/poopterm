//! iTerm2's inline images: OSC 1337 `File=`, and `MultipartFile`,
//! `FilePart` and `FileEnd`.
use crate::{
	graphics::{Extent, base64::encode},
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	(term.cell_width, term.cell_height) = (10, 20);
	term
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// A `width` × `height` PNG, base64-encoded.
fn png(width: u32, height: u32) -> String {
	use image::ImageEncoder;
	let pixels = [1u8, 2, 3, 255].repeat((width * height) as usize);
	let mut png = Vec::new();
	image::codecs::png::PngEncoder::new(&mut png)
		.write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
		.unwrap();
	encode(&png, true)
}

/// An image's cell, drawn size and pixel size.
type Shown = ((isize, usize), Extent, (usize, usize));

fn shown(term: &Term) -> Vec<Shown> {
	term
		.images
		.iter()
		.map(|image| {
			assert!(image.kitty.is_none());
			(
				(image.row, image.col),
				image.extent,
				(image.bitmap.width, image.bitmap.height),
			)
		})
		.collect()
}

#[test]
fn inline_images_are_shown_and_the_cursor_moves_past_them() {
	let mut t = terminal(10, 5);
	send(
		&mut t,
		&format!("ab\x1b]1337;File=inline=1:{}\x07", png(20, 30)),
	);
	assert_eq!(
		shown(&t),
		[(
			(0, 2),
			Extent::Pixels {
				width: 20,
				height: 30
			},
			(20, 30)
		)]
	);
	// On the image's last row, after its last column.
	assert_eq!((t.cx, t.cy), (4, 1));
	// Sizes follow the arguments.
	send(
		&mut t,
		&format!(
			"\r\x1b]1337;File=inline=1;width=3;size=99:{}\x1b\\",
			png(20, 30)
		),
	);
	assert_eq!(
		shown(&t)[1].1,
		Extent::Pixels {
			width: 30,
			height: 45
		}
	);
	assert_eq!((t.cx, t.cy), (3, 3));
}

#[test]
fn the_cursor_can_stay_put() {
	let mut t = terminal(10, 5);
	send(
		&mut t,
		&format!(
			"\x1b]1337;File=inline=1;doNotMoveCursor=1:{}\x07",
			png(20, 30)
		),
	);
	assert_eq!(shown(&t).len(), 1);
	assert_eq!((t.cx, t.cy), (0, 0));
}

#[test]
fn images_at_the_bottom_scroll_the_screen() {
	let mut t = terminal(10, 3);
	send(&mut t, "\r\n\r\nx");
	send(
		&mut t,
		&format!("\x1b]1337;File=inline=1:{}\x07", png(10, 50)),
	);
	// Three rows tall from the last row: the screen scrolls twice.
	assert_eq!(shown(&t)[0].0, (0, 1));
	assert_eq!((t.cx, t.cy), (2, 2));
	assert_eq!(t.scrollback.len(), 2);
}

#[test]
fn images_reaching_the_right_edge_wrap_pending() {
	let mut t = terminal(4, 3);
	send(
		&mut t,
		&format!("\x1b]1337;File=inline=1:{}\x07", png(40, 20)),
	);
	assert_eq!((t.cx, t.cy, t.wrap_pending), (3, 0, true));
	send(&mut t, "y");
	assert_eq!((t.cx, t.cy), (1, 1));
}

#[test]
fn text_written_over_an_image_erases_it() {
	let mut t = terminal(10, 3);
	send(
		&mut t,
		&format!("\x1b]1337;File=inline=1:{}\x07", png(20, 20)),
	);
	send(&mut t, "\rX");
	assert!(t.images.is_empty());
}

#[test]
fn downloads_and_bad_files_show_nothing() {
	let mut t = terminal(10, 3);
	for file in [
		format!("name=eC5wbmc=:{}", png(2, 2)),
		format!("inline=0:{}", png(2, 2)),
		"inline=1:@@@@".into(),
		format!("inline=1:{}", encode(b"GIF89a not really", true)),
		format!("inline=1;size=999999999:{}", png(2, 2)),
		"inline=1".into(),
	] {
		send(&mut t, &format!("\x1b]1337;File={file}\x07"));
	}
	assert!(t.images.is_empty());
	assert_eq!((t.cx, t.cy), (0, 0));
}

#[test]
fn multipart_files_arrive_in_pieces() {
	let mut t = terminal(10, 5);
	let data = png(20, 30);
	let (first, rest) = data.split_at(7);
	send(&mut t, "\x1b]1337;MultipartFile=inline=1;width=40px\x07");
	send(&mut t, &format!("\x1b]1337;FilePart={first}\x07"));
	send(&mut t, &format!("\x1b]1337;FilePart={rest}\x1b\\"));
	assert!(t.images.is_empty());
	send(&mut t, "\x1b]1337;FileEnd\x07");
	assert_eq!(
		shown(&t)[0].1,
		Extent::Pixels {
			width: 40,
			height: 60
		}
	);
	// Parts and ends without a start do nothing.
	send(
		&mut t,
		&format!("\x1b]1337;FilePart={data}\x07\x1b]1337;FileEnd\x07"),
	);
	assert_eq!(t.images.len(), 1);
	// A new start drops an unfinished file.
	send(&mut t, "\x1b]1337;MultipartFile=inline=1\x07");
	send(&mut t, &format!("\x1b]1337;FilePart={first}\x07"));
	send(&mut t, "\x1b]1337;MultipartFile=inline=1;width=20px\x07");
	send(
		&mut t,
		&format!("\x1b]1337;FilePart={data}\x07\x1b]1337;FileEnd\x07"),
	);
	assert_eq!(
		shown(&t)[1].1,
		Extent::Pixels {
			width: 20,
			height: 30
		}
	);
	// A bad part fails the file.
	send(&mut t, "\x1b]1337;MultipartFile=inline=1\x07");
	send(&mut t, &format!("\x1b]1337;FilePart={data}\x07"));
	send(&mut t, "\x1b]1337;FilePart=!!\x07\x1b]1337;FileEnd\x07");
	assert_eq!(t.images.len(), 2);
	assert!(t.iterm.upload.is_none());
}

#[test]
fn a_gif_inline_image_shows_its_first_frame() {
	// A 1×1 white GIF: one cell, as a PNG of that size would be.
	let gif = "R0lGODlhAQABAIAAAP///wAAACwAAAAAAQABAAACAkQBADs=";
	let mut t = terminal(10, 5);
	send(&mut t, &format!("\x1b]1337;File=inline=1:{gif}\x07"));
	let shown = shown(&t);
	assert_eq!(shown.len(), 1);
	assert_eq!((shown[0].0, shown[0].2), ((0, 0), (1, 1)));
	assert_eq!(t.images[0].bitmap.pixels, [0xffff_ffff]);
	// A damaged GIF shows nothing, as a damaged PNG does.
	let mut t = terminal(10, 5);
	send(&mut t, "\x1b]1337;File=inline=1:R0lGODlhAQABAIAAAP\x07");
	assert!(t.images.is_empty());
}
