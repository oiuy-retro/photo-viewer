use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let icon = out.join("app.ico");
    fs::write(&icon, make_ico()).unwrap();
    let manifest = out.join("app.manifest");
    fs::write(&manifest, MANIFEST).unwrap();

    let mut res = winres::WindowsResource::new();
    res.set_icon(icon.to_str().unwrap());
    res.set_manifest_file(manifest.to_str().unwrap());
    res.set("FileDescription", "Photo Viewer");
    res.set("ProductName", "Photo Viewer");
    res.set("OriginalFilename", "Photo Viewer.exe");
    res.set("LegalCopyright", "");
    res.compile().expect("embed the icon and manifest");
    // GNU ld drops the icon unless the resource archive is pulled in whole.
    let lib = out.join("libresource.a");
    println!("cargo:rustc-link-arg-bins=-Wl,--whole-archive");
    println!("cargo:rustc-link-arg-bins={}", lib.display());
    println!("cargo:rustc-link-arg-bins=-Wl,--no-whole-archive");
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" name="Photo.Viewer" type="win32"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;

fn make_ico() -> Vec<u8> {
    let images = [(16u32, paint(16)), (32, paint(32)), (48, paint(48)), (256, paint(256))];
    let mut file = Vec::new();
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + images.len() * 16;
    let mut blobs = Vec::new();
    for (size, pixels) in &images {
        let size = *size;
        let xor = size * size * 4;
        let mask_stride = ((size + 31) / 32) * 4;
        let mask = mask_stride * size;
        let bytes = 40 + xor + mask;
        file.push(if size == 256 { 0 } else { size as u8 });
        file.push(if size == 256 { 0 } else { size as u8 });
        file.push(0);
        file.push(0);
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&(bytes as u32).to_le_bytes());
        file.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += bytes as usize;
        blobs.push(dib(pixels, size));
    }
    for blob in blobs {
        file.extend_from_slice(&blob);
    }
    file
}

fn dib(pixels: &[u8], size: u32) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(&40u32.to_le_bytes());
    blob.extend_from_slice(&(size as i32).to_le_bytes());
    blob.extend_from_slice(&((size as i32) * 2).to_le_bytes());
    blob.extend_from_slice(&1u16.to_le_bytes());
    blob.extend_from_slice(&32u16.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    blob.extend_from_slice(&0i32.to_le_bytes());
    blob.extend_from_slice(&0i32.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    let stride = size as usize;
    for y in (0..stride).rev() {
        let start = y * stride * 4;
        blob.extend_from_slice(&pixels[start..start + stride * 4]);
    }
    let mask_stride = ((size + 31) / 32) * 4;
    for _ in 0..size {
        blob.extend(std::iter::repeat(0).take(mask_stride as usize));
    }
    blob
}

fn paint(size: usize) -> Vec<u8> {
    let mut px = vec![0u8; size * size * 4];
    let s = size as i32;
    let mut put = |x: i32, y: i32, r: u8, g: u8, b: u8, a: u8| {
        if x < 0 || y < 0 || x >= s || y >= s {
            return;
        }
        let i = (y as usize * size + x as usize) * 4;
        px[i] = b;
        px[i + 1] = g;
        px[i + 2] = r;
        px[i + 3] = a;
    };

    for y in 0..s {
        for x in 0..s {
            put(x, y, 0, 0, 0, 0);
        }
    }

    let m = (s / 14).max(1);
    let left = m;
    let top = m;
    let right = s - m;
    let bottom = s - m;
    let radius = (s / 8).max(2);

    for y in top..bottom {
        for x in left..right {
            let dx0 = x - left;
            let dy0 = y - top;
            let dx1 = right - 1 - x;
            let dy1 = bottom - 1 - y;
            let corner = (dx0 < radius && dy0 < radius && (radius - dx0) * (radius - dx0) + (radius - dy0) * (radius - dy0) > radius * radius)
                || (dx1 < radius && dy0 < radius && (radius - dx1) * (radius - dx1) + (radius - dy0) * (radius - dy0) > radius * radius)
                || (dx0 < radius && dy1 < radius && (radius - dx0) * (radius - dx0) + (radius - dy1) * (radius - dy1) > radius * radius)
                || (dx1 < radius && dy1 < radius && (radius - dx1) * (radius - dx1) + (radius - dy1) * (radius - dy1) > radius * radius);
            if corner {
                continue;
            }
            put(x, y, 0, 51, 153, 255);
        }
    }

    let inset = (s / 10).max(2);
    let il = left + inset;
    let it = top + inset;
    let ir = right - inset;
    let ib = bottom - inset;
    for y in it..ib {
        let t = (y - it) as f32 / (ib - it).max(1) as f32;
        let sky_r = (140.0 + t * 70.0) as u8;
        let sky_g = (180.0 + t * 40.0) as u8;
        let sky_b = 230u8;
        for x in il..ir {
            put(x, y, sky_r, sky_g, sky_b, 255);
        }
    }

    let sun_r = (s / 7).max(2);
    let sun_x = il + (ir - il) / 4;
    let sun_y = it + (ib - it) / 4;
    for y in (sun_y - sun_r)..(sun_y + sun_r) {
        for x in (sun_x - sun_r)..(sun_x + sun_r) {
            let dx = x - sun_x;
            let dy = y - sun_y;
            if dx * dx + dy * dy <= sun_r * sun_r {
                put(x, y, 255, 204, 0, 255);
            }
        }
    }

    let ground = it + (ib - it) * 5 / 9;
    for y in ground..ib {
        for x in il..ir {
            let hill = ((x - il) * 3 / (ir - il).max(1)) - 1;
            let rise = ib - y;
            let height = (s / 4) - hill.abs() * (s / 14);
            if rise < height.max(3) {
                let g = 80 + ((y - ground) * 40 / (ib - ground).max(1)).clamp(0, 40);
                put(x, y, 19, (g + 40) as u8, 13, 255);
            }
        }
    }

    for x in il..ir {
        put(x, it, 0, 51, 153, 255);
        put(x, ib - 1, 0, 51, 153, 255);
    }
    for y in it..ib {
        put(il, y, 0, 51, 153, 255);
        put(ir - 1, y, 0, 51, 153, 255);
    }
    px
}
