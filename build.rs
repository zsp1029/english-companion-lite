// Embed the application identity and Windows visual-styles manifest without a
// resource-compiler dependency. Both Rust Windows toolchains are supported.
use std::{env, fs, path::PathBuf};
fn u16le(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn align(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
fn main() {
    println!("cargo:rerun-if-changed=assets/companion.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = fs::read("assets/companion.ico").unwrap();
    let mut resources: Vec<(u16, u16, Vec<u8>)> = vec![];
    let mut group = vec![0, 0, 1, 0, 3, 0];
    for i in 0..3 {
        let e = &ico[6 + i * 16..22 + i * 16];
        let len = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
        let pos = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
        group.extend(&e[..12]);
        u16le(&mut group, i as u16 + 1);
        resources.push((3, i as u16 + 1, ico[pos..pos + len].to_vec()));
    }
    resources.push((14, 1, group));
    resources.push((24, 1, fs::read("assets/app.manifest").unwrap()));
    let dest = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let mut res = vec![0; 32];
        res[4] = 32;
        res[8] = 255;
        res[9] = 255;
        res[12] = 255;
        res[13] = 255;
        for (kind, id, data) in resources {
            u32le(&mut res, data.len() as u32);
            u32le(&mut res, 32);
            u16le(&mut res, 0xffff);
            u16le(&mut res, kind);
            u16le(&mut res, 0xffff);
            u16le(&mut res, id);
            u32le(&mut res, 0);
            u16le(&mut res, 0x1030);
            u16le(&mut res, 0x409);
            u32le(&mut res, 0);
            u32le(&mut res, 0);
            res.extend(data);
            align(&mut res);
        }
        let path = dest.join("identity.res");
        fs::write(&path, res).unwrap();
        println!("cargo:rustc-link-arg-bins={}", path.display());
    } else {
        // Three resource types, five names, five languages, five data entries.
        let mut section = vec![0u8; 344];
        let put = |buf: &mut Vec<u8>, at: usize, n: u32| {
            buf[at..at + 4].copy_from_slice(&n.to_le_bytes())
        };
        let dir = |buf: &mut Vec<u8>, at: usize, count: u16| {
            buf[at + 14..at + 16].copy_from_slice(&count.to_le_bytes())
        };
        dir(&mut section, 0, 3);
        for (i, (kind, offset, count)) in [(3, 40, 3), (14, 80, 1), (24, 104, 1)]
            .into_iter()
            .enumerate()
        {
            put(&mut section, 16 + i * 8, kind);
            put(&mut section, 20 + i * 8, 0x80000000 | offset as u32);
            dir(&mut section, offset, count);
        }
        let names = [56, 64, 72, 96, 120];
        let mut relocations = vec![];
        for (i, (_, id, data)) in resources.into_iter().enumerate() {
            let lang = 128 + i * 24;
            let entry = 248 + i * 16;
            put(&mut section, names[i], id as u32);
            put(&mut section, names[i] + 4, 0x80000000 | lang as u32);
            dir(&mut section, lang, 1);
            put(&mut section, lang + 16, 0x409);
            put(&mut section, lang + 20, entry as u32);
            let pos = section.len();
            put(&mut section, entry, pos as u32);
            put(&mut section, entry + 4, data.len() as u32);
            section.extend(data);
            align(&mut section);
            relocations.push(entry as u32);
        }
        let mut obj = vec![];
        u16le(&mut obj, 0x8664);
        u16le(&mut obj, 1);
        u32le(&mut obj, 0);
        u32le(&mut obj, 60 + section.len() as u32 + 50);
        u32le(&mut obj, 1);
        u16le(&mut obj, 0);
        u16le(&mut obj, 0);
        obj.extend(b".rsrc\0\0\0");
        u32le(&mut obj, 0);
        u32le(&mut obj, 0);
        u32le(&mut obj, section.len() as u32);
        u32le(&mut obj, 60);
        u32le(&mut obj, 60 + section.len() as u32);
        u32le(&mut obj, 0);
        u16le(&mut obj, 5);
        u16le(&mut obj, 0);
        u32le(&mut obj, 0x40300040);
        obj.extend(section);
        for at in relocations {
            u32le(&mut obj, at);
            u32le(&mut obj, 0);
            u16le(&mut obj, 3);
        }
        obj.extend(b".rsrc\0\0\0");
        u32le(&mut obj, 0);
        u16le(&mut obj, 1);
        u16le(&mut obj, 0);
        obj.extend([3, 0]);
        u32le(&mut obj, 4);
        let path = dest.join("identity.o");
        fs::write(&path, obj).unwrap();
        println!("cargo:rustc-link-arg-bins={}", path.display());
    }
}
