use windows::{
    Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::{BOOL, Result},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    On,
    Muted,
    Unknown,
}
impl State {
    pub fn from_mute(muted: Option<bool>) -> Self {
        match muted {
            Some(false) => Self::On,
            Some(true) => Self::Muted,
            None => Self::Unknown,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::On => "เปิดไมค์ (On)",
            Self::Muted => "ปิดไมค์ (Muted)",
            Self::Unknown => "ไม่พบไมค์ / อ่านสถานะไม่ได้",
        }
    }
}

const SIZE: usize = 32;
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32), width: f32) -> bool {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a.0 - t * dx).hypot(y - a.1 - t * dy) <= width / 2.0
}
fn color(x: f32, y: f32, state: State) -> Option<[u8; 3]> {
    let body = (12.0..=20.0).contains(&x) && (7.0..=15.0).contains(&y)
        || (x - 16.0).hypot(y - 7.0) <= 4.0
        || (x - 16.0).hypot(y - 15.0) <= 4.0;
    let arc = y >= 15.0 && ((x - 16.0).hypot(y - 15.0) - 9.0).abs() <= 1.5;
    let stand = segment(x, y, (7.0, 12.0), (7.0, 15.0), 3.0)
        || segment(x, y, (25.0, 12.0), (25.0, 15.0), 3.0)
        || segment(x, y, (16.0, 24.0), (16.0, 28.0), 3.0)
        || segment(x, y, (11.0, 29.0), (21.0, 29.0), 3.0);
    let ink = match state {
        State::On => [34, 197, 94],
        State::Muted => [244, 63, 82],
        State::Unknown => [148, 163, 184],
    };
    if state == State::Muted {
        // Dark border separates the slash from the mic, so shape also conveys mute.
        if segment(x, y, (5.0, 4.0), (27.0, 28.0), 3.2) {
            return Some(ink);
        }
        if segment(x, y, (5.0, 4.0), (27.0, 28.0), 6.0) {
            return Some([25, 29, 38]);
        }
    }
    if state == State::Unknown && (x - 24.0).hypot(y - 24.0) <= 7.0 {
        return Some(
            if segment(x, y, (24.0, 20.0), (24.0, 24.0), 1.8) || (x - 24.0).hypot(y - 27.0) < 1.0 {
                [25, 29, 38]
            } else {
                [250, 204, 21]
            },
        );
    }
    (body || arc || stand).then_some(ink)
}

// Four samples per axis give clean edges at the small tray size. BGRA is premultiplied.
fn pixels(state: State) -> Vec<u8> {
    let mut pixels = vec![0; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut sum = [0u32; 3];
            let mut count = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    if let Some(rgb) = color(
                        x as f32 + (sx as f32 + 0.5) / 4.0,
                        y as f32 + (sy as f32 + 0.5) / 4.0,
                        state,
                    ) {
                        for c in 0..3 {
                            sum[c] += rgb[c] as u32;
                        }
                        count += 1;
                    }
                }
            }
            let p = (y * SIZE + x) * 4;
            pixels[p] = (sum[2] / 16) as u8;
            pixels[p + 1] = (sum[1] / 16) as u8;
            pixels[p + 2] = (sum[0] / 16) as u8;
            pixels[p + 3] = (count * 255 / 16) as u8;
        }
    }
    pixels
}

fn create(state: State) -> Result<HICON> {
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE as i32,
                biHeight: -(SIZE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(pixels(state).as_ptr(), bits.cast(), SIZE * SIZE * 4);
        let mask_bits = [0u8; SIZE * SIZE / 8];
        let mask = CreateBitmap(
            SIZE as i32,
            SIZE as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        );
        if mask.is_invalid() {
            let _ = DeleteObject(bitmap.into());
            return Err(windows::core::Error::from_thread());
        }
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: BOOL(1),
            hbmColor: bitmap,
            hbmMask: mask,
            ..Default::default()
        });
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

pub struct Icons {
    on: HICON,
    muted: HICON,
    unknown: HICON,
}
impl Icons {
    pub fn new() -> Result<Self> {
        let mut icons = Self {
            on: HICON::default(),
            muted: HICON::default(),
            unknown: HICON::default(),
        };
        icons.on = create(State::On)?;
        icons.muted = create(State::Muted)?;
        icons.unknown = create(State::Unknown)?;
        Ok(icons)
    }
    pub fn get(&self, state: State) -> HICON {
        match state {
            State::On => self.on,
            State::Muted => self.muted,
            State::Unknown => self.unknown,
        }
    }
}
impl Drop for Icons {
    fn drop(&mut self) {
        unsafe {
            for icon in [self.on, self.muted, self.unknown] {
                if !icon.is_invalid() {
                    let _ = DestroyIcon(icon);
                }
            }
        }
    }
}

// Export the exact native artwork as portable .ico assets, also used for visual QA.
pub fn export(folder: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    for (name, state) in [
        ("mic-on", State::On),
        ("mic-muted", State::Muted),
        ("mic-unknown", State::Unknown),
    ] {
        let mut dib = vec![0u8; 40];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&(SIZE as i32).to_le_bytes());
        dib[8..12].copy_from_slice(&((SIZE * 2) as i32).to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        let pixels = pixels(state);
        for row in (0..SIZE).rev() {
            dib.extend_from_slice(&pixels[row * SIZE * 4..(row + 1) * SIZE * 4]);
        }
        dib.resize(dib.len() + SIZE * SIZE / 8, 0);
        let mut ico = vec![0, 0, 1, 0, 1, 0, 32, 32, 0, 0, 1, 0, 32, 0];
        ico.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&dib);
        std::fs::write(folder.join(format!("{name}.ico")), ico)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_input_never_shows_as_open() {
        assert_eq!(State::from_mute(None), State::Unknown);
        assert_eq!(State::from_mute(Some(true)), State::Muted);
        assert_eq!(State::from_mute(Some(false)), State::On);
    }
}
