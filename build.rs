use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;

use clap::Shell;

#[path = "src/bin/sp/app.rs"]
mod app;

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR should be set");
    fs::create_dir_all(&out_dir).expect(&format!(
        "couldn't create output directory {}",
        out_dir.to_string_lossy()
    ));

    generate_man_page(&out_dir).expect("couldn't generate manpage");

    let mut app = app::app();
    app.gen_completions("sp", Shell::Bash, &out_dir);
    app.gen_completions("sp", Shell::Fish, &out_dir);
    app.gen_completions("sp", Shell::Zsh, &out_dir);
}

fn generate_man_page(out_dir: impl AsRef<Path>) -> Result<(), io::Error> {
    // If asciidoc isn't installed, then don't do anything.
    if let Err(err) = process::Command::new("a2x").output() {
        eprintln!("Could not run 'a2x' binary, skipping man page generation.");
        eprintln!("Error from running 'a2x': {}", err);
        return Ok(());
    }
    // 1. Read asciidoc template.
    // 2. Interpolate template with auto-generated docs.
    // 3. Save interpolation to disk.
    // 4. Use a2x (part of asciidoc) to convert to man page.
    let out_dir = out_dir.as_ref();
    let cwd = env::current_dir()?;
    let tpl_path = cwd.join("doc").join("sp.1.txt.tpl");
    let txt_path = out_dir.join("sp.1.txt");

    let mut tpl = String::new();
    File::open(&tpl_path)?.read_to_string(&mut tpl)?;
    tpl = tpl.replace("{VERSION}", env!("CARGO_PKG_VERSION"));

    File::create(&txt_path)?.write_all(tpl.as_bytes())?;
    let result = process::Command::new("a2x")
        .arg("--no-xmllint")
        .arg("--doctype")
        .arg("manpage")
        .arg("--format")
        .arg("manpage")
        .arg(&txt_path)
        .spawn()?
        .wait()?;
    if !result.success() {
        eprintln!("'a2x' failed with exit code {:?}", result.code());
    }
    Ok(())
}
