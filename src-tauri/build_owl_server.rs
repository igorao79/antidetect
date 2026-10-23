//! Packs the Owl server's sources (`donut-sync/`) into `OUT_DIR/owl-server.tar`
//! for `workspace_deploy.rs`, which builds the server on a VPS from them. The
//! layout is the repository's, so the server's Dockerfile and compose file
//! work there unchanged.

use std::path::Path;

/// The build context `donut-sync/docker-compose.yml` uses, as listed in the
/// repository's `.dockerignore`, plus the two files that run it.
const FILES: &[&str] = &[
  ".dockerignore",
  "donut-sync/package.json",
  "donut-sync/tsconfig.json",
  "donut-sync/tsconfig.build.json",
  "donut-sync/Dockerfile",
  "donut-sync/docker-compose.yml",
];
const SOURCES: &str = "donut-sync/src";

pub fn write(out_dir: &Path) {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("src-tauri sits in the repository")
    .to_path_buf();
  let mut entries: Vec<String> = FILES.iter().map(|f| f.to_string()).collect();
  collect(&root, &root.join(SOURCES), &mut entries);
  entries.sort();

  let file = std::fs::File::create(out_dir.join("owl-server.tar")).expect("create owl-server.tar");
  let mut tar = tar::Builder::new(file);
  for entry in &entries {
    let bytes = std::fs::read(root.join(entry)).unwrap_or_else(|e| panic!("read {entry}: {e}"));
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    tar
      .append_data(&mut header, entry, bytes.as_slice())
      .unwrap_or_else(|e| panic!("pack {entry}: {e}"));
  }
  tar.finish().expect("finish owl-server.tar");

  for file in FILES {
    println!("cargo:rerun-if-changed=../{file}");
  }
  println!("cargo:rerun-if-changed=../{SOURCES}");
}

fn collect(root: &Path, dir: &Path, entries: &mut Vec<String>) {
  let Ok(read) = std::fs::read_dir(dir) else {
    return;
  };
  for entry in read.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect(root, &path, entries);
    } else if path.extension().is_some_and(|e| e == "ts")
      && !path.to_string_lossy().ends_with(".spec.ts")
    {
      let relative = path.strip_prefix(root).expect("under the repository");
      entries.push(relative.to_string_lossy().replace('\\', "/"));
    }
  }
}
