use macroquad::prelude::{Color, Image, clear_background, draw_rectangle};

const BACKGROUND: [u8; 4] = [4, 6, 14, 255];
const TOP_LEFT: [u8; 4] = [235, 55, 75, 255];
const TOP_RIGHT: [u8; 4] = [55, 220, 115, 255];
const BOTTOM_LEFT: [u8; 4] = [55, 115, 235, 255];
const BOTTOM_RIGHT: [u8; 4] = [240, 205, 55, 255];
const SAMPLE_POINTS: [(&str, f32, f32, [u8; 4]); 4] = [
    ("top-left", 0.08, 0.08, TOP_LEFT),
    ("top-right", 0.92, 0.08, TOP_RIGHT),
    ("bottom-left", 0.08, 0.92, BOTTOM_LEFT),
    ("bottom-right", 0.92, 0.92, BOTTOM_RIGHT),
];

pub fn draw_orientation_probe(width: f32, height: f32) {
    clear_background(rgba(BACKGROUND));
    let marker_width = width * 0.18;
    let marker_height = height * 0.18;
    draw_rectangle(0.0, 0.0, marker_width, marker_height, rgba(TOP_LEFT));
    draw_rectangle(
        width - marker_width,
        0.0,
        marker_width,
        marker_height,
        rgba(TOP_RIGHT),
    );
    draw_rectangle(
        0.0,
        height - marker_height,
        marker_width,
        marker_height,
        rgba(BOTTOM_LEFT),
    );
    draw_rectangle(
        width - marker_width,
        height - marker_height,
        marker_width,
        marker_height,
        rgba(BOTTOM_RIGHT),
    );
}

pub fn validate_orientation(image: &Image) -> Result<(), String> {
    if image.width() < 10 || image.height() < 10 {
        return Err(format!(
            "framebuffer is too small: {}x{}",
            image.width(),
            image.height()
        ));
    }

    for (name, normalized_x, normalized_y, expected) in SAMPLE_POINTS {
        let x = (normalized_x * image.width() as f32) as usize;
        let screen_y = (normalized_y * image.height() as f32) as usize;
        let framebuffer_y = image.height() - screen_y - 1;
        let actual = image.get_image_data()[framebuffer_y * image.width() + x];
        if !approximately_equal(actual, expected) {
            return Err(format!(
                "{name} marker mismatch at screen ({x}, {screen_y}): expected {expected:?}, found {actual:?}"
            ));
        }
    }
    Ok(())
}

fn rgba(bytes: [u8; 4]) -> Color {
    Color::from_rgba(bytes[0], bytes[1], bytes[2], bytes[3])
}

fn approximately_equal(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual
        .into_iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_probe() -> Image {
        let mut image = Image::gen_image_color(100, 100, rgba(BACKGROUND));
        for (.., normalized_x, normalized_y, color) in SAMPLE_POINTS {
            let center_x = (normalized_x * image.width() as f32) as i32;
            let screen_y = (normalized_y * image.height() as f32) as i32;
            let framebuffer_y = image.height() as i32 - screen_y - 1;
            for y in framebuffer_y - 2..=framebuffer_y + 2 {
                for x in center_x - 2..=center_x + 2 {
                    image.set_pixel(x as u32, y as u32, rgba(color));
                }
            }
        }
        image
    }

    #[test]
    fn accepts_correct_corner_orientation() {
        assert!(validate_orientation(&synthetic_probe()).is_ok());
    }

    #[test]
    fn rejects_vertical_reflection() {
        let mut image = synthetic_probe();
        flip_rows(&mut image);
        assert!(validate_orientation(&image).is_err());
    }

    #[test]
    fn rejects_horizontal_reflection() {
        let mut image = synthetic_probe();
        let width = image.width();
        for row in image.get_image_data_mut().chunks_exact_mut(width) {
            row.reverse();
        }
        assert!(validate_orientation(&image).is_err());
    }

    fn flip_rows(image: &mut Image) {
        let row_pixels = image.width();
        for y in 0..image.height() / 2 {
            let top = y * row_pixels;
            let bottom = (image.height() - y - 1) * row_pixels;
            let pixels = image.get_image_data_mut();
            let (upper, lower) = pixels.split_at_mut(bottom);
            upper[top..top + row_pixels].swap_with_slice(&mut lower[..row_pixels]);
        }
    }
}
