//! App icons for the list (PLAN.md §14.2): the icon Windows shows for a
//! program, read from its .exe and handed to the page as a PNG data URL.
//!
//! The page asks by exe path (the service's `AppInfo::exe`). Answers,
//! including "no icon", are cached for the app's lifetime: an exe's icon
//! doesn't change while it runs, and the list asks again on every tick.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use base64::Engine as _;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, HGDIOBJ, ReleaseDC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
};

/// Pixels per side: sharp at 40 px on a 150% display.
const SIZE: i32 = 64;

/// Exe path → data URL, or `None` when it has no icon.
#[derive(Default)]
pub struct Icons(Mutex<HashMap<String, Option<String>>>);

/// The program's icon as `data:image/png;base64,…`, or `None`. Never an
/// error (an async command with a borrowed State must return a Result).
#[tauri::command]
pub async fn app_icon(
    icons: tauri::State<'_, Icons>,
    exe: String,
) -> Result<Option<String>, String> {
    if let Some(hit) = icons
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&exe)
    {
        return Ok(hit.clone());
    }
    let path = exe.clone();
    let url = tauri::async_runtime::spawn_blocking(move || data_url(&path))
        .await
        .ok()
        .flatten();
    icons
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(exe, url.clone());
    Ok(url)
}

fn data_url(exe: &str) -> Option<String> {
    if !exe.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let (w, h, rgba) = extract(exe)?;
    let png = encode_png(w, h, &rgba)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

/// The exe's first icon at [`SIZE`], as straight RGBA rows top to bottom.
fn extract(exe: &str) -> Option<(u32, u32, Vec<u8>)> {
    let mut name = [0u16; 260];
    let wide: Vec<u16> = exe.encode_utf16().collect();
    if wide.len() >= name.len() {
        return None;
    }
    name[..wide.len()].copy_from_slice(&wide);
    let mut handles = [HICON::default()];
    // SAFETY: `name` is NUL-terminated (zeroed past the path); `handles`
    // has room for the one icon asked for.
    let got = unsafe { PrivateExtractIconsW(&name, 0, SIZE, SIZE, Some(&mut handles), None, 0) };
    let icon = handles[0];
    if got == 0 || got == u32::MAX || icon.is_invalid() {
        return None;
    }
    let out = pixels(icon);
    // SAFETY: the icon came from PrivateExtractIconsW and is ours to free.
    unsafe {
        let _ = DestroyIcon(icon);
    }
    out
}

fn pixels(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = ICONINFO::default();
    // SAFETY: `info` is a valid out pointer; the bitmaps it returns are
    // ours and deleted below.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let color = read_bitmap(info.hbmColor);
    let mask = read_bitmap(info.hbmMask);
    // SAFETY: both handles came from GetIconInfo (a null one is ignored).
    unsafe {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    let (w, h, mut bgra) = color?;
    // Old icons carry no alpha: take transparency from the mask instead.
    if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        let (_, _, m) = mask?;
        for (p, q) in bgra
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(m.as_chunks::<4>().0)
        {
            p[3] = if q[0] == 0 { 255 } else { 0 };
        }
    }
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    Some((w, h, bgra))
}

/// A bitmap as 32-bit top-down BGRA.
fn read_bitmap(bmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    if bmp.is_invalid() {
        return None;
    }
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a BITMAP-sized out buffer.
    let n = unsafe {
        GetObjectW(
            HGDIOBJ(bmp.0),
            size_of::<BITMAP>() as i32,
            Some((&raw mut bm).cast()),
        )
    };
    let (w, h) = (
        u32::try_from(bm.bmWidth).ok()?,
        u32::try_from(bm.bmHeight).ok()?,
    );
    if n == 0 || w == 0 || h == 0 || w > 256 || h > 256 {
        return None;
    }
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bm.bmWidth,
            biHeight: -bm.bmHeight,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    // SAFETY: the screen DC is released below; `buf` holds w×h 32-bit
    // pixels, which is what `bi` asks for.
    let lines = unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bmp,
            0,
            h,
            Some(buf.as_mut_ptr().cast()),
            &mut bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    (lines == h as i32).then_some((w, h, buf))
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_exe_has_an_icon() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        let url = data_url(&format!(r"{windir}\explorer.exe")).expect("explorer has an icon");
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn non_exe_paths_and_missing_files_have_none() {
        assert_eq!(data_url(r"C:\Windows\win.ini"), None);
        assert_eq!(data_url(r"C:\nope\missing.exe"), None);
    }
}
